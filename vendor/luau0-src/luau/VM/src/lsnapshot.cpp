// RTS-CORE-ENGINE SNAPSHOT: saves a VM's whole heap and loads it into another VM
// of the same build, as moho's save games did for Lua 5.0 (issue #363:
// replay keyframes; later save games).
//
// The loading VM must be at the snapshot's *base*: the same objects and
// protos created in the same order up to `base` gcid and `baseProto` proto
// id (the engine starts the same session the same way and stops before the
// first tick). Base objects are matched by gcid, base protos by proto id,
// and keep the loading VM's own object, with the saved state written over
// what can change: a table's contents (not one holding light userdata,
// process addresses), a Lua closure's upvalues, a coroutine (not the VM's
// own threads, gcid <= `internal`: the main thread, mlua's), a buffer's
// bytes, and the registry's string-keyed entries (mlua keeps its own under
// light userdata and integer keys). Native closures, userdata and protos
// don't change. Everything newer is written in full and recreated with its
// gcid, so table order (which hashes object keys by gcid) and every later
// gcid are the same after a load as they would have been.
//
// Saving reads the heap only (no allocation, so the saving game carries on
// unchanged) and is split into parts the engine writes in parallel.
//
// Format (version 1): little-endian, varints (LEB128; signed ones zigzag).
//   header     magic, version, base, internal, gcidnext, baseProto,
//              protoidnext, rngstate, the basic types' metatables
//   strings    every live string, by (hash, bytes); a reference is its hash
//              (plus its rank when hashes collide), so one new string does
//              not renumber the rest
//   protos     those newer than the base, by proto id
//   upvalues   numbered by first reference from closures and coroutines
//              in gcid order
//   objects    everything with a gcid, in gcid order, each record prefixed
//              with its length; written in parts
#include "lua.h"
#include "lobject.h"
#include "lstate.h"
#include "ltable.h"
#include "lfunc.h"
#include "lmem.h"
#include "lgc.h"
#include "lapi.h"
#include "lstring.h"
#include "ludata.h"
#include "lbuffer.h"

#include <algorithm>
#include <cstring>
#include <mutex>
#include <set>
#include <string>
#include <unordered_map>
#include <vector>

namespace
{

const uint32_t kMagic = 0x504e534c; // "LSNP"
const uint32_t kVersion = 2;

// Base objects: the loading VM's kept, the saved state written over it, or
// (the registry) its string-keyed entries.
const uint8_t kBaseKeep = 0;
const uint8_t kBaseContents = 1;
const uint8_t kBaseRegistry = 2;

// Userdata newer than the base: bytes here, or made by the engine (its
// payload is the engine's to save).
const uint8_t kUdataRaw = 0;
const uint8_t kUdataExternal = 1;

typedef void (*Sink)(void* context, const uint8_t* data, size_t size);

// Native function addresses are written relative to one of the VM's own, so
// they survive another process of the same build (ASLR moves the image as a
// whole).
intptr_t anchor()
{
    return reinterpret_cast<intptr_t>(&luaH_new);
}

struct Buf
{
    std::vector<uint8_t> b;
    void u8(uint8_t v)
    {
        b.push_back(v);
    }
    void u32(uint32_t v)
    {
        raw(&v, 4);
    }
    void u64(uint64_t v)
    {
        raw(&v, 8);
    }
    void var(uint64_t v)
    {
        while (v >= 0x80)
        {
            b.push_back(uint8_t(v) | 0x80);
            v >>= 7;
        }
        b.push_back(uint8_t(v));
    }
    void svar(int64_t v)
    {
        var((uint64_t(v) << 1) ^ uint64_t(v >> 63));
    }
    void raw(const void* p, size_t n)
    {
        const uint8_t* c = (const uint8_t*)p;
        b.insert(b.end(), c, c + n);
    }
    void bytes(const std::vector<uint8_t>& o)
    {
        b.insert(b.end(), o.begin(), o.end());
    }
};

struct Reader
{
    const uint8_t* p;
    const uint8_t* end;
    bool bad = false;

    bool need(size_t n)
    {
        if (size_t(end - p) < n)
        {
            bad = true;
            return false;
        }
        return true;
    }
    uint8_t u8()
    {
        return need(1) ? *p++ : 0;
    }
    uint32_t u32()
    {
        uint32_t v = 0;
        raw(&v, 4);
        return v;
    }
    uint64_t u64()
    {
        uint64_t v = 0;
        raw(&v, 8);
        return v;
    }
    uint64_t var()
    {
        uint64_t v = 0;
        for (int shift = 0; shift < 64; shift += 7)
        {
            uint8_t c = u8();
            v |= uint64_t(c & 0x7f) << shift;
            if (!(c & 0x80))
                return v;
        }
        bad = true;
        return 0;
    }
    int64_t svar()
    {
        uint64_t v = var();
        return int64_t(v >> 1) ^ -int64_t(v & 1);
    }
    void raw(void* out, size_t n)
    {
        if (need(n))
        {
            memcpy(out, p, n);
            p += n;
        }
    }
    const uint8_t* skip(size_t n)
    {
        const uint8_t* at = p;
        if (need(n))
            p += n;
        return at;
    }
};

uint32_t gcidOf(GCObject* o)
{
    switch (o->gch.tt)
    {
    case LUA_TTABLE:
        return gco2h(o)->gcid;
    case LUA_TFUNCTION:
        return gco2cl(o)->gcid;
    case LUA_TUSERDATA:
        return gco2u(o)->gcid;
    case LUA_TBUFFER:
        return gco2buf(o)->gcid;
    case LUA_TTHREAD:
        return gco2th(o)->gcid;
    default:
        return 0;
    }
}

bool hasGcid(int tt)
{
    return tt == LUA_TTABLE || tt == LUA_TFUNCTION || tt == LUA_TUSERDATA || tt == LUA_TBUFFER || tt == LUA_TTHREAD;
}

// One of mlua's error objects (`rts_internaldtor`): left out of keyframes.
bool isInternalUdata(global_State* g, Udata* u)
{
    if (u->tag != UTAG_IDTOR || !g->rts_internaldtor)
        return false;
    void (*dtor)(void*) = nullptr;
    memcpy(&dtor, &u->data + u->len - sizeof(dtor), sizeof(dtor));
    return dtor == g->rts_internaldtor;
}

bool stringLess(TString* a, TString* b)
{
    if (a->hash != b->hash)
        return a->hash < b->hash;
    if (a->len != b->len)
        return a->len < b->len;
    return memcmp(a->data, b->data, a->len) < 0;
}

} // namespace

struct lua_SnapshotWriter
{
    lua_State* L;
    uint32_t base;
    uint32_t internal;
    uint32_t baseProto;
    bool externalUserdata;
    std::vector<GCObject*> objects; // gcid order, the main thread excluded
    std::vector<TString*> strings;  // (hash, bytes) order
    std::unordered_map<TString*, uint32_t> collisionRank;
    std::vector<Proto*> protos; // newer than the base, by proto id
    std::unordered_map<Proto*, bool> protoSeen;
    std::unordered_map<UpVal*, uint32_t> upvalIds;
    std::vector<UpVal*> upvals;
    std::vector<Udata*> external;

    mutable std::mutex errorLock;
    mutable std::string error;

    void fail(const std::string& what) const
    {
        std::lock_guard<std::mutex> lock(errorLock);
        if (error.empty())
            error = what;
    }

    // Notes a proto (and its children) to write, if newer than the base.
    void noteProto(Proto* p)
    {
        if (p->protoid <= baseProto || !protoSeen.emplace(p, true).second)
            return;
        protos.push_back(p);
        for (int i = 0; i < p->sizep; i++)
            noteProto(p->p[i]);
    }

    // The engine's userdata (mlua's: they carry a destructor) when external;
    // other userdata (`newproxy`) are plain bytes.
    bool isExternal(Udata* u) const
    {
        return externalUserdata && u->tag == UTAG_IDTOR;
    }

    // A written coroutine: newer than the base, or a base one that isn't
    // the VM's own.
    bool writesThread(lua_State* th) const
    {
        return th != L->global->mainthread && th->gcid > internal;
    }

    uint32_t upvalId(UpVal* uv)
    {
        auto it = upvalIds.find(uv);
        if (it != upvalIds.end())
            return it->second;
        uint32_t id = uint32_t(upvals.size()) + 1;
        upvalIds.emplace(uv, id);
        upvals.push_back(uv);
        return id;
    }

    // Ids from the begin pass (0 and an error if a reference was missed).
    uint32_t knownUpval(UpVal* uv) const
    {
        auto it = upvalIds.find(uv);
        if (it != upvalIds.end())
            return it->second;
        fail("an upvalue no closure or thread names");
        return 0;
    }

    uint32_t knownProto(Proto* p) const
    {
        if (p->protoid > baseProto && !protoSeen.count(p))
            fail("a proto no closure names");
        return p->protoid;
    }

