-- Differential probe: what each faction's tactical missile defence does to a
-- tactical missile (shoots it down, zaps it, turns it away, or misses).
-- Logged in a format both engines print identically.
--   real game (Windows): dev/probes/run_faf.ps1 probe_tmd 60
--   ours: cargo run -- --headless --no-ai --ticks 500 --script dev/probes/maps/probe_tmd/probe.lua
-- One lane per faction, 100 apart: a T1 power generator (army 1) with the
-- faction's TMD 6 beside it, and the faction's TML (army 2) 200 away firing
-- one missile at the generator. Every 2 ticks while it flies: the tick,
-- the faction, the missile's position. When it's gone: where, and the
-- generator's health (or 'dead').
local armies = ListArmies()

for _, brain in ArmyBrains do
    for _, u in brain:GetListOfUnits(categories.ALLUNITS, false) or {} do
        if not u:BeenDestroyed() then u:Destroy() end
    end
end

local function make(bp, army, x, z)
    return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0)
end
local function f1(n) return string.format('%.1f', n) end

ForkThread(function()
    LOG('PROBE start ', GetGameTick())
    local lanes = {}
    for i, f in {'ue', 'ua', 'ur', 'xs'} do
        local z = 550 + i * 100
        local lane = { f = f }
        lane.site = make(f .. 'b1101', armies[1], 100.5, z)
        lane.tmd = make(f .. 'b4201', armies[1], 106.5, z)
        lane.tmd:SetCanTakeDamage(false)
        lane.tml = make(f .. 'b2108', armies[2], 300.5, z)
        local w = lane.tml:GetWeapon(1)
        local create = w.CreateProjectileAtMuzzle
        w.CreateProjectileAtMuzzle = function(self, muzzle)
            local p = create(self, muzzle)
            if p and not lane.p then
                lane.p = p
                LOG('PROBE ', GetGameTick(), ' ', f, ' launched')
            end
            return p
        end
        table.insert(lanes, lane)
    end
    WaitTicks(5)
    for _, lane in lanes do
        lane.full = lane.site:GetHealth()
        LOG('PROBE ', lane.f, ' site health ', f1(lane.full))
        lane.tml:GiveTacticalSiloAmmo(1)
        IssueTactical({lane.tml}, lane.site:GetPosition())
    end
    for _ = 1, 400 do
        WaitTicks(1)
        local flying = false
        for _, lane in lanes do
            if not lane.p then
                flying = true
            elseif not lane.done then
                if lane.p:BeenDestroyed() then
                    lane.done = true
                    local x, y, z = lane.last[1], lane.last[2], lane.last[3]
                    local h = lane.site:BeenDestroyed() and 'dead' or f1(lane.site:GetHealth())
                    LOG('PROBE ', GetGameTick(), ' ', lane.f, ' gone at ', f1(x), ',', f1(y), ',', f1(z), ' site ', h)
                else
                    flying = true
                    local x, y, z = lane.p:GetPositionXYZ()
                    lane.last = {x, y, z}
                    if math.mod(GetGameTick(), 2) == 0 then
                        LOG('PROBE ', GetGameTick(), ' ', lane.f, ' ', f1(x), ',', f1(y), ',', f1(z))
                    end
                end
            end
        end
        if not flying then break end
    end
    WaitTicks(10)
    for _, lane in lanes do
        LOG('PROBE ', lane.f, ' final site ', lane.site:BeenDestroyed() and 'dead' or f1(lane.site:GetHealth()))
    end
    LOG('PROBE done')
end)
