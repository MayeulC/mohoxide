-- Engine hook: an experimental starting to be built asks for a
-- picture-in-picture popup (`/lua/sim/pip.lua`); one finished, and a nuke
-- launched, are marked in the recording for the replay timeline
-- (`Replay.AddEvent`).
do
    local Base = Unit
    Unit = ClassUnit(Base) {
        OnStartBeingBuilt = function(self, builder, layer)
            Base.OnStartBeingBuilt(self, builder, layer)
            if self.Blueprint.CategoriesHash.EXPERIMENTAL then
                import("/lua/sim/pip.lua").Request(self, 'experimental_started')
            end
        end,

        OnStopBeingBuilt = function(self, builder, layer)
            Base.OnStopBeingBuilt(self, builder, layer)
            if self.Blueprint.CategoriesHash.EXPERIMENTAL then
                Replay.AddEvent('experimental_built', self)
            end
        end,

        OnNukeLaunched = function(self)
            Replay.AddEvent('nuke_launch', self)
            Base.OnNukeLaunched(self)
        end,
    }
end