    void str(Buf& b, TString* s) const
    {
        if (!std::binary_search(strings.begin(), strings.end(), s, stringLess))
            fail("string not in the heap walk: '" + std::string(s->data, std::min<size_t>(s->len, 60)) + "'");
        b.var(s->hash);
        auto it = collisionRank.find(s);
        if (it != collisionRank.end())
            b.var(it->second);
    }

    void optStr(Buf& b, TString* s) const
    {
        b.u8(s != nullptr);
        if (s)
            str(b, s);
    }

    void ref(Buf& b, GCObject* o) const
    {
        b.var(o ? uint64_t(gcidOf(o)) + 1 : 0);
    }

    // `light`: set when the value is a light userdata (a process address).
    void value(Buf& b, int tt, const Value& v, const int* extra, bool& light) const
    {
        b.u8(uint8_t(tt));
        switch (tt)
        {
        case LUA_TNIL:
        case LUA_TDEADKEY: // its key was collected; nothing can name it again
            break;
        case LUA_TBOOLEAN:
            b.u8(uint8_t(v.b));
            break;
        case LUA_TNUMBER:
            b.raw(&v.n, 8);
            break;
        case LUA_TVECTOR:
            b.raw(v.v, 8);
            b.raw(extra, 4);
            break;
        case LUA_TLIGHTUSERDATA:
            light = true;
            b.raw(&v.p, 8);
            b.u8(uint8_t(extra[0]));
            break;
        case LUA_TSTRING:
            str(b, &v.gc->ts);
            break;
        case LUA_TUPVAL:
            b.var(knownUpval(&v.gc->uv));
            break;
        case LUA_TUSERDATA:
            if (isInternalUdata(L->global, &v.gc->u))
            {
                // An mlua error a script kept: not in keyframes (nil).
                b.b.back() = LUA_TNIL;
                break;
            }
            b.var(uint64_t(gcidOf(v.gc)) + 1);
            break;
        default:
            if (gcidOf(v.gc) == 0 && v.gc != obj2gco(L->global->mainthread))
                fail("an object made while gcids were frozen (snapshot building) is still referenced");
            b.var(uint64_t(gcidOf(v.gc)) + 1);
            break;
        }
    }

    void tv(Buf& b, const TValue* o, bool& light) const
    {
        value(b, o->tt, o->value, o->extra, light);
    }

    void table(Buf& b, LuaTable* h, bool& light) const
    {
        ref(b, (GCObject*)h->metatable);
        b.u8(h->readonly);
        b.u8(h->safeenv);
        b.var(h->sizearray);
        b.svar(h->lastfree);
        bool dummy = h->node == &luaH_dummynode;
        b.u8(dummy ? 0xff : h->lsizenode);
        for (int i = 0; i < h->sizearray; i++)
            tv(b, &h->array[i], light);
        if (dummy)
            return;
        int n = sizenode(h);
        for (int i = 0; i < n; i++)
        {
            const LuaNode* node = &h->node[i];
            value(b, node->key.tt, node->key.value, node->key.extra, light);
            b.svar(node->key.next);
            tv(b, &node->val, light);
        }
    }

    void closure(Buf& b, Closure* cl, bool& light) const
    {
        b.u8(cl->isC);
        b.u8(cl->nupvalues);
        b.u8(cl->stacksize);
        b.u8(cl->preload);
        ref(b, (GCObject*)cl->env);
        if (cl->isC)
        {
            b.svar(reinterpret_cast<intptr_t>(cl->c.f) - anchor());
            b.u8(cl->c.cont != nullptr);
            if (cl->c.cont)
                b.svar(reinterpret_cast<intptr_t>(cl->c.cont) - anchor());
            size_t len = cl->c.debugname ? strlen(cl->c.debugname) : 0;
            b.var(cl->c.debugname ? len + 1 : 0);
            b.raw(cl->c.debugname, len);
            for (int i = 0; i < cl->nupvalues; i++)
                tv(b, &cl->c.upvals[i], light);
        }
        else
        {
            b.var(knownProto(cl->l.p));
            for (int i = 0; i < cl->nupvalues; i++)
                tv(b, &cl->l.uprefs[i], light);
        }
    }

    // A coroutine: its frames, then its stack's values. Not its stack's
    // capacity (the collector shrinks idle stacks, unseen by scripts): the
    // loader sizes the stack to what the frames use.
    void thread(Buf& b, lua_State* th, bool& light) const
    {
        b.u8(th->status);
        b.u8(th->singlestep);
        ref(b, (GCObject*)th->gt);
        b.var(th->top - th->stack);
        b.var(th->base - th->stack);
        b.var(th->ci - th->base_ci);
        b.var(th->nCcalls);
        b.var(th->baseCcalls);
        // Not `cachedslot`: a table lookup's hint, unseen by scripts.
        optStr(b, th->namecall);
        for (CallInfo* ci = th->base_ci; ci <= th->ci; ci++)
        {
            b.var(ci->base - th->stack);
            b.var(ci->func - th->stack);
            b.var(ci->top - th->stack);
            uint64_t pc = 0;
            if (ci->savedpc && ttisfunction(ci->func) && !clvalue(ci->func)->isC)
                pc = uint64_t(ci->savedpc - clvalue(ci->func)->l.p->code) + 1;
            b.var(pc);
            // A new thread's first frame never has these set (and nothing
            // reads them): zeros, not whatever the memory held.
            b.svar(ci == th->base_ci ? 0 : ci->nresults);
            b.var(ci == th->base_ci ? 0 : ci->flags);
        }
        for (StkId s = th->stack; s < th->top; s++)
            tv(b, s, light);
    }

    // What of a base object can have changed since the base.
    void baseObject(Buf& b, GCObject* o, bool& light) const
    {
        switch (o->gch.tt)
        {
        case LUA_TTABLE:
        {
            LuaTable* h = gco2h(o);
            if (h == hvalue(registry(L)))
            {
                registryEntries(b, h, light);
                return;
            }
            Buf contents;
            bool held = false;
            table(contents, h, held);
            b.u8(held ? kBaseKeep : kBaseContents);
            if (!held)
                b.bytes(contents.b);
            return;
        }
        case LUA_TFUNCTION:
        {
            Closure* cl = gco2cl(o);
            if (cl->isC)
            {
                b.u8(kBaseKeep);
                return;
            }
            b.u8(kBaseContents);
            b.var(cl->nupvalues);
            for (int i = 0; i < cl->nupvalues; i++)
                tv(b, &cl->l.uprefs[i], light);
            return;
        }
        case LUA_TTHREAD:
        {
            lua_State* th = gco2th(o);
            if (!writesThread(th))
            {
                b.u8(kBaseKeep);
                return;
            }
            b.u8(kBaseContents);
            thread(b, th, light);
            return;
        }
        case LUA_TBUFFER:
        {
            Buffer* buf = gco2buf(o);
            b.u8(kBaseContents);
            b.var(buf->len);
            b.raw(buf->data, buf->len);
            return;
        }
        default:
            b.u8(kBaseKeep); // userdata: their payloads are fixed
            return;
        }
    }

    // The registry's string-keyed entries (the engine's named tables);
    // mlua's own are under light userdata and integer keys.
    void registryEntries(Buf& b, LuaTable* h, bool& light) const
    {
        b.u8(kBaseRegistry);
        std::vector<const LuaNode*> entries;
        if (h->node != &luaH_dummynode)
            for (int i = 0; i < sizenode(h); i++)
            {
                const LuaNode* n = &h->node[i];
                if (n->key.tt == LUA_TSTRING && !ttisnil(&n->val) && !ttislightuserdata(&n->val))
                    entries.push_back(n);
            }
        // By key: the loading VM's registry lays them out its own way.
        std::sort(entries.begin(), entries.end(), [](const LuaNode* x, const LuaNode* y) {
            return stringLess(&x->key.value.gc->ts, &y->key.value.gc->ts);
        });
        b.var(entries.size());
        for (const LuaNode* n : entries)
        {
            str(b, &n->key.value.gc->ts);
            tv(b, &n->val, light);
        }
    }

    // One object's record: type, gcid, then its kind's fields.
    void object(Buf& b, GCObject* o) const
    {
        int tt = o->gch.tt;
        uint32_t gcid = gcidOf(o);
        b.u8(uint8_t(tt));
        b.var(gcid);
        bool light = false;
        if (gcid <= base || (tt == LUA_TTHREAD && gco2th(o) == L->global->mainthread))
        {
            baseObject(b, o, light);
            if (light)
                fail("a base " + std::string(lua_typename(L, tt)) + " (gcid " + std::to_string(gcid) + ") holds a light userdata");
            return;
        }
        switch (tt)
        {
        case LUA_TTABLE:
            table(b, gco2h(o), light);
            break;
        case LUA_TFUNCTION:
            closure(b, gco2cl(o), light);
            break;
        case LUA_TUSERDATA:
        {
            Udata* u = gco2u(o);
            b.u8(u->tag);
            ref(b, (GCObject*)u->metatable);
            b.var(u->len);
            bool external = isExternal(u);
            b.u8(external ? kUdataExternal : kUdataRaw);
            if (!external)
                b.raw(u->data, u->len);
            break;
        }
        case LUA_TBUFFER:
        {
            Buffer* buf = gco2buf(o);
            b.var(buf->len);
            b.raw(buf->data, buf->len);
            break;
        }
        case LUA_TTHREAD:
            thread(b, gco2th(o), light);
            break;
        }
        if (light)
            fail("a " + std::string(lua_typename(L, tt)) + " (gcid " + std::to_string(gcid) + ") holds a light userdata");
    }

