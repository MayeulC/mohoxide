-- moho's CLobby and CDiscoveryService for the interface (`InternalCreateLobby`,
-- `InternalCreateDiscoveryService`; FAF's lobbyComm.lua subclasses them).
--
-- Protocol 'None' (FAF's Skirmish button) is a lobby with no network: no
-- peers, nothing sent or received. 'UDP' / 'TCP' (the LAN screen's Create and
-- Join, Direct Connect) is networked: the engine's connection mesh
-- (`ui/net_lobby.rs`, `net::core`), whose events a pump thread turns into
-- FAF's callbacks each frame. Engine callbacks run the way moho runs them,
-- after the call that causes them. `LaunchGame` leaves the game configuration
-- for the renderer (`__engine_launch_request`, taken by
-- `UserVm::take_launch_request`), which starts the session; in a networked
-- lobby the host's launch goes to every peer, which launch the same game.
local methods = moho.lobby_methods

-- The local player's peer id in an offline lobby (FAF's `UILobbyPeerId`).
local LOCAL_ID = '0'

-- Runs `fn` on a later frame, as moho delivers its lobby events.
local function later(fn)
    ForkThread(function()
        WaitSeconds(0)
        fn()
    end)
end

local function state(self) return rawget(self, '_engine_lobby') end

-- Adds a line to the lobby's chat.
local chat

-- Starts the game this lobby launched (or was told to launch by the host).
-- Not without the host's map: the game would crash on loading it.
local function launch(self, config, seed, delay)
    local s = state(self)
    if s.launched then return end
    if s.mapFailed then
        chat("Can't start: the map " .. s.mapFailed .. " couldn't be downloaded. Leave the game, or get the map and rejoin.")
        return
    end
    s.launched = true
    rawset(_G, '__engine_launch_request', {
        Config = config,
        LocalID = s.net and __net_local_id() or LOCAL_ID,
        Seed = seed,
        Delay = delay,
    })
    later(function() self:GameLaunched() end)
end

-- The host's internet join code, in the lobby chat and on the clipboard:
-- friends paste it into Direct Connect's address box (iroh: no port
-- forwarding).
local function showJoinCode()
    local code = __net_join_code()
    if not code then return end
    local copied = CopyToClipboard(code)
    local ok, lobby = pcall(import, '/lua/ui/lobby/lobby.lua')
    if ok and lobby.AddChatText then
        pcall(lobby.AddChatText, 'Internet join code' .. (copied and ' (copied to the clipboard)' or '') .. ': ' .. code)
        pcall(lobby.AddChatText, 'Friends paste it into Direct Connect (IP Address box, any port).')
    end
end

chat = function(text)
    local ok, lobby = pcall(import, '/lua/ui/lobby/lobby.lua')
    if ok and lobby.AddChatText then pcall(lobby.AddChatText, text) end
end

-- One engine event as FAF's callback.
local function dispatch(self, s, e)
    local kind = e.kind
    if kind == 'hosting' then
        s.host = true
        self:Hosting()
        showJoinCode()
    elseif kind == 'welcomed' then
        self:ConnectionToHostEstablished(e['local'], s.name, e.host)
    elseif kind == 'join_failed' then
        -- FAF builds its lobby screen (`GUI.panel`) only once the
        -- host lets us in, and parents its "Connection failed"
        -- dialog to it: a join turned away at the door (another
        -- version, a full lobby) failed there and left the screen
        -- hanging. Then the same message is shown here.
        local ok, err = pcall(self.ConnectionFailed, self, e.reason)
        if not ok then
            WARN('lobby: ConnectionFailed failed (' .. tostring(err) .. '); showing the reason ourselves')
            local UIUtil = import('/lua/ui/uiutil.lua')
            UIUtil.ShowInfoDialog(GetFrame(0), LOCF('<LOC lob_0001>Connection failed: %s', e.reason), '<LOC _OK>', function()
                import('/lua/ui/lobby/lobby.lua').ReturnToMenu(false)
            end)
        end
    elseif kind == 'peer_disconnected' then
        self:PeerDisconnected(e.name, e.id)
    elseif kind == 'data' then
        local data = e.data
        if type(data) == 'table' then
            data.SenderID = e.from
            data.SenderName = e.name
            self:DataReceived(data)
        end
    elseif kind == 'eject' then
        self:Ejected(e.reason)
    elseif kind == 'launch' then
        -- The host's launch (seed and delay). FAF's lobby also has
        -- every client call LaunchGame itself (its 'Launch' message);
        -- the game starts once both have happened, with FAF's
        -- configuration and the host's seed.
        if not s.host then
            s.engineLaunch = e
            if s.pendingConfig then
                launch(self, s.pendingConfig, e.seed, e.delay)
            else
                -- Without FAF's call (a lobby that doesn't make it),
                -- the host's configuration after a moment.
                ForkThread(function()
                    WaitSeconds(2)
                    if not s.launched then launch(self, e.config, e.seed, e.delay) end
                end)
            end
        end
    end
