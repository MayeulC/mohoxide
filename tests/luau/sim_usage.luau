--!strict
-- Type-checked against the generated sim.d.luau by `tests/api_types.rs`
-- (`definitions_type_check`): valid uses of the generated sim API must
-- produce no diagnostics.
local u: Unit = (nil :: any)
u:SetBuildRate(5)
local _rate: number = u:GetBuildRate()
local _e: Entity = u
local _l: Layer = "Land"
local cb: UnitCallbacks = (nil :: any)
cb.OnStopBeingBuilt(u, nil, "Air")
local _air: UnitBlueprintAir = { BankFactor = 1, Winged = true }
local _t: string = debug.traceback()
-- A declared overload and a typed stub.
local _a: { Unit }? = GetUnitsInRect({ x0 = 0, y0 = 0, x1 = 10, y1 = 10 })
local _b: { Unit }? = GetUnitsInRect(0, 0, 10, 10)
local v: Vector = (nil :: any)
DrawCircle(v, 5, "ffffffff")
-- Vectors, bones and armies from implemented functions.
local p: Vector = u:GetPosition("Turret")
local _px: number = p.x + p[2]
Warp(u, p, nil)
local _q: Vector? = u:CalculateWorldPositionFromRelative({ 0, 0, -5 })
-- Returned objects and lists.
local w: Weapon? = u:GetWeapon(1)
if w then
    local proj: Projectile? = w:CreateProjectile("Muzzle")
    if proj then
        proj:SetCollideEntity(true):SetCollideSurface(true)
    end
end
local brain: AIBrain? = GetArmyBrain("ARMY_1")
if brain then
    local _units: { Unit } = brain:GetListOfUnits((nil :: any), false)
end