    void proto(Buf& b, Proto* p, bool& light) const
    {
        b.u8(p->nups);
        b.u8(p->numparams);
        b.u8(p->is_vararg);
        b.u8(p->maxstacksize);
        b.u8(p->flags);
        b.var(p->sizecode);
        b.raw(p->code, size_t(p->sizecode) * sizeof(Instruction));
        b.var(p->sizek);
        for (int i = 0; i < p->sizek; i++)
            tv(b, &p->k[i], light);
        b.var(p->sizep);
        for (int i = 0; i < p->sizep; i++)
            b.var(knownProto(p->p[i]));
        b.var(p->sizelineinfo);
        if (p->lineinfo)
        {
            // One delta per instruction, then (aligned) the absolute lines;
            // the alignment gap is uninitialised: written as zeros.
            size_t abs = (uint8_t*)p->abslineinfo - p->lineinfo;
            size_t at = b.b.size();
            b.raw(p->lineinfo, p->sizelineinfo);
            for (size_t i = size_t(p->sizecode); i < abs && i < size_t(p->sizelineinfo); i++)
                b.b[at + i] = 0;
            b.var(abs);
            b.var(p->linegaplog2);
        }
        b.var(p->sizelocvars);
        for (int i = 0; i < p->sizelocvars; i++)
        {
            optStr(b, p->locvars[i].varname);
            b.var(p->locvars[i].startpc);
            b.var(p->locvars[i].endpc);
            b.u8(p->locvars[i].reg);
        }
        b.var(p->sizeupvalues);
        for (int i = 0; i < p->sizeupvalues; i++)
            optStr(b, p->upvalues[i]);
        optStr(b, p->source);
        optStr(b, p->debugname);
        b.svar(p->linedefined);
        b.svar(p->bytecodeid);
        b.var(p->sizetypeinfo);
        if (p->typeinfo)
            b.raw(p->typeinfo, p->sizetypeinfo);
    }
};

namespace
{

bool collectObject(void* context, lua_Page* page, GCObject* gco)
{
    std::vector<GCObject*>* all = (std::vector<GCObject*>*)context;
    all->push_back(gco);
    return false;
}

void emit(Sink sink, void* context, const Buf& b)
{
    sink(context, b.b.data(), b.b.size());
}

} // namespace

// Starts a save: indexes the heap (read only). `base`: the last gcid the
// loading VM will have made itself. `externalUserdata`: userdata newer than
// the base are the engine's (`lua_snapshot_external*`); otherwise their bytes
// are written. Never fails; errors surface from the part writers.
LUA_API lua_SnapshotWriter* lua_snapshot_begin(lua_State* L, unsigned base, unsigned internal, unsigned baseProto, int externalUserdata)
{
    lua_SnapshotWriter* w = new lua_SnapshotWriter();
    w->L = L;
    w->base = base;
    w->internal = internal;
    w->baseProto = baseProto;
    w->externalUserdata = externalUserdata != 0;

    std::vector<GCObject*> all;
    all.reserve(1 << 20);
    luaM_visitgco(L, &all, collectObject);
    for (GCObject* o : all)
    {
        int tt = o->gch.tt;
        if (tt == LUA_TUSERDATA && isInternalUdata(L->global, gco2u(o)))
            continue;
        // Made while gcids were frozen (garbage of the renderer's snapshot
        // building): left out; a reference to one fails the save.
        if (hasGcid(tt) && gcidOf(o) == 0)
            continue;
        if (hasGcid(tt))
            w->objects.push_back(o);
        else if (tt == LUA_TSTRING)
            w->strings.push_back(&o->ts);
    }
    std::sort(w->objects.begin(), w->objects.end(), [](GCObject* a, GCObject* b) { return gcidOf(a) < gcidOf(b); });
    std::sort(w->strings.begin(), w->strings.end(), stringLess);
    for (size_t i = 0; i < w->strings.size();)
    {
        size_t j = i + 1;
        while (j < w->strings.size() && w->strings[j]->hash == w->strings[i]->hash)
            j++;
        if (j - i > 1)
            for (size_t k = i; k < j; k++)
                w->collisionRank.emplace(w->strings[k], uint32_t(k - i));
        i = j;
    }

    // Protos to write; upvalues by first reference, in gcid order.
    for (GCObject* o : w->objects)
    {
        if (o->gch.tt == LUA_TFUNCTION)
        {
            Closure* cl = gco2cl(o);
            if (!cl->isC)
            {
                w->noteProto(cl->l.p);
                for (int i = 0; i < cl->nupvalues; i++)
                    if (ttisupval(&cl->l.uprefs[i]))
                        w->upvalId(upvalue(&cl->l.uprefs[i]));
            }
        }
        else if (o->gch.tt == LUA_TTHREAD)
        {
            if (w->writesThread(gco2th(o)))
                for (UpVal* uv = gco2th(o)->openupval; uv; uv = uv->u.open.threadnext)
                    w->upvalId(uv);
        }
        else if (o->gch.tt == LUA_TUSERDATA && gcidOf(o) > base && w->isExternal(gco2u(o)))
            w->external.push_back(gco2u(o));
    }
    // Protos only a constant names (none expected) fail the part writers
    // (`knownProto`).
    std::sort(w->protos.begin(), w->protos.end(), [](Proto* a, Proto* b) { return a->protoid < b->protoid; });
    return w;
}

LUA_API size_t lua_snapshot_objectcount(lua_SnapshotWriter* w)
{
    return w->objects.size();
}

// Header, strings, protos and upvalues (one part, before the objects).
LUA_API void lua_snapshot_writehead(lua_SnapshotWriter* w, Sink sink, void* context)
{
    lua_State* L = w->L;
    global_State* g = L->global;
    Buf b;
    b.u32(kMagic);
    b.u32(kVersion);
    b.var(w->base);
    b.var(w->internal);
    b.var(g->gcidnext);
    b.var(w->baseProto);
    b.var(g->protoidnext);
    b.u64(g->rngstate);
    for (int i = 0; i < LUA_T_COUNT; i++)
        w->ref(b, (GCObject*)g->mt[i]);

    b.var(w->strings.size());
    for (TString* s : w->strings)
    {
        b.var(s->len);
        b.raw(s->data, s->len);
    }

    bool light = false;
    b.var(w->protos.size());
    for (Proto* p : w->protos)
        b.var(p->protoid);
    for (Proto* p : w->protos)
        w->proto(b, p, light);

    b.var(w->upvals.size());
    for (UpVal* uv : w->upvals)
    {
        bool open = upisopen(uv);
        b.u8(open);
        if (!open)
            w->tv(b, uv->v, light);
    }
    // Open ones: their thread and slot.
    for (GCObject* o : w->objects)
        if (o->gch.tt == LUA_TTHREAD && w->writesThread(gco2th(o)))
        {
            lua_State* th = gco2th(o);
            for (UpVal* uv = th->openupval; uv; uv = uv->u.open.threadnext)
            {
                b.var(w->knownUpval(uv));
                b.var(uint64_t(th->gcid) + 1);
                b.var(uv->v - th->stack);
            }
        }
    b.var(0);
    if (light)
        w->fail("a proto constant or upvalue holds a light userdata");

    b.var(w->objects.size());
    emit(sink, context, b);
}

// Objects [from, to) in gcid order: their records, each prefixed with its
// length. Read only: parts may be written concurrently.
LUA_API void lua_snapshot_writeobjects(lua_SnapshotWriter* w, size_t from, size_t to, Sink sink, void* context)
{
    Buf out;
    Buf record;
    for (size_t i = from; i < to && i < w->objects.size(); i++)
    {
        record.b.clear();
        w->object(record, w->objects[i]);
        out.var(record.b.size());
        out.bytes(record.b);
    }
    emit(sink, context, out);
}

// The first error any part met (empty: none). Valid until the writer ends.
LUA_API const char* lua_snapshot_writeerror(lua_SnapshotWriter* w)
{
    return w->error.c_str();
}

// Userdata newer than the base, when external: how many, and each one's
// gcid; `lua_snapshot_pushexternal` pushes one (no allocation) so the engine
// can save its payload.
LUA_API size_t lua_snapshot_externalcount(lua_SnapshotWriter* w)
{
    return w->external.size();
}

LUA_API unsigned lua_snapshot_externalgcid(lua_SnapshotWriter* w, size_t i)
{
    return w->external[i]->gcid;
}

LUA_API void lua_snapshot_pushexternal(lua_State* L, lua_SnapshotWriter* w, size_t i)
{
    lua_checkstack(L, 1);
    setuvalue(L, L->top, w->external[i]);
    L->top++;
}