end

-- The map a host's message names: its game options, or the whole game
-- (`GameInfo` on joining, `Launch`).
local function scenarioOf(data)
    local options = data.Options
    if type(data.GameInfo) == 'table' then options = data.GameInfo.GameOptions end
    local file = type(options) == 'table' and options.ScenarioFile
    if type(file) == 'string' and file ~= '' then return file end
end

-- A host's message naming a map we don't have: its download from FAF's
-- vault starts (`net::map_download`; what the FAF client does before the
-- game starts). True while the message must wait for it.
local function waitsForMap(s, e)
    if e.kind ~= 'data' or type(e.data) ~= 'table' then return false end
    local file = scenarioOf(e.data)
    if not file then return false end
    local download = __map_download(file)
    if download and download.state == 'done' then s.mapFailed = nil end
    if not download or download.state ~= 'downloading' then return false end
    s.mapFailed = nil
    if not s.mapDownload or s.mapDownload.file ~= file then
        s.mapDownload = { file = file, started = CurrentTime() }
    end
    return true
end

-- Megabytes for the chat: "5 MB", "0.4 MB".
local function megabytes(bytes)
    local mb = bytes / (1024 * 1024)
    if mb < 10 then return string.format('%.1f MB', mb) end
    return string.format('%d MB', math.floor(mb + 0.5))
end

-- Seconds a download runs before the chat shows its progress, then between
-- progress lines: most vault maps are a few MB and are done before the first.
local ProgressAfter = 2
local ProgressEvery = 3

