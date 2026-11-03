-- Engine hook: an ACU's death asks for a picture-in-picture popup
-- (`/lua/sim/pip.lua`) and is marked in the recording for the replay
-- timeline (`Replay.AddEvent`).
do
    local Base = ACUUnit
    ACUUnit = ClassUnit(Base) {
        OnKilled = function(self, instigator, type, overkillRatio)
            import("/lua/sim/pip.lua").Request(self, 'acu_death')
            Replay.AddEvent('acu_death', self)
            Base.OnKilled(self, instigator, type, overkillRatio)
        end,
    }
end
