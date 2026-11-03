# Picture-in-picture popups
`/lua/ui/game/pip.lua` (engine Lua, `src/lua_overrides/`) opens small world
views that follow a unit for a few seconds: an ACU dying, an
experimental being started. A popup is only moho's own pieces (a
`UIWorldView` on its own camera, `PiP1`..`PiPn`, and `TrackEntities`), so the
module runs unchanged in FAF.

- **Sim side**: `import('/lua/sim/pip.lua').Request(unit, reason, opts)` puts
  `{reason, army, entities, position, ...}` in `Sync.PiPRequests`, only on
  clients whose focus army is the unit's or allied to it, or observing (an
  enemy's event never reaches a player's interface). Sync isn't sim state, so
  this doesn't touch determinism. Engine hooks call it from
  `ACUUnit.OnKilled` (`acu_death`) and `Unit.OnStartBeingBuilt` on an
  `EXPERIMENTAL` (`experimental_started`).
- **UI side**: `gamemain.CreateUI` (engine hook) registers `OnRequests` with
  `AddOnSyncHashedCallback`; it checks the focus army again (it can change
  after the sim wrote) and the player's option (`pip_<reason>`, Interface
  tab, engine hook on `options.lua`), then calls `Show`.
- **For mods**: `Show{entities, position, zoom, duration, title, priority,
  id, screen}` returns the popup; `Close(popup or id)`, `Popups()`,
  `SetSlots(n)` (default 2; a full set gives way to a higher priority),
  `Reasons[reason]` (per-reason defaults, `false` turns one off), `Anchor`
  (where they stack), `OnClick(popup)` (default: the main camera goes
  there). `screen` is the monitor: 0 the main window, 1 the second screen
  (FAF's second head, `GetFrame(1)`); without that screen, the main window.
- Each popup is FAF's `Window` with the minimap's textures: title bar,
  close button, drag to move; slot k (camera `PiPk`) keeps the place a
  player gives it (`pip_window_<k>` in the prefs), else stacks up from
  `Anchor`. Its middle is `UIRP_UnderWorld`, as the minimap's, so the
  picture draws over it.
- Moho's under-world pass: world views and controls marked
  `UIRP_UnderWorld` draw together by depth, the rest of the interface over
  them. The renderer draws every world view first, so the draw list cuts a
  marked item out of each world view above it (`controls::under_world`);
  over the views below it, it draws as any item.
- A camera request for a view opened this frame waits for its camera
  (made when the view first shows) up to 30 frames.
- `dev/scenes/ui/pip.lua` shows both: `--session --no-ai --no-audio --script
  dev/scenes/ui/pip.lua --screenshot pip.png --screenshot-tick 80`.
- Missing: `NoseCam` for the ACU's death shot.

## PiP API
Sim, `/lua/sim/pip.lua`:
- `Request(unit, reason, opts)`: a popup on `unit` for the unit's side, its
  allies and observers. `reason` names it (`'acu_death'`,
  `'experimental_started'`, a mod's own); `opts` overrides `zoom`,
  `duration`, `title`, `priority`.

UI, `/lua/ui/game/pip.lua`:
- `Show{entities, position, zoom, duration, title, priority, id, screen}`
  opens one and returns it (nil if it lost to higher-priority popups):
  `entities` ids or units to follow, `position` `{x, y, z}`, `duration`
  seconds (nil: until closed), `id` replaces the popup with that id,
  `screen` 0 the main window / 1 the second.
- `Close(popup or id)`, `Popups()` (oldest first).
- `SetSlots(n)` (default 2; full: a new one replaces the lowest priority
  if it's at least as high).
- `Reasons[reason] = {title, zoom, duration, priority}` (`false` turns
  it off; no entry: `Default`).
- `Anchor = {right, bottom, width, height, gap, scoreboard}`: where popups
  stack until moved, and the picture's size; with `scoreboard` (default),
  popups on the main screen take the score panel's width and right edge
  (the picture keeps `width` x `height`'s shape).
- `Textures = {tl, tm, tr, ml, m, mr, bl, bm, br, borderColor}`: the
  frame (FAF's `Window` texture table; the minimap's by default). Read
  at each `Show`, so a change reskins popups opened after it; `m` always
  sits under the picture.
- `OnClick(popup)`: clicking the picture (default: the main camera goes
  there). `OnRequests(list)`: the sim's requests (the focus-army check).
- Options: a pref `pip_<reason>` set to 0 turns a reason off (the
  Interface tab has `pip_acu_death`, `pip_experimental_started`).

## Example mod: ACU under 10% health
`OnHealthChanged` only fires at quarter steps, so the sim side hooks
`OnDamage`, asks once on crossing 10% and re-arms above 25%. The flag is
sim state set the same on every client; only `Request`'s delivery differs.

`/hook/lua/sim/units/ACUUnit.lua`:
```lua
do
    local Base = ACUUnit
    ACUUnit = ClassUnit(Base) {
        OnDamage = function(self, instigator, amount, vector, damageType)
            Base.OnDamage(self, instigator, amount, vector, damageType)
            local fraction = self:GetHealth() / self:GetMaxHealth()
            if fraction < 0.1 and not self.PiPLowHealth and not self.Dead then
                self.PiPLowHealth = true
                import('/lua/sim/pip.lua').Request(self, 'acu_low_health')
            elseif fraction > 0.25 then
                self.PiPLowHealth = nil
            end
        end,
    }
end
```

`/hook/lua/ui/game/pip.lua`:
```lua
Reasons.acu_low_health = { title = 'ACU below 10%', zoom = 12, duration = 6, priority = 25 }
```

`/hook/lua/options/options.lua` (optional, a toggle on the Interface tab):
```lua
table.insert(options.ui.items, {
    title = 'Popup: ACU Below 10%', key = 'pip_acu_low_health', type = 'toggle', default = 1,
    custom = { states = { { text = '<LOC _Off>', key = 0 }, { text = '<LOC _On>', key = 1 } } },
})
```
It's installed as `dev/vault/mods/pip_acu_low_health`; `dev/scenes/ui/pip_low_health.lua`
wears ARMY_1's ACU down to show it:
`cargo run -- --session --vault dev/vault --mod 5e3c1f0a-pip-acu-low-health --script dev/scenes/ui/pip_low_health.lua`.

