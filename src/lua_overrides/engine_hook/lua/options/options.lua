
-- Engine hook: the picture-in-picture popups' options (Interface tab), read
-- by `/lua/ui/game/pip.lua` as `pip_<reason>`.
do
    local states = { { text = "<LOC _Off>", key = 0 }, { text = "<LOC _On>", key = 1 } }
    table.insert(options.ui.items, {
        title = "<LOC pip_0002>Popup: ACU Destroyed",
        tip = "<LOC pip_0003>Show a picture-in-picture popup when an ACU on your team dies (observers: any ACU).",
        key = 'pip_acu_death',
        type = 'toggle',
        default = 1,
        custom = { states = states },
    })
    table.insert(options.ui.items, {
        title = "<LOC pip_0004>Popup: Experimental Started",
        tip = "<LOC pip_0005>Show a picture-in-picture popup when your team starts building an experimental (observers: any team).",
        key = 'pip_experimental_started',
        type = 'toggle',
        default = 1,
        custom = { states = states },
    })
end

-- Engine hook: the replay timeline's screen (Interface tab), read by
-- `/lua/ui/game/timeline.lua`.
table.insert(options.ui.items, {
    title = "<LOC timeline_0003>Replay Timeline",
    tip = "<LOC timeline_0004>Where a replay shows its timeline of major events (click it to jump there). Without a second screen, Second Screen shows it on the main one.",
    key = 'replay_timeline',
    type = 'toggle',
    default = 'main',
    custom = {
        states = {
            { text = "<LOC _Off>", key = 'off' },
            { text = "<LOC timeline_0005>Main Screen", key = 'main' },
            { text = "<LOC timeline_0006>Second Screen", key = 'second' },
        },
    },
    set = function(key, value, startup)
        if not startup and GetCurrentUIState() == 'game' then
            -- After the options are saved (the timeline reads them).
            ForkThread(function()
                WaitSeconds(0)
                import("/lua/ui/game/timeline.lua").Refresh()
            end)
        end
    end,
})

-- Engine hook: replay keyframes (Interface tab), read by the engine when a
-- replay starts (`UserVm::keyframe_settings`). A keyframe is the whole game
-- saved at a moment, so the timeline can jump back to it; another process
-- can play the replay ahead to write them before you get there.
table.insert(options.ui.items, {
    title = "<LOC keyframe_0001>Replay: Play Ahead",
    tip = "<LOC keyframe_0002>While you watch a replay, play it ahead in the background, saving keyframes so jumping anywhere on the timeline is quick. Uses spare processor cores and disk.",
    key = 'replay_play_ahead',
    type = 'toggle',
    default = 1,
    custom = { states = { { text = "<LOC _Off>", key = 0 }, { text = "<LOC _On>", key = 1 } } },
})
table.insert(options.ui.items, {
    title = "<LOC keyframe_0003>Replay: Keyframe Every",
    tip = "<LOC keyframe_0004>How often a replay's keyframes are saved: more often makes a jump quicker and uses more disk.",
    key = 'replay_keyframe_every',
    type = 'toggle',
    default = 60,
    custom = {
        states = {
            { text = "<LOC keyframe_0005>30 seconds", key = 30 },
            { text = "<LOC keyframe_0006>1 minute", key = 60 },
            { text = "<LOC keyframe_0007>2 minutes", key = 120 },
        },
    },
})
table.insert(options.ui.items, {
    title = "<LOC keyframe_0008>Replay: Keyframe Disk Space",
    tip = "<LOC keyframe_0009>Disk kept for replays' keyframes; the replays watched longest ago lose theirs first. An hour of a big game takes about 100 MB.",
    key = 'replay_keyframe_budget',
    type = 'toggle',
    default = 2048,
    custom = {
        states = {
            { text = "<LOC keyframe_0010>512 MB", key = 512 },
            { text = "<LOC keyframe_0011>2 GB", key = 2048 },
            { text = "<LOC keyframe_0012>8 GB", key = 8192 },
        },
    },
})

-- Engine hook: the cache of generated mip levels (Video tab). Textures
-- shipped without a full mip chain get the rest generated at load (as the
-- real game's D3DX does); the cache keeps them so the next load is quick.
-- Applied by the engine's own console commands; pruned once per run.
table.insert(options.video.items, {
    title = "<LOC texcache_0001>Texture Cache Disk Space",
    tip = "<LOC texcache_0002>Disk kept for textures the engine finished (mip levels they shipped without), so they load quickly next time. The ones used longest ago go first.",
    key = 'texture_cache_mb',
    type = 'toggle',
    default = 2048,
    set = function(key, value, startup)
        ConExecute("eng_TextureCacheMB " .. tostring(value))
    end,
    custom = {
        states = {
            { text = "<LOC texcache_0003>512 MB", key = 512 },
            { text = "<LOC texcache_0004>1 GB", key = 1024 },
            { text = "<LOC texcache_0005>2 GB", key = 2048 },
            { text = "<LOC texcache_0006>4 GB", key = 4096 },
            { text = "<LOC texcache_0007>8 GB", key = 8192 },
        },
    },
})
table.insert(options.video.items, {
    title = "<LOC texcache_0008>Texture Cache: Keep Unused",
    tip = "<LOC texcache_0009>How long a cached texture that hasn't been used is kept.",
    key = 'texture_cache_days',
    type = 'toggle',
    default = 90,
    set = function(key, value, startup)
        ConExecute("eng_TextureCacheDays " .. tostring(value))
    end,
    custom = {
        states = {
            { text = "<LOC texcache_0010>1 month", key = 30 },
            { text = "<LOC texcache_0011>3 months", key = 90 },
            { text = "<LOC texcache_0012>6 months", key = 180 },
            { text = "<LOC texcache_0013>1 year", key = 365 },
        },
    },
})