LUA_API void lua_snapshot_end(lua_SnapshotWriter* w)
{
    delete w;
}

// ---------------------------------------------------------------------------
// Loading

struct lua_SnapshotReader
{
    lua_State* L;
    std::vector<uint8_t> data;
    std::string error;

    uint32_t base = 0;
    uint32_t internal = 0;
    uint32_t gcidnext = 0;
    uint32_t baseProto = 0;
    uint32_t protoidnext = 0;
    uint64_t rngstate = 0;
    std::vector<uint32_t> metatables;

    std::vector<TString*> strings;
    std::unordered_map<unsigned, std::pair<uint32_t, uint32_t>> byHash; // first index, count
    std::vector<Proto*> protos; // newer than the base, in section order
    std::unordered_map<uint32_t, Proto*> protosById; // base ones too
    std::vector<UpVal*> upvals;
    std::unordered_map<uint32_t, GCObject*> objects; // by gcid, base ones too

    const uint8_t* protoSection = nullptr;
    const uint8_t* upvalSection = nullptr;
    struct Record
    {
        uint8_t tt;
        uint32_t gcid;
        const uint8_t* body;
        const uint8_t* end;
    };
    std::vector<Record> records;
    // External userdata to be made by the engine: gcid, tag, size.
    struct External
    {
        uint32_t gcid;
        uint8_t tag;
        size_t len;
        Udata* made;
    };
    std::vector<External> externals;
    std::set<std::string>* debugnames;
    std::string where = "head";

    void fail(const std::string& what)
    {
        if (error.empty())
            error = what + " (reading " + where + ")";
    }

    TString* str(Reader& r)
    {
        unsigned hash = unsigned(r.var());
        auto it = byHash.find(hash);
        if (it == byHash.end())
        {
            fail("unknown string hash " + std::to_string(hash));
            return nullptr;
        }
        uint32_t index = it->second.first;
        if (it->second.second > 1)
            index += uint32_t(r.var());
        return strings[index];
    }

    TString* optStr(Reader& r)
    {
        return r.u8() ? str(r) : nullptr;
    }

    GCObject* ref(Reader& r)
    {
        uint64_t v = r.var();
        if (v == 0)
            return nullptr;
        auto it = objects.find(uint32_t(v - 1));
        if (it == objects.end())
        {
            fail("missing object gcid " + std::to_string(v - 1));
            return nullptr;
        }
        return it->second;
    }

    void value(Reader& r, TValue* o)
    {
        int tt = r.u8();

        o->tt = tt;
        memset(o->extra, 0, sizeof(o->extra));
        switch (tt)
        {
        case LUA_TNIL:
            o->value.gc = nullptr;
            break;
        case LUA_TDEADKEY:
            o->value.gc = nullptr;
            break;
        case LUA_TBOOLEAN:
            o->value.b = r.u8();
            break;
        case LUA_TNUMBER:
            r.raw(&o->value.n, 8);
            break;
        case LUA_TVECTOR:
            r.raw(o->value.v, 8);
            r.raw(o->extra, 4);
            break;
        case LUA_TLIGHTUSERDATA:
            r.raw(&o->value.p, 8);
            o->extra[0] = r.u8();
            break;
        case LUA_TSTRING:
        {
            TString* ts = str(r); // obj2gco evaluates its argument twice
            o->value.gc = obj2gco(ts);
            break;
        }
        case LUA_TUPVAL:
        {
            uint64_t id = r.var();
            if (id == 0 || id > upvals.size())
            {
                fail("bad upvalue id");
                o->tt = LUA_TNIL;
                break;
            }
            o->value.gc = obj2gco(upvals[id - 1]);
            break;
        }
        default:
        {
            GCObject* g = ref(r);
            if (!g || g->gch.tt != tt)
            {
                fail("object of the wrong type");
                o->tt = LUA_TNIL;
                o->value.gc = nullptr;
                break;
            }
            o->value.gc = g;
            break;
        }
        }
    }

    // Steps over a value whose objects may not exist yet.
    void skipValue(Reader& r)
    {
        switch (r.u8())
        {
        case LUA_TNIL:
        case LUA_TDEADKEY:
            break;
        case LUA_TBOOLEAN:
            r.u8();
            break;
        case LUA_TNUMBER:
            r.skip(8);
            break;
        case LUA_TVECTOR:
            r.skip(12);
            break;
        case LUA_TLIGHTUSERDATA:
            r.skip(9);
            break;
        case LUA_TSTRING:
        {
            auto it = byHash.find(unsigned(r.var()));
            if (it != byHash.end() && it->second.second > 1)
                r.var();
            break;
        }
        default:
            r.var();
            break;
        }
    }

    Proto* proto(Reader& r)
    {
        uint32_t id = uint32_t(r.var());
        auto it = protosById.find(id);
        if (it == protosById.end())
        {
            fail("missing proto id " + std::to_string(id));
            return nullptr;
        }
        return it->second;
    }
};