-- The chat's lines for a map download: what and how big once the vault says
-- (or after a moment if it doesn't), progress only while a big or slow one
-- is still going, then done or why not. True once it is over.
local function reportMapDownload(s)
    local current = s.mapDownload
    local download = __map_download(current.file)
    local elapsed = CurrentTime() - current.started
    local name = string.match(current.file, '^/[Mm][Aa][Pp][Ss]/([^/]+)/') or current.file
    if not current.announced and (not download or download.state ~= 'downloading' or download.total or elapsed >= 1) then
        current.announced = true
        local size = download and download.total and (' (' .. megabytes(download.total) .. ')') or ''
        chat('Downloading the map ' .. name .. size .. " from FAF's vault...")
        current.nextProgress = current.started + ProgressAfter
    end
    if not download or download.state ~= 'downloading' then
        if download and download.state == 'done' then
            chat('Downloaded the map.')
        else
            chat("Couldn't download the map: " .. tostring(download and download.reason or 'unknown'))
            -- Blocks launching (`launch`): the map isn't here.
            s.mapFailed = name
        end
        return true
    end
    if current.announced and CurrentTime() >= current.nextProgress then
        current.nextProgress = CurrentTime() + ProgressEvery
        local received = download.received or 0
        if download.total and download.total > 0 then
            chat(string.format('Downloading the map: %s of %s (%d%%)', megabytes(received), megabytes(download.total), math.floor(100 * received / download.total)))
        else
            chat('Downloading the map: ' .. megabytes(received))
        end
    end
    return false
end

-- Networked lobbies: turn the engine's events into FAF's callbacks. A
-- message naming a map we don't have waits, with every event after it, until
-- the map is downloaded (or that fails, and FAF's lobby says it's missing).
local function pump(self)
    local s = state(self)
    local nextBeacon = 0
    s.held = {}
    while not s.destroyed do
        for _, e in __net_poll() do
            table.insert(s.held, e)
        end
        if s.mapDownload and reportMapDownload(s) then
            s.mapDownload = nil
        end
        while not s.mapDownload and not s.destroyed and table.getn(s.held) > 0 do
            local e = table.remove(s.held, 1)
            if waitsForMap(s, e) then
                table.insert(s.held, 1, e)
            else
                dispatch(self, s, e)
            end
        end
        -- Hosts announce the game on the LAN (the engine sends at most one
        -- beacon a second).
        if s.host and not s.destroyed and CurrentTime() >= nextBeacon then
            nextBeacon = CurrentTime() + 1
            local ok, info = pcall(self.GameConfigRequested, self)
            if ok and type(info) == 'table' then
                info.Protocol = s.protocol
                info.Hostname = s.name
                __net_beacon(info)
            end
        end
        WaitSeconds(0)
    end
end

function InternalCreateLobby(class, protocol, localPort, maxConnections, playerName, playerUID, natTraversalProvider)
    local lobby = setmetatable({}, class)
    local networked = protocol == 'UDP' or protocol == 'TCP'
    local s = { name = playerName or 'Player', host = false, destroyed = false, protocol = protocol, net = false }
    lobby._engine_lobby = s
    if networked then
        local ok = __net_create(localPort or 0, s.name, playerUID and tostring(playerUID) or nil)
        if not ok then
            -- FAF's lobby reports a nil lobby as "Failed to create lobby".
            return nil
        end
        s.net = true
        ForkThread(pump, lobby)
    end
    return lobby
end

methods.GetLocalPlayerID = function(self)
    local s = state(self)
    return s.net and (__net_local_id() or '-1') or LOCAL_ID
end
methods.GetLocalPlayerName = function(self) return state(self).name end
methods.GetLocalPort = function(self) return state(self).net and __net_port() or nil end
methods.IsHost = function(self) return state(self).host end
methods.GetPeers = function(self) return state(self).net and __net_peers() or {} end
methods.GetPeer = function(self, peerId)
    if not state(self).net then return nil end
    peerId = tostring(peerId)
    for _, p in __net_peers() do
        if p.id == peerId then return p end
    end
    -- FAF reads fields of the result: an unknown peer is a quiet, empty one.
    return { id = peerId, name = '', ping = 0, quiet = 0, status = 'None', establishedPeers = {} }
end
methods.MakeValidGameName = function(self, name) return name end
methods.MakeValidPlayerName = function(self, peerId, name) return name end
methods.BroadcastData = function(self, data)
    if state(self).net then __net_broadcast(data) end
end
methods.SendData = function(self, peerId, data)
    if state(self).net then __net_send(tostring(peerId), data) end
end
-- The engine connects the mesh itself (the host tells joiners about the
-- other members), as moho does on a LAN.
methods.ConnectToPeer = function(self, address, name, peerId) end
methods.DisconnectFromPeer = function(self, peerId)
    if state(self).net then __net_disconnect(tostring(peerId)) end
end
methods.EjectPeer = function(self, peerId, reason)
    if state(self).net then __net_disconnect(tostring(peerId), reason or 'KickedByHost') end
end
methods.DebugDump = function(self) LOG(repr(state(self))) end
methods.Destroy = function(self)
    local s = state(self)
    if s.destroyed then return end
    s.destroyed = true
    -- A launched game keeps the connection (the sim plays over it).
    if s.net then __net_close() end
end

methods.HostGame = function(self)
    local s = state(self)
    if s.net then
        __net_host()
        return
    end
    s.host = true
    later(function()
        if not s.destroyed then self:Hosting() end
    end)
end

methods.JoinGame = function(self, address, remotePlayerName, remotePlayerPeerId)
    local s = state(self)
    if s.net then
        __net_join(tostring(address))
        return
    end
    later(function()
        if not s.destroyed then self:ConnectionFailed('Networked games need a UDP lobby') end
    end)
end

methods.LaunchGame = function(self, gameConfig)
    local s = state(self)
    if s.net and s.host then
        local seed, delay = __net_launch(gameConfig)
        launch(self, gameConfig, seed, delay)
    elseif s.net then
        -- A joiner: the host decides the seed; start once its launch is in.
        s.pendingConfig = gameConfig
        if s.engineLaunch then
            launch(self, gameConfig, s.engineLaunch.seed, s.engineLaunch.delay)
        end
    else
        launch(self, gameConfig)
    end
end

-- The LAN screen's list of games (moho's CDiscoveryService): the engine
-- listens for host beacons; a pump thread calls the class's GameFound /
-- GameUpdated / RemoveGame (0-based indices, as moho).
local discovery = moho.discovery_service_methods

