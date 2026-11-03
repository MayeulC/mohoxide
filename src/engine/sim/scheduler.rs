//! Deterministic coroutine scheduler for moho's Lua threads
//! (`ForkThread`, `WaitTicks`, `KillThread`, `SuspendCurrentThread`, ...).
//!
//! Determinism: every task gets a monotonically increasing id at creation.
//! Sleeping tasks live in a min-heap keyed by `(wake_tick, id)`, so tasks due on
//! the same tick always resume in creation order on every machine. Lookups by
//! Lua thread use a pointer-keyed `HashMap`, which is never iterated.
//!
//! Forked tasks go to a FIFO `pending` queue and first run at the end of the
//! tick they were forked in (after that tick's due tasks), including children
//! forked while draining — see docs/architecture/lua-bridge.md.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, VecDeque};

use mlua::prelude::*;

use crate::engine::lua::heap_snapshot::{gcid_of, LoadedHeap, SavedValue};

pub type TaskId = u64;

struct Task {
    thread: LuaThread,
    /// Arguments for the first resume; `None` once started.
    start_args: Option<LuaMultiValue>,
    /// Suspended indefinitely until `ResumeThread`.
    suspended: bool,
}

#[derive(Default)]
pub struct Scheduler {
    tick: u64,
    /// A sim tick has begun ([`Scheduler::begin_tick`]): from then on `tick`
    /// is the number the tick's snapshot will carry. False only before the
    /// first tick.
    advanced: bool,
    next_id: TaskId,
    // Looked up by id; `save` sorts the ids before walking them.
    #[allow(clippy::disallowed_types)]
    tasks: HashMap<TaskId, Task>,
    #[allow(clippy::disallowed_types)]
    by_thread: HashMap<usize, TaskId>,
    sleeping: BinaryHeap<Reverse<(u64, TaskId)>>,
    pending: VecDeque<TaskId>,
    /// The sim's waits (moho's): a yield of `n` waits `n - 1` ticks. Off,
    /// a yield of `n` waits `n` steps (the interface's frames).
    moho_waits: bool,
}

impl Scheduler {
    pub fn tick(&self) -> u64 {
        self.tick
    }

    /// The tick number the next snapshot will carry: `tick` during and
    /// after a sim tick, the first tick before any has run (session setup).
    /// Effects are stamped with it.
    pub fn snapshot_tick(&self) -> u64 {
        if self.advanced { self.tick } else { self.tick + 1 }
    }

    /// Waits as the real game's sim threads do ([`Scheduler::wait`]).
    pub fn set_moho_waits(&mut self, on: bool) {
        self.moho_waits = on;
    }

    /// What a thread that yielded `value` (its first yielded value, if a
    /// number) does next. The real game's sim (FAF ForgedAlliance.exe,
    /// dev/probes probe_ticks, `GetGameTick()` either side): a yield of `n`
    /// waits `floor(n) - 1` ticks, at least 1 (`WaitTicks(2)` and `(1)` 1
    /// tick, `(3)` 2, `(10)` 9; 2.5 1, 3.9 2); below 1 it runs again in the
    /// same tick (`WaitTicks(0)` 0 ticks); a bare `coroutine.yield()` never
    /// came back (suspended, as `SuspendCurrentThread`). FAF's sim
    /// `WaitSeconds(n)` is `WaitTicks(n * 10 + 1)`, so it waits `n` seconds.
    /// Without moho waits: `max(n, 1)` steps, and a bare yield one.
    pub fn wait(&self, value: Option<f64>) -> ResumeOutcome {
        match value {
            _ if !self.moho_waits => ResumeOutcome::Yielded(value.filter(|n| *n >= 1.0).map_or(1, |n| n as u64)),
            None => ResumeOutcome::Suspended,
            Some(n) if n.is_nan() => ResumeOutcome::Yielded(1),
            Some(n) if n < 1.0 => ResumeOutcome::Yielded(0),
            Some(n) => ResumeOutcome::Yielded((n.floor() as u64).saturating_sub(1).max(1)),
        }
    }

    /// A sim tick starts (before any stage runs): the tick number goes up
    /// first, so every stage of beat `n` reads `n`, as the real game's
    /// `Sim::AdvanceBeat` (MohoEngine.dll 0x10318f50) adds 1 to the tick
    /// (`Sim` +0x910, what `GetGameTick` reads) at 0x10318fed, before its
    /// command dispatch, script and motion stages.
    pub fn begin_tick(&mut self) {
        self.tick += 1;
        self.advanced = true;
    }

    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    fn key(thread: &LuaThread) -> usize {
        thread.to_pointer() as usize
    }

    /// Queues a new task; it first runs when [`Scheduler::next_pending`] drains it.
    pub fn fork(&mut self, thread: LuaThread, args: LuaMultiValue) -> TaskId {
        let id = self.next_id;
        self.next_id += 1;
        self.by_thread.insert(Self::key(&thread), id);
        self.tasks.insert(
            id,
            Task {
                thread,
                start_args: Some(args),
                suspended: false,
            },
        );
        self.pending.push_back(id);
        id
    }

