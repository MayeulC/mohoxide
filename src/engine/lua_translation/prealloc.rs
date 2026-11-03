/// Strips SupCom table pre-allocation hints `{&narr &nhash}` / `{&narr &nhash, …}`.
///
/// SupCom's Lua interpreter extends the NEWTABLE opcode with source-level size hints.
/// These are invalid Luau syntax; Luau infers table capacity from the constructor shape.
pub fn strip(source: &str) -> String {
    let mut out = String::with_capacity(source.len());
    let mut s = source;
    while let Some(pos) = s.find("{&") {
        out.push_str(&s[..pos]);
        match parse_hint(&s[pos..]) {
            Some((rest, false)) => { out.push_str("{}"); s = rest; }
            Some((rest, true))  => { out.push('{');      s = rest; }
            None                => { out.push('{');      s = &s[pos + 1..]; }
        }
    }
    out.push_str(s);
    out
}

/// Matches `{&\d+ &\d+` (the space optional: LOUD writes `{&1&4}`) followed
/// by `}`, `, ` or the first field (LOUD: `{&1&1 n=0}`) at `s[0..]`.
/// Returns `(remaining, had_content)`:
///   `false` → closing `}` consumed, caller emits `{}`
///   `true`  → any `, ` separator consumed, caller emits `{` then `remaining`
fn parse_hint(s: &str) -> Option<(&str, bool)> {
    let b = s.as_bytes();
    let mut i = 1; // skip '{'
    if b.get(i) != Some(&b'&') { return None; }
    i += 1;
    if !b.get(i).is_some_and(|c| c.is_ascii_digit()) { return None; }
    while b.get(i).is_some_and(|c| c.is_ascii_digit()) { i += 1; }
    while b.get(i) == Some(&b' ') { i += 1; }
    if b.get(i) != Some(&b'&') { return None; }
    i += 1;
    if !b.get(i).is_some_and(|c| c.is_ascii_digit()) { return None; }
    while b.get(i).is_some_and(|c| c.is_ascii_digit()) { i += 1; }
    while b.get(i).is_some_and(|c| *c == b' ' || *c == b'\t') { i += 1; }
    match b.get(i) {
        Some(b'}') => Some((&s[i + 1..], false)),
        Some(b',') => {
            i += 1;
            if b.get(i) == Some(&b' ') { i += 1; }
            Some((&s[i..], true))
        }
        Some(_) => Some((&s[i..], true)),
        None => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verbatim block from fa/lua/system/class.lua lines 571-689.
    // Only the {&N &M} hints should change; all other syntax must survive intact.
    const CLASS_LUA_INPUT: &str = r#"--- Instantiation of a class, referring to the paragraphs of text at the top of this file
ClassFactory = {
    __call = function(self, ...)

        -- LOG(string.format("%s -> %s", "ClassFactory", tostring(self.__name)))

        -- create the new entity with us as its meta table
        local instance = {&1 &0}
        setmetatable(instance, self)

        -- call class initialisation functions, if they exist
        local initfn = self.__init
        if initfn then
            initfn(instance, unpack(arg))
        end
        local postinitfn = self.__post_init
        if postinitfn then
            postinitfn(instance, unpack(arg))
        end

        return instance
    end
}

UIFactory = {
    ---@param self any
    ---@param ... any
    ---@return table
    __call = function (self, ...)
        -- LOG(string.format("%s -> %s", "UIFactory", tostring(self.__name)))

        local instance = {&15 &0}
        setmetatable(instance, self)

        local initfn = self.__init
        if initfn then
            initfn(instance, unpack(arg))
        end
        local postinitfn = self.__post_init
        if postinitfn then
            postinitfn(instance, unpack(arg))
        end

        return instance
    end
}

CFactory = {
    ---@param self any
    ---@return table
    __call = function (self)
        -- LOG(string.format("%s -> %s", "CFactory", tostring(self.__name)))
        local instance = {&1 &0}
        return setmetatable(instance, self)
    end
}

TrashBagFactory = {
    ---@param self any
    ---@return table
    __call = function (self)
        -- LOG(string.format("%s -> %s", "PropFactory", tostring(self.__name)))
        local instance = {&0 &2}
        return setmetatable(instance, self)
    end
}

PropFactory = {
    ---@param self any
    ---@return table
    __call = function (self)
        -- LOG(string.format("%s -> %s", "PropFactory", tostring(self.__name)))
        local instance = {&7 &0}
        return setmetatable(instance, self)
    end
}

EntityFactory = {
    ---@param self any
    ---@return table
    __call = function (self, ...)
        -- LOG(string.format("%s -> %s", "EntityFactory", tostring(self.__name)))

        local instance = {&1 &0}
        setmetatable(instance, self)

        -- call class initialisation functions, if they exist
        local initfn = self.__init
        if initfn then
            initfn(instance, unpack(arg))
        end
        local postinitfn = self.__post_init
        if postinitfn then
            postinitfn(instance, unpack(arg))
        end

        return instance
    end
}

ProjectileFactory = {
    ---@param self any
    ---@return table
    __call = function (self)
        -- LOG(string.format("%s -> %s", "ProjectileFactory", tostring(self.__name)))
        local instance = {&7 &0}
        return setmetatable(instance, self)
    end
}

DummyProjectileFactory = {
    ---@param self any
    ---@return table
    __call = function (self)
        -- LOG(string.format("%s -> %s", "ProjectileFactory", tostring(self.__name)))
        -- needs a hash part of one for the _c_object field
        local instance = {&3 &0}
        return setmetatable(instance, self)
    end
}"#;

    const CLASS_LUA_EXPECTED: &str = r#"--- Instantiation of a class, referring to the paragraphs of text at the top of this file
ClassFactory = {
    __call = function(self, ...)

        -- LOG(string.format("%s -> %s", "ClassFactory", tostring(self.__name)))

        -- create the new entity with us as its meta table
        local instance = {}
        setmetatable(instance, self)

        -- call class initialisation functions, if they exist
        local initfn = self.__init
        if initfn then
            initfn(instance, unpack(arg))
        end
        local postinitfn = self.__post_init
        if postinitfn then
            postinitfn(instance, unpack(arg))
        end

        return instance
    end
}

UIFactory = {
    ---@param self any
    ---@param ... any
    ---@return table
    __call = function (self, ...)
        -- LOG(string.format("%s -> %s", "UIFactory", tostring(self.__name)))

        local instance = {}
        setmetatable(instance, self)

        local initfn = self.__init
        if initfn then
            initfn(instance, unpack(arg))
        end
        local postinitfn = self.__post_init
        if postinitfn then
            postinitfn(instance, unpack(arg))
        end

        return instance
    end
}

CFactory = {
    ---@param self any
    ---@return table
    __call = function (self)
        -- LOG(string.format("%s -> %s", "CFactory", tostring(self.__name)))
        local instance = {}
        return setmetatable(instance, self)
    end
}

TrashBagFactory = {
    ---@param self any
    ---@return table
    __call = function (self)
        -- LOG(string.format("%s -> %s", "PropFactory", tostring(self.__name)))
        local instance = {}
        return setmetatable(instance, self)
    end
}

PropFactory = {
    ---@param self any
    ---@return table
    __call = function (self)
        -- LOG(string.format("%s -> %s", "PropFactory", tostring(self.__name)))
        local instance = {}
        return setmetatable(instance, self)
    end
}

EntityFactory = {
    ---@param self any
    ---@return table
    __call = function (self, ...)
        -- LOG(string.format("%s -> %s", "EntityFactory", tostring(self.__name)))

        local instance = {}
        setmetatable(instance, self)

        -- call class initialisation functions, if they exist
        local initfn = self.__init
        if initfn then
            initfn(instance, unpack(arg))
        end
        local postinitfn = self.__post_init
        if postinitfn then
            postinitfn(instance, unpack(arg))
        end

        return instance
    end
}

ProjectileFactory = {
    ---@param self any
    ---@return table
    __call = function (self)
        -- LOG(string.format("%s -> %s", "ProjectileFactory", tostring(self.__name)))
        local instance = {}
        return setmetatable(instance, self)
    end
}

DummyProjectileFactory = {
    ---@param self any
    ---@return table
    __call = function (self)
        -- LOG(string.format("%s -> %s", "ProjectileFactory", tostring(self.__name)))
        -- needs a hash part of one for the _c_object field
        local instance = {}
        return setmetatable(instance, self)
    end
}"#;

    #[test]
    fn class_lua_block() {
        assert_eq!(strip(CLASS_LUA_INPUT), CLASS_LUA_EXPECTED);
    }

    #[test]
    fn hint_with_content() {
        assert_eq!(strip("local t = {&1 &8, x = 1}"), "local t = {x = 1}");
        assert_eq!(strip("local t = {&4 &0, 1, 2, 3, 4}"), "local t = {1, 2, 3, 4}");
    }

    /// LOUD's spellings: no space between the hints, and the first field
    /// straight after them (`lazyvar.lua`, `system/MultiEvent.lua`).
    #[test]
    fn hint_without_spaces_or_comma() {
        assert_eq!(strip("result = {&1&4}"), "result = {}");
        assert_eq!(strip("self.EventCallbacks = {&1&1 n=0}"), "self.EventCallbacks = {n=0}");
    }

    #[test]
    fn leaves_normal_tables_intact() {
        assert_eq!(strip("local t = {}"), "local t = {}");
        assert_eq!(strip("local t = {1, 2, 3}"), "local t = {1, 2, 3}");
        assert_eq!(strip("local t = {x = 1}"), "local t = {x = 1}");
    }

    #[test]
    fn no_false_positive_on_bitand() {
        assert_eq!(strip("local x = {a & b}"), "local x = {a & b}");
    }
}