function InternalCreateDiscoveryService(class)
    local service = setmetatable({}, class)
    local s = { destroyed = false }
    service._engine_discovery = s
    if not __disc_open() then
        WARN('LAN discovery unavailable (the discovery port is in use?)')
    end
    ForkThread(function()
        while not s.destroyed do
            for _, f in __disc_poll() do
                if s.destroyed then break end
                if f.kind == 'found' then
                    service:GameFound(f.index, f.info)
                elseif f.kind == 'updated' then
                    service:GameUpdated(f.index, f.info)
                elseif f.kind == 'removed' then
                    service:RemoveGame(f.index)
                end
            end
            WaitSeconds(0.25)
        end
    end)
    return service
end

discovery.GetGameCount = function(self) return __disc_count() end
discovery.Reset = function(self)
    for _, f in __disc_poll(true) do
        self:RemoveGame(f.index)
    end
end
discovery.Destroy = function(self)
    local s = rawget(self, '_engine_discovery')
    if s and not s.destroyed then
        s.destroyed = true
        __disc_close()
    end
end

-- In-game chat from other players (`SessionSendChatMessage`), handed to
-- FAF's gamemain.ReceiveChat as moho does, and (networked games) FAF's
-- disconnect dialog kept current as moho does: it opens itself when a player
-- has been quiet for 5 s or eject votes are pending. Polls cheaply when there
-- is nothing to do.
ForkThread(function()
    local nextDialog = 0
    while true do
        -- Pauses (anyone's) for FAF's pause bookkeeping: who paused, so
        -- only they can resume at once (`/lua/ui/game/pause.lua`).
        for _, e in __net_pause_poll() do
            local ok, gamemain = pcall(import, '/lua/ui/game/gamemain.lua')
            if ok then
                local done, err
                if e.paused then
                    done, err = pcall(gamemain.OnPause, e.by, 3)
                else
                    done, err = pcall(gamemain.OnResume)
                end
                if not done then WARN('pause: ' .. tostring(err)) end
            end
        end
        -- Checksums that differ: FAF's "Desync Detected" dialog (it shows
        -- once until closed, and not again after "Don't tell me any more").
        for _, e in __net_desync_poll() do
            local ok, uimain = pcall(import, '/lua/ui/uimain.lua')
            if ok then
                local done, err = pcall(uimain.ShowDesyncDialog, e.beat, e.strings)
                if not done then WARN('desync dialog: ' .. tostring(err)) end
            end
        end
        if CurrentTime() >= nextDialog then
            nextDialog = CurrentTime() + 0.5
            if __net_session_clients() then
                local ok, uimain = pcall(import, '/lua/ui/uimain.lua')
                if ok then
                    local done, err = pcall(uimain.UpdateDisconnectDialog)
                    if not done then WARN('disconnect dialog: ' .. tostring(err)) end
                end
            end
        end
        local received = __net_chat_poll()
        if table.getn(received) > 0 then
            local ok, gamemain = pcall(import, '/lua/ui/game/gamemain.lua')
            if ok then
                for _, m in received do
                    local done, err = pcall(gamemain.ReceiveChat, m.sender, m.data)
                    if not done then WARN('chat: ' .. tostring(err)) end
                end
            end
        end
        WaitSeconds(0.1)
    end
end)