    /// Next forked task waiting for its first run (FIFO), skipping killed ones.
    pub fn next_pending(&mut self) -> Option<TaskId> {
        while let Some(id) = self.pending.pop_front() {
            if self.tasks.contains_key(&id) {
                return Some(id);
            }
        }
        None
    }

    /// Removes a task; returns false if the thread isn't scheduled. Stale heap
    /// entries are skipped lazily when popped.
    pub fn kill(&mut self, thread: &LuaThread) -> bool {
        match self.by_thread.remove(&Self::key(thread)) {
            Some(id) => self.tasks.remove(&id).is_some(),
            None => false,
        }
    }

    /// Marks `thread`'s task suspended: once it yields it isn't rescheduled
    /// until [`Scheduler::resume_suspended`]. False if it isn't scheduled.
    pub fn suspend(&mut self, thread: &LuaThread) -> bool {
        let Some(id) = self.by_thread.get(&Self::key(thread)) else { return false };
        match self.tasks.get_mut(id) {
            Some(t) => {
                t.suspended = true;
                true
            }
            None => false,
        }
    }

    /// Wakes a suspended task on the next tick; no-op if it isn't suspended.
    pub fn resume_suspended(&mut self, thread: &LuaThread) -> bool {
        let Some(&id) = self.by_thread.get(&Self::key(thread)) else { return false };
        match self.tasks.get_mut(&id) {
            Some(t) if t.suspended => {
                t.suspended = false;
                self.sleeping.push(Reverse((self.tick + 1, id)));
                true
            }
            _ => false,
        }
    }

    pub fn contains(&self, thread: &LuaThread) -> bool {
        self.by_thread.contains_key(&Self::key(thread))
    }

    /// Starts a tick and returns the tasks due in it (a VM with no sim tick
    /// around its threads: the interface's frames).
    pub fn advance(&mut self) -> Vec<TaskId> {
        self.begin_tick();
        self.take_due()
    }

    /// The tasks due this tick (started with [`Scheduler::begin_tick`]), in
    /// deterministic order. The caller resumes each with [`Scheduler::resume`].
    pub fn take_due(&mut self) -> Vec<TaskId> {
        let mut due = Vec::new();
        while let Some(&Reverse((wake, id))) = self.sleeping.peek() {
            if wake > self.tick {
                break;
            }
            self.sleeping.pop();
            if self.tasks.get(&id).is_some_and(|t| !t.suspended) {
                due.push(id);
            }
        }
        due
    }

    /// Takes what's needed to resume task `id` (it may have been killed by an
    /// earlier task this tick, in which case `None`).
    pub fn begin_resume(&mut self, id: TaskId) -> Option<(LuaThread, LuaMultiValue)> {
        let task = self.tasks.get_mut(&id)?;
        let args = task.start_args.take().unwrap_or_default();
        Some((task.thread.clone(), args))
    }

    /// Records the outcome of a resume. `yielded` is the first yielded value.
    pub fn end_resume(&mut self, id: TaskId, outcome: ResumeOutcome) {
        let Some(task) = self.tasks.get_mut(&id) else { return };
        match outcome {
            // No wait: again this tick, after the tick's other threads.
            ResumeOutcome::Yielded(0) if !task.suspended => self.pending.push_back(id),
            ResumeOutcome::Yielded(ticks) => {
                if !task.suspended {
                    self.sleeping.push(Reverse((self.tick + ticks, id)));
                }
            }
            ResumeOutcome::Suspended => task.suspended = true,
            ResumeOutcome::Finished => {
                let key = Self::key(&task.thread);
                self.tasks.remove(&id);
                self.by_thread.remove(&key);
            }
        }
    }
}

/// A [`Scheduler`] in a keyframe: threads and start arguments by gcid.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct SavedScheduler {
    tick: u64,
    advanced: bool,
    next_id: TaskId,
    /// (id, thread, start arguments, suspended), by id.
    tasks: Vec<(TaskId, u32, Option<Vec<SavedValue>>, bool)>,
    /// The sleeping heap's entries, sorted (entries are unique, so any heap
    /// built from them pops in the same order).
    sleeping: Vec<(u64, TaskId)>,
    pending: Vec<TaskId>,
    moho_waits: bool,
}

impl Scheduler {
    /// This scheduler as a keyframe holds it.
    pub fn save(&self, lua: &Lua) -> LuaResult<SavedScheduler> {
        let mut ids: Vec<&TaskId> = self.tasks.keys().collect();
        ids.sort();
        let mut tasks = Vec::with_capacity(ids.len());
        for id in ids {
            let t = &self.tasks[id];
            let thread = gcid_of(lua, &LuaValue::Thread(t.thread.clone()))?;
            let args = t.start_args.as_ref().map(|a| a.iter().map(|v| SavedValue::save(lua, v)).collect::<LuaResult<Vec<_>>>()).transpose()?;
            tasks.push((*id, thread, args, t.suspended));
        }
        let mut sleeping: Vec<(u64, TaskId)> = self.sleeping.iter().map(|Reverse(e)| *e).collect();
        sleeping.sort();
        Ok(SavedScheduler {
            tick: self.tick,
            advanced: self.advanced,
            next_id: self.next_id,
            tasks,
            sleeping,
            pending: self.pending.iter().copied().collect(),
            moho_waits: self.moho_waits,
        })
    }

