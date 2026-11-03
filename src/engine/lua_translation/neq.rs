/// Rewrites SupCom's `!=` (not-equal) operator to Luau's `~=`.
///
/// SupCom's custom Lua 5.0 interpreter accepts `!=`; Luau does not.
pub fn rewrite(source: &str) -> String {
    source.replace("!=", "~=")
}

#[cfg(test)]
mod tests {
    use super::*;

    // Verbatim block from fa/lua/system/import.lua lines 128-145.
    // Only the `!=` on line 137 should change; all other syntax must survive intact.
    const IMPORT_LUA_INPUT: &str = r#"    ---@type ModuleInfo
    local moduleinfo = {
        name = name,
        used_by = {},
        track_imports = true,
    }

    -- Define a new 'import' function customized for the module, to track import dependencies
    local _import = function(name2, isLazy)
        if name2:sub(1, 1) != '/' then
            name2 = FileCollapsePath(name .. '/../' .. name2)
        end
        local module2 = import(name2, isLazy) -- this will use the global import
        if __modules[name].__moduleinfo.track_imports then
            module2.__moduleinfo.used_by[name] = true
        end
        return module2
    end

    -- set up an environment for the new module"#;

    const IMPORT_LUA_EXPECTED: &str = r#"    ---@type ModuleInfo
    local moduleinfo = {
        name = name,
        used_by = {},
        track_imports = true,
    }

    -- Define a new 'import' function customized for the module, to track import dependencies
    local _import = function(name2, isLazy)
        if name2:sub(1, 1) ~= '/' then
            name2 = FileCollapsePath(name .. '/../' .. name2)
        end
        local module2 = import(name2, isLazy) -- this will use the global import
        if __modules[name].__moduleinfo.track_imports then
            module2.__moduleinfo.used_by[name] = true
        end
        return module2
    end

    -- set up an environment for the new module"#;

    #[test]
    fn import_lua_block() {
        assert_eq!(rewrite(IMPORT_LUA_INPUT), IMPORT_LUA_EXPECTED);
    }

    #[test]
    fn multiple_on_one_line() {
        assert_eq!(
            rewrite("if a != b and c != d then"),
            "if a ~= b and c ~= d then",
        );
    }

    #[test]
    fn leaves_valid_luau_alone() {
        assert_eq!(rewrite("if a ~= b then"), "if a ~= b then");
        assert_eq!(rewrite("local x = 1"), "local x = 1");
    }
}