namespace
{

// Gives a fresh object the saved gcid.
void setGcid(GCObject* o, uint32_t gcid)
{
    switch (o->gch.tt)
    {
    case LUA_TTABLE:
        gco2h(o)->gcid = gcid;
        break;
    case LUA_TFUNCTION:
        gco2cl(o)->gcid = gcid;
        break;
    case LUA_TUSERDATA:
        gco2u(o)->gcid = gcid;
        break;
    case LUA_TBUFFER:
        gco2buf(o)->gcid = gcid;
        break;
    case LUA_TTHREAD:
        gco2th(o)->gcid = gcid;
        break;
    }
}

void fillTable(lua_SnapshotReader* s, Reader& r, LuaTable* h)
{
    lua_State* L = s->L;
    h->metatable = (LuaTable*)s->ref(r);
    h->readonly = r.u8();
    h->safeenv = r.u8();
    int sizearray = int(r.var());
    int lastfree = int(r.svar());
    uint8_t lsizenode = r.u8();
    if (r.bad)
        return;

    if (h->node != &luaH_dummynode)
        luaM_freearray(L, h->node, sizenode(h), LuaNode, h->memcat);
    if (h->array)
        luaM_freearray(L, h->array, h->sizearray, TValue, h->memcat);
    h->array = sizearray ? luaM_newarray(L, sizearray, TValue, h->memcat) : nullptr;
    h->sizearray = sizearray;
    if (lsizenode == 0xff)
    {
        h->node = (LuaNode*)&luaH_dummynode;
        h->lsizenode = 0;
        h->nodemask8 = 0;
    }
    else
    {
        h->node = luaM_newarray(L, 1 << lsizenode, LuaNode, h->memcat);
        h->lsizenode = lsizenode;
        h->nodemask8 = uint8_t((1 << lsizenode) - 1);
    }
    h->lastfree = lastfree;
    h->tmcache = 0; // unknown: looked up again

    for (int i = 0; i < sizearray; i++)
        s->value(r, &h->array[i]);
    if (lsizenode == 0xff)
        return;
    int n = 1 << lsizenode;
    for (int i = 0; i < n; i++)
    {
        LuaNode* node = &h->node[i];
        TValue key;
        s->value(r, &key);
        node->key.value = key.value;
        memcpy(node->key.extra, key.extra, sizeof(node->key.extra));
        node->key.tt = key.tt;
        node->key.next = int(r.svar());
        s->value(r, &node->val);
    }
}

// The registry's string-keyed entries, set over the loading VM's.
void fillRegistry(lua_SnapshotReader* s, Reader& r, LuaTable* h)
{
    lua_State* L = s->L;
    size_t n = size_t(r.var());
    for (size_t i = 0; i < n && !r.bad && s->error.empty(); i++)
    {
        TString* key = s->str(r);
        TValue value;
        s->value(r, &value);
        if (!key)
            return;
        TValue k;
        setsvalue(L, &k, key);
        setobj(L, luaH_set(L, h, &k), &value);
    }
}

void fillProto(lua_SnapshotReader* s, Reader& r, Proto* p)
{
    lua_State* L = s->L;
    p->nups = r.u8();
    p->numparams = r.u8();
    p->is_vararg = r.u8();
    p->maxstacksize = r.u8();
    p->flags = r.u8();
    p->sizecode = int(r.var());
    p->code = luaM_newarray(L, p->sizecode, Instruction, p->memcat);
    r.raw(p->code, size_t(p->sizecode) * sizeof(Instruction));
    p->codeentry = p->code;
    p->sizek = int(r.var());
    p->k = luaM_newarray(L, p->sizek, TValue, p->memcat);
    for (int i = 0; i < p->sizek; i++)
    {
        setnilvalue(&p->k[i]); // filled once every object exists
        s->skipValue(r);
    }
    p->sizep = int(r.var());
    p->p = luaM_newarray(L, p->sizep, Proto*, p->memcat);
    for (int i = 0; i < p->sizep; i++)
        p->p[i] = s->proto(r);
    p->sizelineinfo = int(r.var());
    if (p->sizelineinfo)
    {
        p->lineinfo = luaM_newarray(L, p->sizelineinfo, uint8_t, p->memcat);
        r.raw(p->lineinfo, p->sizelineinfo);
        p->abslineinfo = (int*)(p->lineinfo + r.var());
        p->linegaplog2 = int(r.var());
    }
    p->sizelocvars = int(r.var());
    p->locvars = luaM_newarray(L, p->sizelocvars, LocVar, p->memcat);
    for (int i = 0; i < p->sizelocvars; i++)
    {
        p->locvars[i].varname = s->optStr(r);
        p->locvars[i].startpc = int(r.var());
        p->locvars[i].endpc = int(r.var());
        p->locvars[i].reg = r.u8();
    }
    p->sizeupvalues = int(r.var());
    p->upvalues = luaM_newarray(L, p->sizeupvalues, TString*, p->memcat);
    for (int i = 0; i < p->sizeupvalues; i++)
        p->upvalues[i] = s->optStr(r);
    p->source = s->optStr(r);
    p->debugname = s->optStr(r);
    p->linedefined = int(r.svar());
    p->bytecodeid = int(r.svar());
    p->sizetypeinfo = int(r.var());
    if (p->sizetypeinfo)
    {
        p->typeinfo = luaM_newarray(L, p->sizetypeinfo, uint8_t, p->memcat);
        r.raw(p->typeinfo, p->sizetypeinfo);
    }
}

// A proto's constants, once every object exists (a constant can be a table).
void fillProtoConstants(lua_SnapshotReader* s, Reader& r, Proto* p)
{
    r.u8(), r.u8(), r.u8(), r.u8(), r.u8();
    int sizecode = int(r.var());
    r.skip(size_t(sizecode) * sizeof(Instruction));
    int sizek = int(r.var());
    for (int i = 0; i < sizek; i++)
        s->value(r, &p->k[i]);
}

// Skips the rest of a proto record (after its constants).
void skipProtoTail(lua_SnapshotReader* s, Reader& r)
{
    int sizep = int(r.var());
    for (int i = 0; i < sizep; i++)
        r.var();
    int sizelineinfo = int(r.var());
    if (sizelineinfo)
    {
        r.skip(sizelineinfo);
        r.var();
        r.var();
    }
    int sizelocvars = int(r.var());
    for (int i = 0; i < sizelocvars; i++)
    {
        s->optStr(r);
        r.var();
        r.var();
        r.u8();
    }
    int sizeupvalues = int(r.var());
    for (int i = 0; i < sizeupvalues; i++)
        s->optStr(r);
    s->optStr(r);
    s->optStr(r);
    r.svar();
    r.svar();
    size_t sizetypeinfo = size_t(r.var());
    r.skip(sizetypeinfo);
}

void fillThread(lua_SnapshotReader* s, Reader& r, lua_State* th)
{
    lua_State* L = s->L;
    th->status = r.u8();
    th->singlestep = r.u8() != 0;
    th->gt = (LuaTable*)s->ref(r);
    size_t top = size_t(r.var());
    size_t base = size_t(r.var());
    size_t ci = size_t(r.var());
    th->nCcalls = (unsigned short)r.var();
    th->baseCcalls = (unsigned short)r.var();
    th->cachedslot = 0;
    th->namecall = s->optStr(r);
    struct Frame
    {
        size_t base, func, top;
        uint64_t pc;
        int nresults;
        unsigned flags;
    };
    std::vector<Frame> frames(ci + 1);
    size_t used = top;
    for (Frame& f : frames)
    {
        f.base = size_t(r.var());
        f.func = size_t(r.var());
        f.top = size_t(r.var());
        f.pc = r.var();
        f.nresults = int(r.svar());
        f.flags = unsigned(r.var());
        used = std::max(used, std::max(f.top, std::max(f.base, f.func + 1)));
    }
    if (r.bad || base > top || ci > size_t(LUAI_MAXCALLS))
    {
        s->fail("bad thread record");
        return;
    }
    // Room as a new thread has at least, and as the frames use.
    int stacksize = int(std::max<size_t>(used, BASIC_STACK_SIZE) + EXTRA_STACK);
    int sizeci = int(std::max<size_t>(ci + 1, BASIC_CI_SIZE));

    luaM_freearray(L, th->stack, th->stacksize, TValue, th->memcat);
    luaM_freearray(L, th->base_ci, th->size_ci, CallInfo, th->memcat);
    th->stack = luaM_newarray(L, stacksize, TValue, th->memcat);
    th->stacksize = stacksize;
    th->stack_last = th->stack + (stacksize - EXTRA_STACK);
    for (int i = 0; i < stacksize; i++)
        setnilvalue(th->stack + i);
    th->base_ci = luaM_newarray(L, sizeci, CallInfo, th->memcat);
    th->size_ci = sizeci;
    th->end_ci = th->base_ci + sizeci - 1;
    th->top = th->stack + top;
    th->base = th->stack + base;
    th->ci = th->base_ci + ci;
    th->openupval = nullptr;
    th->isactive = false;

    for (size_t i = 0; i < top; i++)
        s->value(r, th->stack + i);
    for (size_t i = 0; i <= ci; i++)
    {
        CallInfo* c = th->base_ci + i;
        const Frame& f = frames[i];
        c->base = th->stack + f.base;
        c->func = th->stack + f.func;
        c->top = th->stack + f.top;
        c->savedpc = nullptr;
        if (f.pc && ttisfunction(c->func) && !clvalue(c->func)->isC)
            c->savedpc = clvalue(c->func)->l.p->code + (f.pc - 1);
        c->nresults = f.nresults;
        c->flags = f.flags;
    }
}

void fillClosure(lua_SnapshotReader* s, Reader& r, Closure* cl)
{
    r.u8(); // isC, nupvalues: made with them
    r.u8();
    cl->stacksize = r.u8();
    cl->preload = r.u8();
    s->ref(r); // env: made with it
    if (cl->isC)
    {
        r.svar();
        if (r.u8())
            r.svar();
        size_t len = size_t(r.var());
        if (len)
            r.skip(len - 1);
        for (int i = 0; i < cl->nupvalues; i++)
            s->value(r, &cl->c.upvals[i]);
    }
    else
    {
        r.var();
        for (int i = 0; i < cl->nupvalues; i++)
            s->value(r, &cl->l.uprefs[i]);
    }
}

bool collectBase(void* context, lua_Page* page, GCObject* gco)
{
    lua_SnapshotReader* s = (lua_SnapshotReader*)context;
    if (hasGcid(gco->gch.tt) && gcidOf(gco) <= s->base)
        s->objects.emplace(gcidOf(gco), gco);
    else if (gco->gch.tt == LUA_TPROTO && gco2p(gco)->protoid <= s->baseProto)
        s->protosById.emplace(gco2p(gco)->protoid, gco2p(gco));
    return false;
}

// Closes a base coroutine's open upvalues before its stack is replaced:
// the closures that shared them get the saved ones.
void dropOpenUpvalues(lua_State* th)
{
    for (UpVal* uv = th->openupval; uv;)
    {
        UpVal* next = uv->u.open.threadnext;
        uv->u.open.prev->u.open.next = uv->u.open.next;
        uv->u.open.next->u.open.prev = uv->u.open.prev;
        setnilvalue(&uv->u.value);
        uv->v = &uv->u.value;
        uv = next;
    }
    th->openupval = nullptr;
}

} // namespace

