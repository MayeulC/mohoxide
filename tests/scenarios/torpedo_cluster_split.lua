-- Scenario test (tests/scenarios.rs): The Solace (xaa0306) drops
-- torpedo clusters that split in `OnEnterWater` into two torpedoes, each
-- with half the damage. Moho runs `OnEnterWater` before the step's move
-- is committed (`Projectile::CheckCollision`, MohoEngine.dll 0x1027e430,
-- after `SetPendingTransform`), so the cluster is still above the water
-- there and its torpedoes start above it: they enter the water next tick
-- and home on the target from their own `OnEnterWater`. The real game's
-- (dev/probes/maps/probe_auditgaps, lane g4) hit a surfaced frigate 12
-- times a pass for 417 each.
rawset(_G, "ScenarioResult", { done = false, failures = {}, notes = {} })
local function check(ok, msg) if not ok then table.insert(ScenarioResult.failures, msg) end end
local function note(msg) table.insert(ScenarioResult.notes, msg) end
ForkThread(function()
    local ok, err = pcall(function()
        WaitTicks(2)
        local function depth(x, z) return GetSurfaceHeight(x, z) - GetTerrainHeight(x, z) end
        local site
        local w, h = GetMapSize()
        for z = 100, h - 100, 8 do
            for x = 100, w - 100, 8 do
                if not site and depth(x, z) > 10 and depth(x - 30, z) > 10 then site = { x, z } end
            end
        end
        check(site ~= nil, 'found deep water')
        if not site then return end
        local sx, sz = site[1], site[2]
        local target = CreateUnitHPR('ues0103', 'ARMY_2', sx, GetSurfaceHeight(sx, sz), sz, 0, 0, 0)
        target:SetMaxHealth(30000) target:SetHealth(nil, 30000)
        target:SetFireState(1)
        local hits, wrong = 0, {}
        local on = target.OnDamage
        target.OnDamage = function(self, inst, amount, vec, kind)
            on(self, inst, amount, vec, kind)
            hits = hits + 1
            if math.abs(amount - 417) > 0.01 then table.insert(wrong, amount) end
        end
        local bomber = CreateUnitHPR('xaa0306', 'ARMY_1', sx + 40, GetSurfaceHeight(sx + 40, sz) + 20, sz, 0, 0, 0)
        -- Where the cluster is when it enters the water: above the surface.
        local entries, above = 0, 0
        local weapon = bomber:GetWeaponByLabel('Bomb')
        local create = weapon.CreateProjectileAtMuzzle
        weapon.CreateProjectileAtMuzzle = function(self, muzzle)
            local p = create(self, muzzle)
            local enter = p.OnEnterWater
            p.OnEnterWater = function(proj)
                local x, y, z = proj:GetPositionXYZ()
                entries = entries + 1
                if y >= GetSurfaceHeight(x, z) then above = above + 1 end
                enter(proj)
            end
            return p
        end
        IssueAttack({ bomber }, target)
        for _ = 1, 150 do
            WaitTicks(1)
            if hits >= 12 then break end
        end
        WaitTicks(10)
        note(string.format('%d clusters entered the water (%d above it in the callback), %d hits', entries, above, hits))
        check(entries >= 6, 'a pass drops 6 clusters: ' .. entries)
        check(above == entries, 'a cluster is above the water in OnEnterWater: ' .. above .. ' of ' .. entries)
        check(hits == 12, 'a pass hits 12 times (6 clusters x 2 torpedoes): ' .. hits)
        check(table.getn(wrong) == 0, 'each torpedo does half the cluster damage, 417: ' .. table.concat(wrong, ' '))
    end)
    if not ok then check(false, tostring(err)) end
    ScenarioResult.done = true
end)
