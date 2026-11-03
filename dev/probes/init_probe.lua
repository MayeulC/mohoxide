-- FA init file for differential probes: the stock game data (read-only, from
-- the sibling `Supreme Commander Forged Alliance` folder) plus this repo's
-- probe maps under /maps. Used by dev/probes/run_fa.sh.
path = {}

local function mount_dir(dir, mountpoint)
    table.insert(path, { dir = dir, mountpoint = mountpoint })
end

local game = InitFileDir .. '\\..\\..\\..\\Supreme Commander Forged Alliance'
mount_dir(InitFileDir .. '\\maps', '/maps')
mount_dir(game .. '\\gamedata\\*.scd', '/')
mount_dir(game, '/')

hook = { '/schook' }
protocols = {}