// Starts a load into `L`, which must be at the snapshot's base (nothing
// made after it). Reads the head and indexes the records; the engine then
// makes the external userdata (`lua_snapshot_loadexternal*`) and finishes.
// Returns null with `err` filled on failure.
LUA_API lua_SnapshotReader* lua_snapshot_loadbegin(lua_State* L, const uint8_t* data, size_t len, char* err, size_t errlen)
{
    lua_SnapshotReader* s = new lua_SnapshotReader();
    s->L = L;
    s->data.assign(data, data + len);
    static std::set<std::string> debugnames; // native names, kept for good
    s->debugnames = &debugnames;
    global_State* g = L->global;

    Reader r{s->data.data(), s->data.data() + s->data.size()};
    auto fail = [&](const std::string& what) {
        snprintf(err, errlen, "%s", what.c_str());
        delete s;
        return (lua_SnapshotReader*)nullptr;
    };
    if (r.u32() != kMagic)
        return fail("not a heap snapshot");
    if (r.u32() != kVersion)
        return fail("heap snapshot version differs");
    s->base = uint32_t(r.var());
    s->internal = uint32_t(r.var());
    s->gcidnext = uint32_t(r.var());
    s->baseProto = uint32_t(r.var());
    s->protoidnext = uint32_t(r.var());
    s->rngstate = r.u64();
    if (g->gcidnext != s->base || g->protoidnext != s->baseProto)
        return fail("the VM is not at the snapshot's base (gcid " + std::to_string(g->gcidnext) + ", base " + std::to_string(s->base) + "; proto " +
                    std::to_string(g->protoidnext) + ", base " + std::to_string(s->baseProto) + ")");
    if (g->gcstate != GCSpause)
        return fail("the collector is mid-cycle");
    for (int i = 0; i < LUA_T_COUNT; i++)
        s->metatables.push_back(uint32_t(r.var()));

    size_t nstrings = size_t(r.var());
    s->strings.reserve(nstrings);
    for (size_t i = 0; i < nstrings && !r.bad; i++)
    {
        size_t l = size_t(r.var());
        const uint8_t* bytes = r.skip(l);
        if (r.bad)
            break;
        TString* ts = luaS_newlstr(L, (const char*)bytes, l);
        auto it = s->byHash.find(ts->hash);
        if (it == s->byHash.end())
            s->byHash.emplace(ts->hash, std::make_pair(uint32_t(i), 1u));
        else
            it->second.second++;
        s->strings.push_back(ts);
    }

    s->objects.reserve(1 << 16);
    luaM_visitgco(L, s, collectBase);
    s->objects.emplace(0, obj2gco(g->mainthread));

    size_t nprotos = size_t(r.var());
    // Proto shells now (records name each other), filled below.
    for (size_t i = 0; i < nprotos && !r.bad; i++)
    {
        Proto* p = luaF_newproto(L);
        p->protoid = uint32_t(r.var());
        s->protos.push_back(p);
        s->protosById.emplace(p->protoid, p);
    }
    s->protoSection = r.p;
    for (size_t i = 0; i < nprotos && !r.bad; i++)
    {
        s->where = "proto id " + std::to_string(s->protos[i]->protoid);
        fillProto(s, r, s->protos[i]);
        if (!s->error.empty())
            return fail(s->error);
    }

    size_t nupvals = size_t(r.var());
    s->upvalSection = r.p;
    for (size_t i = 0; i < nupvals; i++)
    {
        UpVal* uv = luaM_newgco(L, UpVal, sizeof(UpVal), L->activememcat);
        luaC_init(L, uv, LUA_TUPVAL);
        uv->markedopen = 0;
        uv->v = &uv->u.value;
        setnilvalue(uv->v);
        s->upvals.push_back(uv);
    }
    // Skip the upvalue records for now (values need the objects).
    for (size_t i = 0; i < nupvals && !r.bad; i++)
        if (!r.u8())
            s->skipValue(r);
    while (!r.bad && r.var() != 0) // open upvalues: id, thread, slot
    {
        r.var();
        r.var();
    }

    size_t nobjects = size_t(r.var());
    s->records.reserve(nobjects);
    for (size_t i = 0; i < nobjects && !r.bad; i++)
    {
        size_t l = size_t(r.var());
        const uint8_t* body = r.skip(l);
        if (r.bad)
            break;
        Reader rr{body, body + l};
        lua_SnapshotReader::Record rec{rr.u8(), uint32_t(rr.var()), rr.p, body + l};
        s->records.push_back(rec);
        if (rec.tt == LUA_TUSERDATA && rec.gcid > s->base)
        {
            uint8_t tag = rr.u8();
            rr.var(); // metatable
            size_t ulen = size_t(rr.var());
            if (rr.u8() == kUdataExternal)
                s->externals.push_back({rec.gcid, tag, ulen, nullptr});
        }
    }
    if (r.bad)
        return fail("heap snapshot is truncated");

    for (auto& rec : s->records)
        if (rec.gcid <= s->base && !s->objects.count(rec.gcid))
            return fail("base object gcid " + std::to_string(rec.gcid) + " is missing from the VM (another build?)");
    return s;
}

LUA_API size_t lua_snapshot_loadexternalcount(lua_SnapshotReader* s)
{
    return s->externals.size();
}

LUA_API unsigned lua_snapshot_loadexternalgcid(lua_SnapshotReader* s, size_t i)
{
    return s->externals[i].gcid;
}

// The engine made external userdata `i`: it is at the top of `L`'s stack
// (left there).
LUA_API void lua_snapshot_loadsetexternal(lua_SnapshotReader* s, size_t i, lua_State* L)
{
    const TValue* o = L->top - 1;
    if (ttisuserdata(o))
        s->externals[i].made = uvalue(o);
}

// Makes every object, fills them, restores the gcid counter. 0 and `err`
// filled on failure (the VM is then unusable).
LUA_API int lua_snapshot_loadfinish(lua_SnapshotReader* s, char* err, size_t errlen)
{
    lua_State* L = s->L;
    global_State* g = L->global;
    auto fail = [&](const std::string& what) {
        snprintf(err, errlen, "%s", what.c_str());
        return 0;
    };
    std::unordered_map<uint32_t, Udata*> made;
    for (auto& e : s->externals)
    {
        if (!e.made)
            return fail("external userdata gcid " + std::to_string(e.gcid) + " was not made");
        made.emplace(e.gcid, e.made);
    }

    // 1. Shells for everything newer than the base, closures last (they are
    // made with their env and proto).
    for (auto& rec : s->records)
    {
        if (rec.gcid <= s->base)
            continue;
        Reader r{rec.body, rec.end};
        GCObject* o = nullptr;
        switch (rec.tt)
        {
        case LUA_TTABLE:
        {
            LuaTable* h = luaH_new(L, 0, 0); // obj2gco evaluates its argument twice
            o = obj2gco(h);
        }
            break;
        case LUA_TTHREAD:
        {
            lua_State* th = luaE_newthread(L);
            o = obj2gco(th);
        }
            break;
        case LUA_TBUFFER:
        {
            size_t l = size_t(r.var());
            Buffer* b = luaB_newbuffer(L, l);
            r.raw(b->data, l);
            o = obj2gco(b);
            break;
        }
        case LUA_TUSERDATA:
        {
            uint8_t tag = r.u8();
            r.var();
            size_t l = size_t(r.var());
            if (r.u8() == kUdataExternal)
            {
                auto it = made.find(rec.gcid);
                if (it == made.end())
                    return fail("external userdata gcid " + std::to_string(rec.gcid) + " was not made");
                Udata* u = it->second;
                o = obj2gco(u);
            }
            else
            {
                Udata* u = luaU_newudata(L, l, tag);
                r.raw(u->data, l);
                o = obj2gco(u);
            }
            break;
        }
        case LUA_TFUNCTION:
            continue;
        default:
            return fail("unknown object type " + std::to_string(rec.tt));
        }
        setGcid(o, rec.gcid);
        s->objects[rec.gcid] = o;
    }
    for (auto& rec : s->records)
    {
        if (rec.gcid <= s->base || rec.tt != LUA_TFUNCTION)
            continue;
        Reader r{rec.body, rec.end};
        uint8_t isC = r.u8();
        uint8_t nup = r.u8();
        r.u8();
        r.u8();
        LuaTable* env = (LuaTable*)s->ref(r);
        Closure* cl;
        if (isC)
        {
            cl = luaF_newCclosure(L, nup, env);
            cl->c.f = reinterpret_cast<lua_CFunction>(anchor() + r.svar());
            if (r.u8())
                cl->c.cont = reinterpret_cast<lua_Continuation>(anchor() + r.svar());
            size_t l = size_t(r.var());
            if (l)
            {
                std::string name((const char*)r.skip(l - 1), l - 1);
                cl->c.debugname = s->debugnames->insert(name).first->c_str();
            }
            for (int i = 0; i < nup; i++)
                setnilvalue(&cl->c.upvals[i]);
        }
        else
            cl = luaF_newLclosure(L, nup, env, s->proto(r));
        setGcid(obj2gco(cl), rec.gcid);
        s->objects[rec.gcid] = obj2gco(cl);
    }
    if (!s->error.empty())
        return fail(s->error);

    // 2. Contents.
    for (auto& rec : s->records)
    {
        Reader r{rec.body, rec.end};
        s->where = "object type " + std::to_string(rec.tt) + " gcid " + std::to_string(rec.gcid);
        auto found = s->objects.find(rec.gcid);
        if (found == s->objects.end())
            return fail("object gcid " + std::to_string(rec.gcid) + " was not made");
        GCObject* o = found->second;
        if (rec.gcid <= s->base)
        {
            uint8_t what = r.u8();
            if (what == kBaseKeep)
                continue;
            switch (rec.tt)
            {
            case LUA_TTABLE:
                if (what == kBaseRegistry)
                    fillRegistry(s, r, gco2h(o));
                else
                    fillTable(s, r, gco2h(o));
                break;
            case LUA_TFUNCTION:
            {
                Closure* cl = gco2cl(o);
                if (cl->isC || int(r.var()) != cl->nupvalues)
                    return fail("base closure gcid " + std::to_string(rec.gcid) + " differs from the VM's (another build?)");
                for (int i = 0; i < cl->nupvalues; i++)
                    s->value(r, &cl->l.uprefs[i]);
                break;
            }
            case LUA_TTHREAD:
                dropOpenUpvalues(gco2th(o));
                fillThread(s, r, gco2th(o));
                break;
            case LUA_TBUFFER:
            {
                Buffer* buf = gco2buf(o);
                if (size_t(r.var()) != buf->len)
                    return fail("base buffer gcid " + std::to_string(rec.gcid) + " changed size");
                r.raw(buf->data, buf->len);
                break;
            }
            }
            if (r.bad)
                return fail("object record gcid " + std::to_string(rec.gcid) + " is truncated");
            if (!s->error.empty())
                return fail(s->error);
            continue;
        }
        switch (rec.tt)
        {
        case LUA_TTABLE:
            fillTable(s, r, gco2h(o));
            break;
        case LUA_TFUNCTION:
            fillClosure(s, r, gco2cl(o));
            break;
        case LUA_TTHREAD:
            fillThread(s, r, gco2th(o));
            break;
        case LUA_TUSERDATA:
        {
            Udata* u = gco2u(o);
            r.u8();
            u->metatable = (LuaTable*)s->ref(r);
            break;
        }
        }
        if (r.bad)
            return fail("object record gcid " + std::to_string(rec.gcid) + " is truncated");
        if (!s->error.empty())
            return fail(s->error);
    }

    // Proto constants and upvalue values, now that every object exists.
    {
        Reader r{s->protoSection, s->data.data() + s->data.size()};
        for (Proto* p : s->protos)
        {
            fillProtoConstants(s, r, p);
            skipProtoTail(s, r);
        }
        if (r.bad)
            return fail("proto section is truncated");
    }
    {
        Reader r{s->upvalSection, s->data.data() + s->data.size()};
        for (UpVal* uv : s->upvals)
            if (!r.u8())
                s->value(r, uv->v);
        for (;;)
        {
            uint64_t id = r.var();
            if (id == 0 || r.bad)
                break;
            GCObject* owner = s->ref(r);
            if (id > s->upvals.size() || !owner || owner->gch.tt != LUA_TTHREAD)
                return fail("bad open upvalue");
            UpVal* uv = s->upvals[id - 1];
            lua_State* th = gco2th(owner);
            uv->v = th->stack + r.var();
            // Thread list: highest slot first.
            UpVal** pp = &th->openupval;
            while (*pp && (*pp)->v > uv->v)
                pp = &(*pp)->u.open.threadnext;
            uv->u.open.threadnext = *pp;
            *pp = uv;
            uv->u.open.prev = &g->uvhead;
            uv->u.open.next = g->uvhead.u.open.next;
            uv->u.open.next->u.open.prev = uv;
            g->uvhead.u.open.next = uv;
        }
    }

    for (int i = 0; i < LUA_T_COUNT; i++)
    {
        GCObject* mt = nullptr;
        if (s->metatables[i])
        {
            auto it = s->objects.find(s->metatables[i] - 1);
            if (it == s->objects.end() || it->second->gch.tt != LUA_TTABLE)
                return fail("a basic type's metatable is missing");
            mt = it->second;
        }
        g->mt[i] = mt ? gco2h(mt) : nullptr;
    }
    g->gcidnext = s->gcidnext;
    g->protoidnext = s->protoidnext;
    g->rngstate = s->rngstate;
    if (!s->error.empty())
        return fail(s->error);
    return 1;
}

