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
