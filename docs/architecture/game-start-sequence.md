The full game load sequence:

  ---
  SC/FAF Game Load — 9 Phases

  Phase 0 — Engine binary

  - Runs init.lua (hardcoded path in binary)
  - Mounts .scd (FA original) + .nx2 (FAF) archives
  - path[] → VFS virtual mounts; hook = {'/schook'}

  Phase 1 — Lua env init (globalInit.lua)

  - Loads import, utils, class, trashbag, localization
  - ConvertCClassToLuaSimplifiedClass() → all moho.* C++ classes → Lua wrappers
  - Loads BuffBlueprints.lua

  Phase 2 — Blueprint pipeline (Blueprints.lua)

  1. Scan + exec /effects/*.bp, /env/*.bp, /meshes/*.bp, /projectiles/*.bp, /props/*.bp, /units/*.bp
  2. Load map-specific .bp, then mod .bp files
  3. ExtractAllMeshBlueprints() — generate mesh BPs from Display.Mesh
  4. PreModBlueprints() → ModBlueprints() → PostModBlueprints()
  5. RegisterAllBlueprints() — engine creates sim-side + UI-side copies

  Phase 3 — Lobby UI

  - EngineStartFrontEndUI() → uimain.lua → main.lua
  - CLobby passed to Lua as moho.lobby_methods
  - Player sets: map, factions, AI personalities, spawn spots, restrictions

  Phase 4 — Launch (lobby.lua:LaunchGame())

  1. MapUtil.LoadScenario() → reads _scenario.lua → paths to .scmap, _save.lua, _script.lua
  2. Broadcast launch to all peers
  3. moho.lobby_methods.LaunchGame(info) — ENGINE CALL → state: Loading

  Phase 5 — Engine pre-sim

  - Loads .scmap binary (terrain/textures/decals) before any Lua
  - Sets ScenarioInfo from GameOptions + PlayerOptions[]

  Phase 6 — SetupSession() (simInit.lua:83)

  1. TriggerManager init
  2. Detect GameHasAIs (non-skirmish type or requiresAiFunctionality)
  3. Init global tables: ArmyBrains, PlatoonHandles, BuilderTable, etc.
  4. ScenarioInfo.Env = scenarioenvironment.lua
  5. Parse restrictions → AddRestriction()
  6. doscript(_save.lua) → Scenario.Armies[name].Units, markers, chains
  7. Handle TeamSpawn shuffle
  8. doscript(_script.lua) → defines OnPopulate(), OnStart(), map logic

  Phase 7 — OnCreateArmyBrain() × N armies

  Called by engine, no units yet:
  1. Non-human: swap brain metatable → AI class from aibrains/index.lua
  2. InitializeStartLocation() — reads ARMY_N marker pos
  3. SetPlans() — load AI builder strategies
  4. Apply BuildRestrictions

  Phase 8 — BeginSession() (simInit.lua:303)

  1. BeginSessionAI() — load platoon templates, AI builders, base templates
  2. OnPopulate(ScenarioInfo) — spawn units, set scenario state
  3. OnStart(ScenarioInfo) — start map logic/triggers
  4. BeginSessionEffects() — Effect-marker entities
  5. BeginSessionTeams() — SetAlliance(), CommonArmy/Union setup
  6. CreateProps(), CreateResources()
  7. score.init(), recall.init(), victoryCondition:StartMonitoring()
  8. Sync table → UI (restrictions, start positions, playable area)
  9. brain:OnBeginSession() × N
  10. Fork GameOverListenerThread → state: Game Started (5)

  Phase 9 — Sim ticks begin

  - Frame 0, Tick 0
  - CSimDriver drives tick loop
  - Render thread interpolates sim snapshots — never mutates sim state

  ---
  Session state machine: None(0) → Loading(1) → SIM Init(3) → SIM Started(4) → Game Started(5)