// Pushes the loaded object with `gcid` (nil when there is none).
LUA_API void lua_snapshot_loadpush(lua_State* L, lua_SnapshotReader* s, unsigned gcid)
{
    lua_checkstack(L, 1);
    auto it = s->objects.find(gcid);
    if (it == s->objects.end())
        setnilvalue(L->top);
    else
    {
        GCObject* o = it->second;
        L->top->value.gc = o;
        L->top->tt = o->gch.tt;
        memset(L->top->extra, 0, sizeof(L->top->extra));
    }
    L->top++;
}

LUA_API void lua_snapshot_loadend(lua_SnapshotReader* s)
{
    delete s;
}

// ---------------------------------------------------------------------------
// Validation (tests, debugging): every reference in the heap names a live
// object of the type it claims.

namespace
{

struct Validator
{
    lua_State* L;
    std::unordered_map<GCObject*, int> live;
    std::string error;

    void check(GCObject* o, int tt, const char* where)
    {
        if (!o || !error.empty())
            return;
        auto it = live.find(o);
        if (it == live.end())
            error = std::string(where) + ": names a dead or foreign object";
        else if (tt >= 0 && it->second != tt)
            error = std::string(where) + ": type " + std::to_string(it->second) + " where " + std::to_string(tt) + " expected";
    }
    void tv(const TValue* v, const char* where)
    {
        if (iscollectable(v) && v->tt != LUA_TDEADKEY)
            check(gcvalue(v), v->tt, where);
    }
    void object(GCObject* o)
    {
        switch (o->gch.tt)
        {
        case LUA_TTABLE:
        {
            LuaTable* h = gco2h(o);
            check((GCObject*)h->metatable, LUA_TTABLE, "table metatable");
            for (int i = 0; i < h->sizearray; i++)
                tv(&h->array[i], "table array");
            if (h->node != &luaH_dummynode)
                for (int i = 0; i < sizenode(h); i++)
                {
                    if (!ttisnil(&h->node[i].val))
                    {
                        TValue k;
                        getnodekey(L, &k, &h->node[i]);
                        tv(&k, "table key");
                        tv(&h->node[i].val, "table value");
                    }
                }
            break;
        }
        case LUA_TFUNCTION:
        {
            Closure* cl = gco2cl(o);
            check((GCObject*)cl->env, LUA_TTABLE, "closure env");
            if (cl->isC)
                for (int i = 0; i < cl->nupvalues; i++)
                    tv(&cl->c.upvals[i], "C closure upvalue");
            else
            {
                check((GCObject*)cl->l.p, LUA_TPROTO, "closure proto");
                for (int i = 0; i < cl->nupvalues; i++)
                    tv(&cl->l.uprefs[i], "closure upref");
            }
            break;
        }
        case LUA_TTHREAD:
        {
            lua_State* th = gco2th(o);
            check((GCObject*)th->gt, LUA_TTABLE, "thread globals");
            for (StkId s = th->stack; s < th->top; s++)
                tv(s, "thread stack");
            for (UpVal* uv = th->openupval; uv; uv = uv->u.open.threadnext)
                check((GCObject*)uv, LUA_TUPVAL, "thread open upvalue");
            break;
        }
        case LUA_TPROTO:
        {
            Proto* p = gco2p(o);
            for (int i = 0; i < p->sizek; i++)
                tv(&p->k[i], "proto constant");
            for (int i = 0; i < p->sizep; i++)
                check((GCObject*)p->p[i], LUA_TPROTO, "proto child");
            check((GCObject*)p->source, LUA_TSTRING, "proto source");
            check((GCObject*)p->debugname, LUA_TSTRING, "proto debugname");
            break;
        }
        case LUA_TUPVAL:
        {
            UpVal* uv = gco2uv(o);
            if (!upisopen(uv))
                tv(uv->v, "closed upvalue");
            break;
        }
        case LUA_TUSERDATA:
            check((GCObject*)gco2u(o)->metatable, LUA_TTABLE, "userdata metatable");
            break;
        }
    }
};

bool collectLive(void* context, lua_Page* page, GCObject* gco)
{
    ((Validator*)context)->live.emplace(gco, gco->gch.tt);
    return false;
}

bool validateOne(void* context, lua_Page* page, GCObject* gco)
{
    Validator* v = (Validator*)context;
    v->object(gco);
    if (!v->error.empty())
    {
        v->error += " (in a " + std::to_string(gco->gch.tt) + ", gcid " + std::to_string(gcidOf(gco)) + ")";
        return false;
    }
    return false;
}

} // namespace

// 1 if the heap is consistent; else 0 and `err` says where.
LUA_API int lua_snapshot_validate(lua_State* L, char* err, size_t errlen)
{
    Validator v;
    v.L = L;
    luaM_visitgco(L, &v, collectLive);
    v.live.emplace(obj2gco(L->global->mainthread), LUA_TTHREAD);
    luaM_visitgco(L, &v, validateOne);
    if (v.error.empty())
        v.object(obj2gco(L->global->mainthread));
    snprintf(err, errlen, "%s", v.error.c_str());
    return v.error.empty();
}

// The last gcid handed out (no allocation).
LUA_API unsigned lua_snapshot_gcidnext(lua_State* L)
{
    return L->global->gcidnext;
}

// The last proto id handed out.
LUA_API unsigned lua_snapshot_protoidnext(lua_State* L)
{
    return L->global->protoidnext;
}

// Collects fully and stops the collector (no allocation, unlike a call
// through mlua).
LUA_API void lua_snapshot_collect(lua_State* L)
{
    lua_gc(L, LUA_GCCOLLECT, 0);
    lua_gc(L, LUA_GCSTOP, 0);
}

// Debugging: object `i` of a writer (gcid order): its gcid and type.
LUA_API unsigned lua_snapshot_objectinfo(lua_SnapshotWriter* w, size_t i, int* tt)
{
    *tt = w->objects[i]->gch.tt;
    return gcidOf(w->objects[i]);
}

// Debugging: the writer's strings (hash order) and protos.
LUA_API size_t lua_snapshot_stringcount(lua_SnapshotWriter* w)
{
    return w->strings.size();
}

