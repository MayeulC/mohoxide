-- Differential probe: an Atlantis diving and surfacing bow first,
-- logged in a format both engines print identically ("PROBE ..." lines).
--   real game (Windows): dev/probes/run_faf.ps1 probe_dive 120
--   ours: cargo run -- --headless --no-ai --ticks 400 --script dev/probes/maps/probe_dive/probe.lua
-- Every tick from the dive order to 40 ticks after it surfaces again: the
-- orientation quaternion as GetOrientation gives it, the nose's height (the
-- vertical part of forward (0, 0, 1) turned by it, reading {x, y, z, w}),
-- the height under the water surface, the layer and the vertical states.
local armies = ListArmies()
for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end
local function depth(x, z) return GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) end
local site
for z = 100, 924, 8 do
    for x = 100, 924, 8 do
        if not site and depth(x, z) > 10 then site = { x, z } end
    end
end
local sx, sz = site[1], site[2]
local sub = CreateUnitHPR('ues0401', armies[1], sx, GetSurfaceHeight(sx, sz), sz, 0, 0, 0)
local function line(phase, i)
    local q = sub:GetOrientation()
    local p = sub:GetPosition()
    local s = sub:GetCurrentLayer()
    for _, n in { 'MovingUp', 'MovingDown' } do
        if sub:IsUnitState(n) then s = s .. '+' .. n end
    end
    local nose = 2 * (q[2] * q[3] - q[4] * q[1])
    LOG(string.format('PROBE %s %d %s q %.5f %.5f %.5f %.5f nose %.5f y %.4f', phase, i, s, q[1], q[2], q[3], q[4], nose, p[2] - GetSurfaceHeight(p[1], p[3])))
end
-- One transition and 40 ticks after it ends (or 250 ticks at most).
local function watch(phase, layer)
    local after
    for i = 0, 250 do
        line(phase, i)
        if not after and sub:GetCurrentLayer() == layer and not sub:IsUnitState('MovingUp') and not sub:IsUnitState('MovingDown') then after = i end
        if after and i >= after + 40 then return end
        WaitTicks(1)
    end
end

ForkThread(function()
    WaitTicks(10)
    line('rest', 0)
    IssueDive({ sub })
    watch('dive', 'Sub')
    IssueDive({ sub })
    watch('surface', 'Water')
    LOG('PROBE done')
    if EndGame then EndGame() end
end)