    /// The scheduler `saved`, its threads from the loaded `heap`.
    pub fn load(lua: &Lua, saved: &SavedScheduler, heap: &LoadedHeap) -> LuaResult<Self> {
        let mut s = Scheduler {
            tick: saved.tick,
            advanced: saved.advanced,
            next_id: saved.next_id,
            moho_waits: saved.moho_waits,
            pending: saved.pending.iter().copied().collect(),
            sleeping: saved.sleeping.iter().map(|e| Reverse(*e)).collect(),
            ..Default::default()
        };
        for (id, thread, args, suspended) in &saved.tasks {
            let LuaValue::Thread(thread) = heap.get(*thread)? else {
                return Err(LuaError::runtime(format!("keyframe: task {id}'s thread (gcid {thread}) isn't a thread")));
            };
            let start_args = args.as_ref().map(|a| a.iter().map(|v| v.load(lua, heap)).collect::<LuaResult<LuaMultiValue>>()).transpose()?;
            s.by_thread.insert(Self::key(&thread), *id);
            s.tasks.insert(*id, Task { thread, start_args, suspended: *suspended });
        }
        Ok(s)
    }
}

pub enum ResumeOutcome {
    /// Yielded, to wake this many ticks on (0: again this tick); see
    /// [`Scheduler::wait`].
    Yielded(u64),
    /// Yielded with no wait (a bare `coroutine.yield()`): until `ResumeThread`.
    Suspended,
    /// Returned or raised; either way the task is gone.
    Finished,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_tick_tasks_run_in_creation_order_and_kill_skips() {
        let lua = Lua::new();
        let mut s = Scheduler::default();
        let f = lua.create_function(|_, ()| Ok(())).unwrap();
        let a = lua.create_thread(f.clone()).unwrap();
        let b = lua.create_thread(f.clone()).unwrap();
        let c = lua.create_thread(f).unwrap();
        let ia = s.fork(a, LuaMultiValue::new());
        let ib = s.fork(b.clone(), LuaMultiValue::new());
        let ic = s.fork(c, LuaMultiValue::new());
        assert!(s.kill(&b));
        assert!(!s.contains(&b));
        assert_eq!(s.next_pending(), Some(ia));
        assert_eq!(s.next_pending(), Some(ic));
        assert_eq!(s.next_pending(), None);
        assert!(s.begin_resume(ib).is_none());
        // Yielded tasks wake in (tick, id) order.
        s.end_resume(ic, ResumeOutcome::Yielded(2));
        s.end_resume(ia, ResumeOutcome::Yielded(2));
        assert!(s.advance().is_empty());
        assert_eq!(s.advance(), vec![ia, ic]);
    }

    #[test]
    fn sim_waits_are_one_tick_shorter_and_a_bare_yield_suspends() {
        let mut s = Scheduler::default();
        let ticks = |s: &Scheduler, v: Option<f64>| match s.wait(v) {
            ResumeOutcome::Yielded(n) => Some(n),
            _ => None,
        };
        // The interface's: n steps, a bare yield one.
        assert_eq!([0.0, 1.0, 2.0, 3.0].map(|n| ticks(&s, Some(n))), [Some(1), Some(1), Some(2), Some(3)]);
        assert_eq!(ticks(&s, None), Some(1));
        // The sim's (the real game's, dev/probes probe_ticks).
        s.set_moho_waits(true);
        let got = [0.0, 0.5, 1.0, 2.0, 2.5, 3.0, 3.9, 5.0, 10.0, 11.0].map(|n| ticks(&s, Some(n)));
        assert_eq!(got, [Some(0), Some(0), Some(1), Some(1), Some(1), Some(2), Some(2), Some(4), Some(9), Some(10)]);
        assert!(matches!(s.wait(None), ResumeOutcome::Suspended));
    }

    #[test]
    fn a_zero_wait_runs_again_the_same_tick_and_a_suspended_task_waits_for_resume() {
        let lua = Lua::new();
        let mut s = Scheduler::default();
        let f = lua.create_function(|_, ()| Ok(())).unwrap();
        let (a, b) = (s.fork(lua.create_thread(f.clone()).unwrap(), LuaMultiValue::new()), s.fork(lua.create_thread(f).unwrap(), LuaMultiValue::new()));
        assert_eq!(s.next_pending(), Some(a));
        s.end_resume(a, ResumeOutcome::Yielded(0));
        assert_eq!(s.next_pending(), Some(b));
        s.end_resume(b, ResumeOutcome::Suspended);
        // a again, after b, in the same drain.
        assert_eq!(s.next_pending(), Some(a));
        s.end_resume(a, ResumeOutcome::Yielded(1));
        assert_eq!(s.next_pending(), None);
        assert_eq!(s.advance(), vec![a]);
        assert!(s.advance().is_empty(), "b stays suspended");
    }
}