LUA_API const char* lua_snapshot_string(lua_SnapshotWriter* w, size_t i, size_t* len)
{
    *len = w->strings[i]->len;
    return w->strings[i]->data;
}

// Debugging: names what holds byte `offset` of a heap snapshot (section,
// and for an object its type and gcid). No VM needed.
LUA_API void lua_snapshot_locate(const uint8_t* data, size_t len, size_t offset, char* out, size_t outlen)
{
    lua_SnapshotReader s;
    s.L = nullptr;
    Reader r{data, data + len};
    auto at = [&](const char* what) {
        snprintf(out, outlen, "%s", what);
    };
    r.u32();
    r.u32();
    for (int i = 0; i < 5; i++)
        r.var();
    r.u64();
    for (int i = 0; i < LUA_T_COUNT; i++)
        r.var();
    if (size_t(r.p - data) > offset)
        return at("header");
    size_t nstrings = size_t(r.var());
    for (size_t i = 0; i < nstrings; i++)
    {
        size_t l = size_t(r.var());
        const uint8_t* bytes = r.skip(l);
        unsigned h = luaS_hash((const char*)bytes, l);
        auto it = s.byHash.find(h);
        if (it == s.byHash.end())
            s.byHash.emplace(h, std::make_pair(uint32_t(i), 1u));
        else
            it->second.second++;
    }
    if (size_t(r.p - data) > offset)
        return at("strings");
    size_t nprotos = size_t(r.var());
    std::vector<unsigned> ids;
    for (size_t i = 0; i < nprotos; i++)
        ids.push_back(unsigned(r.var()));
    // Proto bodies: skip field by field.
    for (size_t i = 0; i < nprotos && !r.bad; i++)
    {
        const uint8_t* start = r.p;
        const uint8_t* constants = nullptr;
        auto inside = [&]() {
            if (size_t(r.p - data) > offset)
            {
                snprintf(out, outlen, "proto id %u, byte %ld of its record (constants from byte %ld)", ids[i], long(offset - (start - data)),
                    constants ? long(constants - start) : -1L);
                return true;
            }
            return false;
        };
        for (int k = 0; k < 5; k++)
            r.u8();
        int sizecode = int(r.var());
        r.skip(size_t(sizecode) * sizeof(Instruction));
        if (inside())
            return;
        int sizek = int(r.var());
        constants = r.p;
        for (int k = 0; k < sizek; k++)
            s.skipValue(r);
        int sizep = int(r.var());
        for (int k = 0; k < sizep; k++)
            r.var();
        int sizelineinfo = int(r.var());
        if (sizelineinfo)
        {
            r.skip(sizelineinfo);
            r.var();
            r.var();
        }
        if (inside())
            return;
        int sizelocvars = int(r.var());
        for (int k = 0; k < sizelocvars; k++)
        {
            if (r.u8())
            {
                auto it = s.byHash.find(unsigned(r.var()));
                if (it != s.byHash.end() && it->second.second > 1)
                    r.var();
            }
            r.var();
            r.var();
            r.u8();
        }
        auto optStr = [&]() {
            if (r.u8())
            {
                auto it = s.byHash.find(unsigned(r.var()));
                if (it != s.byHash.end() && it->second.second > 1)
                    r.var();
            }
        };
        int sizeupvalues = int(r.var());
        for (int k = 0; k < sizeupvalues; k++)
            optStr();
        optStr();
        optStr();
        r.svar();
        r.svar();
        r.skip(size_t(r.var()));
        if (inside())
            return;
    }
    if (size_t(r.p - data) > offset)
        return at("protos");
    size_t nupvals = size_t(r.var());
    for (size_t i = 0; i < nupvals; i++)
        if (!r.u8())
            s.skipValue(r);
    while (!r.bad && r.var() != 0)
    {
        r.var();
        r.var();
    }
    if (size_t(r.p - data) > offset)
        return at("upvalues");
    size_t nobjects = size_t(r.var());
    for (size_t i = 0; i < nobjects && !r.bad; i++)
    {
        size_t l = size_t(r.var());
        const uint8_t* body = r.skip(l);
        if (size_t(r.p - data) > offset)
        {
            Reader rr{body, body + l};
            int tt = rr.u8();
            unsigned gcid = unsigned(rr.var());
            snprintf(out, outlen, "object type %d gcid %u, byte %ld of %zu", tt, gcid, long(offset - (body - data)), l);
            return;
        }
    }
    at("past the end");
}

// Debugging: the writer's protos newer than the base: id, source, line,
// code size.
LUA_API size_t lua_snapshot_protocount(lua_SnapshotWriter* w)
{
    return w->protos.size();
}

LUA_API unsigned lua_snapshot_protoinfo(lua_SnapshotWriter* w, size_t i, const char** source, int* line, int* sizecode)
{
    Proto* p = w->protos[i];
    *source = p->source ? getstr(p->source) : "?";
    *line = p->linedefined;
    *sizecode = p->sizecode;
    return p->protoid;
}

// Debugging: describes the native closures newer than the base (a keyframe
// can't hold one whose upvalue is the engine's Rust closure).
LUA_API void lua_snapshot_describenatives(lua_SnapshotWriter* w, char* out, size_t outlen)
{
    std::string s;
    int n = 0;
    for (GCObject* o : w->objects)
    {
        if (o->gch.tt != LUA_TFUNCTION || gcidOf(o) <= w->base)
            continue;
        Closure* cl = gco2cl(o);
        if (!cl->isC)
            continue;
        if (++n > 12)
            continue;
        s += "[gcid " + std::to_string(cl->gcid) + " " + (cl->c.debugname ? cl->c.debugname : "?") + " upvalues:";
        for (int i = 0; i < cl->nupvalues; i++)
            s += " " + std::string(lua_typename(w->L, cl->c.upvals[i].tt));
        s += "] ";
    }
    s = std::to_string(n) + " natives newer than the base: " + s;
    snprintf(out, outlen, "%s", s.c_str());
}

// Debugging: describes external userdata `i`: its tag, size, metatable
// (gcid, string keys) and what refers to it (table keys, coroutines).
LUA_API void lua_snapshot_describeexternal(lua_SnapshotWriter* w, size_t i, char* out, size_t outlen)
{
    Udata* u = w->external[i];
    std::string s = "tag " + std::to_string(u->tag) + ", " + std::to_string(u->len) + " bytes, metatable ";
    if (LuaTable* mt = u->metatable)
    {
        s += "gcid " + std::to_string(mt->gcid) + " keys:";
        if (mt->node != &luaH_dummynode)
            for (int k = 0; k < sizenode(mt); k++)
                if (mt->node[k].key.tt == LUA_TSTRING && !ttisnil(&mt->node[k].val))
                    s += " " + std::string(getstr(&mt->node[k].key.value.gc->ts));
    }
    else
        s += "none";
    s += "; referred to by:";
    int found = 0;
    for (GCObject* o : w->objects)
    {
        if (found > 6)
            break;
        if (o->gch.tt == LUA_TTABLE)
        {
            LuaTable* h = gco2h(o);
            if (h->node != &luaH_dummynode)
                for (int k = 0; k < sizenode(h); k++)
                    if (ttisuserdata(&h->node[k].val) && uvalue(&h->node[k].val) == u)
                    {
                        found++;
                        s += " table " + std::to_string(h->gcid) + "[" + (h->node[k].key.tt == LUA_TSTRING ? getstr(&h->node[k].key.value.gc->ts) : "?") + "]";
                    }
            for (int k = 0; k < h->sizearray; k++)
                if (ttisuserdata(&h->array[k]) && uvalue(&h->array[k]) == u)
                {
                    found++;
                    s += " table " + std::to_string(h->gcid) + "[" + std::to_string(k + 1) + "]";
                }
        }
        else if (o->gch.tt == LUA_TTHREAD)
        {
            lua_State* th = gco2th(o);
            for (StkId v = th->stack; v < th->top; v++)
                if (ttisuserdata(v) && uvalue(v) == u)
                {
                    found++;
                    s += " thread " + std::to_string(th->gcid) + " (status " + std::to_string(th->status) + ") slot " + std::to_string(v - th->stack);
                }
        }
    }
    snprintf(out, outlen, "%s", s.c_str());
}

// Notes the destructor of the userdata at the top of the stack (one of
// mlua's error objects) as mlua's internal one (lstate.h).
LUA_API void lua_snapshot_setinternaldtor(lua_State* L)
{
    const TValue* o = L->top - 1;
    if (ttisuserdata(o) && uvalue(o)->tag == UTAG_IDTOR)
    {
        Udata* u = uvalue(o);
        void (*dtor)(void*) = nullptr;
        memcpy(&dtor, &u->data + u->len - sizeof(dtor), sizeof(dtor));
        L->global->rts_internaldtor = dtor;
    }
}

// Objects made while `on` take no gcid (see `rts_gcidfrozen`).
LUA_API void lua_snapshot_freezegcids(lua_State* L, int on)
{
    L->global->rts_gcidfrozen = on != 0;
}
