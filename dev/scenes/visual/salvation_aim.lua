-- The Salvation (XAB2307) aiming at a target 200 to its west:
-- its barrel is modelled raised 65 degrees, so the aim's pitch must be taken
-- from that rest pose (moho measures it in the pitch bone's own frame), or
-- the high-arc pitch on top of it swings the barrel back over the turret.
-- Logs AIMCHECK lines: each muzzle's direction against its target's bearing.
-- A UEF T3 artillery (UEB2302 Duke) and a UEF T1 tank aim at targets of their
-- own, for comparison. Each pair is its own two armies, the rest allied, so
-- every gun only sees its own target, spotted by a scout of its own army.
-- cargo run -- --no-ai --armies 6 --fog none --script dev/scenes/visual/salvation_aim.lua --camera 384,896,40
--   (the Duke: --camera 360,740,40; the tank: --camera 384,696,30)
ForkThread(function()
    WaitTicks(2)
    for a = 1, 6 do
        for b = 1, 6 do
            if a ~= b then SetAlliance(a, b, 'Ally') end
        end
        local brain = GetArmyBrain('ARMY_' .. a)
        brain:__engine_SetStorage('ENERGY', 5000000) brain:GiveResource('ENERGY', 5000000)
    end
    SetAlliance(1, 2, 'Enemy')
    SetAlliance(3, 4, 'Enemy')
    SetAlliance(5, 6, 'Enemy')
    local function at(bp, army, x, z) return CreateUnitHPR(bp, army, x, GetTerrainHeight(x, z), z, 0, 0, 0) end
    local pairs = {
        { at('xab2307', 'ARMY_1', 384, 896), at('uel0303', 'ARMY_2', 184, 896) },
        { at('ueb2302', 'ARMY_3', 360, 740), at('uel0303', 'ARMY_4', 360, 940) },
        { at('uel0201', 'ARMY_5', 384, 696), at('uel0201', 'ARMY_6', 399, 696) },
    }
    for _, p in pairs do
        local shooter, target = p[1], p[2]
        target:SetMaxHealth(1e7) target:SetHealth(nil, 1e7) target:SetFireState(1)
        shooter:SetFireState(0)
        -- A scout of the shooter's army beside the target: its weapon needs
        -- the target in its army's sight.
        local q = target:GetPosition()
        local scout = at('uel0101', shooter:GetArmy(), q[1], q[3] + 8)
        scout:SetFireState(1)
    end
    WaitTicks(5)
    for _, p in pairs do IssueAttack({ p[1] }, p[2]) end
    for _ = 1, 90 do
        WaitTicks(10)
        for _, p in pairs do
            local u, t = p[1], p[2]
            if not u:BeenDestroyed() then
                local x, y, z = u:GetBoneDirection('Turret_Muzzle')
                local a, b = u:GetPosition(), t:GetPosition()
                local bearing = math.deg(math.atan2(b[1] - a[1], b[3] - a[3]))
                local heading = math.deg(math.atan2(x, z))
                local off = heading - bearing
                if off > 180 then off = off - 360 elseif off < -180 then off = off + 360 end
                LOG(string.format('AIMCHECK tick %d %s: barrel heading %.1f elevation %.1f, target bearing %.1f, off %.1f deg',
                    GetGameTick(), u.UnitId, heading, math.deg(math.asin(math.max(-1, math.min(1, y)))), bearing, off))
            end
        end
    end
end)
