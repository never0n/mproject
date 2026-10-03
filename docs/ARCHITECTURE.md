# Mortown — Architecture Map

A navigation guide to the codebase. Code is the source of truth; this file explains *where things live and why*.
For Bevy/Rust terms see `docs/GLOSSARY.md`; for step-by-step flows see `docs/FLOWS.md`.

---

## 1. Crates

Cargo workspace (`Cargo.toml`), Rust edition 2024, Bevy pinned to `=0.18.1`.

| Crate (folder) | Package name | Role | Depends on |
|---|---|---|---|
| `crates/world` | `mortown-world` | Deterministic game world: city layout, terrain height, walls, buildings, collision, A* pathfinding, NPC home positions. No Bevy, no I/O. Built identically on client and server. | — |
| `crates/protocol` | `mortown-protocol` | Shared "language" between client and server: `ClientMessage` / `ServerMessage` (JSON via serde), plus game rules both sides need (items, skills, monsters, progression, chat command parser, inventory layout). | serde, serde_json |
| `crates/server` | `mortown-server` | Authoritative game server: Axum WebSocket at `/world`, PostgreSQL (sqlx) for accounts/characters/mail/alliances, 20 Hz simulation on `bevy_ecs`, combat AI, economy, chat, admin console. | world, protocol |
| `crates/client` | `mortown-client` | Desktop Bevy app: rendering, UI windows, input, camera, animation. Talks to the server over WebSocket from a background thread. | world, protocol |

Key rule: **the server is authoritative**. The client sends *intentions* (move toward point, attack, buy item); the server decides results and sends state back.

---

## 2. Folder tree

```
new_desctop_MMO/
├── Cargo.toml              workspace members + dev build profile
├── start.sh                one-click launcher: Postgres (docker) → build → server → client
├── compose.yaml            local PostgreSQL 17 on 127.0.0.1:55432
├── assets/                 runtime assets loaded by the client (see §6)
├── docs/                   project docs (this file, NETWORK.md history, CURSOR_HANDOFF.md, ...)
├── tools/                  Python/Blender scripts that generate assets (not part of the build)
├── MasterPrompt.txt        original design spec of the older browser game (ideas only)
└── crates/
    ├── world/src/
    │   ├── lib.rs          World struct + World::new() city generator; height(), surface_height(),
    │   │                   walkable(), segment_clear(), sight_clear(), hall_at(); constants (SPAWN, CITY_SCALE…)
    │   ├── terrain.rs      terrain tiling (TILE_SIZE, RENDER_HALF), building plots that flatten the ground
    │   ├── navigation.rs   find_path(): A* on a 2 m grid used for click-to-move
    │   └── npcs.rs         RESIDENTS table (the 7 starting NPCs), Role, TALK_RANGE, NPC position helpers
    ├── protocol/
    │   ├── data/           character_rules.json (starting stats), legacy fixtures for tests
    │   └── src/
    │       ├── lib.rs      versions, ClientMessage, ServerMessage, Character, Actor, Monster, starting_stats()
    │       ├── items.rs    item catalog, prices, merchant stock, equip rules
    │       ├── layout.rs   inventory/hotbar/equipment slot placement ("Place") and validation
    │       ├── transfer.rs moving item instances between owners while keeping identity
    │       ├── skills.rs   skills & abilities (ids 1–7), cooldowns, status effects, ground areas
    │       ├── monsters.rs monster kinds and their definitions
    │       ├── progression.rs, attributes.rs  levels, XP, stat growth, attribute points
    │       ├── weapons.rs, actions.rs  weapon families, action/animation timings
    │       ├── crafting.rs, professions.rs, quests.rs  crafting, professions, quests
    │       ├── chat.rs, chat/commands.rs  chat channels and slash-command parser (parse(), help())
    │       ├── mail.rs, exchange.rs, party.rs, alliance.rs, trade.rs  subsystem request/reply types
    │       └── admin.rs    admin target (Player/Npc/Mob), NpcState, admin powers
    ├── server/
    │   ├── migrations/     SQL migrations 0001…0014, applied automatically at server start
    │   ├── tests/network.rs  ignored integration tests (need TEST_DATABASE_URL)
    │   └── src/
    │       ├── main.rs     reads env vars, binds TCP, graceful Ctrl+C shutdown
    │       ├── lib.rs      Server, routes (/health, /world), simulation tick loop, session(),
    │       │               character_lobby(), play_character() — the per-player message loop
    │       ├── simulation.rs  ECS world: players (Adventurer), movement advance(), join/leave, attack, skills
    │       ├── combat.rs (+ combat/abilities.rs, effects.rs, rewards.rs)  mobs, AI, damage, death, XP/loot
    │       ├── store.rs    DB: accounts (Argon2), load/save character, transactional economy exchange_at()
    │       ├── economy.rs  buy/sell/craft/quest/learn rules applied inside store transactions
    │       ├── mail.rs, exchange.rs, party.rs, alliance.rs, alliance_storage.rs  social systems
    │       ├── chat.rs     chat sessions, rate limits, ignore lists
    │       └── console.rs (+ console/admin.rs, console/admin/control.rs)  slash-command execution, roles, admin panel
    └── client/src/
        ├── main.rs         App setup, ALL system registration and ordering; camera, click-to-move, player movement
        ├── network.rs      Network resource (connection + latest server state), background socket thread, receive()
        ├── lobby.rs        login / register / character list / character creation screens
        ├── ui.rs           HUD (name, level, bars), character window, shared colors, WorldInputBlocker
        ├── skin.rs         window theme and UI scale; movable.rs  drag windows around
        ├── menu.rs         gear menu (bottom-right), help; shutdown.rs  clean exit with save
        ├── chat.rs         chat box: typing, history, channels, sending; swallows game hotkeys while typing
        │   ├── chat/windows.rs  chat dialogs (help, ignore list, settings, confirmations)
        │   └── chat/admin.rs    native admin panel UI
        ├── actionbar.rs    F1–F12 bar, icon registry (Icons), tooltips, drag & drop, label() helper
        ├── inventory.rs    bag grid, equipment, 3D character preview camera
        ├── skills.rs       skills window (learn/upgrade)
        ├── combat.rs       CombatUi (current target), combat hotkeys (Tab, Space, 1, 2, G, R), floating text
        ├── target.rs       target frame at the top of the screen
        ├── npcs.rs         NPC services, spawning NPC models, nameplates; dialogue.rs  NPC dialogue window
        ├── trade.rs, crafting.rs, journal.rs, mail.rs, exchange.rs, party.rs, alliance.rs, warehouse.rs
        │                   one UI window per subsystem (setup / input / display)
        ├── scene.rs        builds the 3D city from mortown_world (meshes, lights, roofs visibility)
        │   ├── scene/batching.rs   merges many blocks into few meshes for performance
        │   ├── scene/materials.rs  PBR material palette, geometry cache
        │   └── scene/props.rs      places reference-city GLB props (fountain, stalls, forge…)
        ├── character.rs    hero glTF model + animation graph, AnimatedActor, animate()
        ├── ancestry.rs     race/gender body proportions (bone scaling), female texture
        ├── appearance.rs (+ appearance/arsenal.rs)  visible equipment and procedural weapon meshes
        ├── weapon_pose.rs  hand/weapon grip correction; player_motion.rs (+ poses.rs) attack/skill poses
        ├── action_fx.rs    projectiles, hand glows; skill_effects.rs  ground area effects
        ├── enemies.rs (+ enemies/model.rs, motion.rs)  monster models, glow, attack/death animation
        ├── residents.rs (+ residents/model.rs, motion.rs)  NPC clothing, work tools, work animations
        └── preview.rs      offline visual preview/gallery mode (--view / --ui-preview flags)
```

---

## 3. Client entry point and startup order

Entry: `crates/client/src/main.rs::main()`. There are **no custom Plugins**: every system is registered directly in one big `App::new()` builder.

### 3.1 Before systems run
`main()` parses CLI flags into `RunOptions` (`--view`, `--capture`, `--ui-preview`, `--exit-after`, `--size`), then inserts global resources:
- `Terrain(Arc<World>)` — the shared world geometry (built once by `World::new()`).
- `Orbit` — camera angles/distance; `Movement` — click-to-move path state.
- `network::Network` — connection and the latest server state; `lobby::Form` — login form state.
- `npcs::ServiceRequest` message type (dialogue buttons → trade/craft/mail/alliance windows).
- `DefaultPlugins` with the asset folder from `asset_directory()` and a 1440×900 window.

### 3.2 Startup (runs once)
Two `.chain()`ed groups (each system runs strictly after the previous one):

1. `scene::setup → ui::setup → lobby::setup → actionbar::setup → inventory::setup → crafting::setup → combat::setup → (trade, npcs) → skills → mail → menu → skill_effects → journal → target → dialogue → exchange → (chat, chat::windows) → party → (alliance, warehouse) → movable::setup`
2. After `movable::setup`: `preview → skin → appearance → ancestry → enemies → action_fx → residents`.

**Why order matters:** later setups read resources created by earlier ones. Example: `actionbar::setup` creates the `Icons` resource (icons + font) that `journal::setup`, `target::setup`, `dialogue::setup` etc. require; `scene::setup` spawns the `Player` and camera that many systems expect to exist. In a chain, Bevy applies `Commands` between steps, so a resource inserted by one setup is visible to the next. `movable::setup` must run after all windows exist so it can make them draggable; `skin::setup` restyles windows after they are built.

### 3.3 Update (every frame)
One giant `.chain()` — the groups below run **in this exact order** each frame:

| # | Systems | Purpose |
|---|---|---|
| 1 | `lobby::buttons, typing, submit` | Lobby input may start a connection. |
| 2 | `network::receive` | Drain server messages into `Network`, move hero/remote players toward server positions. Everything after this sees fresh state. |
| 3 | `lobby::display/layout/style/turn_model` | Redraw lobby from new state. |
| 4 | `preview::*`, `chat::admin::input → chat::windows::input → chat::input`, `alliance/warehouse::input`, `skin::*` | Text input first, so typing in chat **swallows** keys before game hotkeys see them. |
| 5 | `movable::input, target::input, party/menu/dialogue/exchange/journal/mail/inventory/trade/skills/crafting::input` | Window clicks and hotkeys (I, J, K, M, L, Esc…). |
| 6 | `ui::character_window, spend_attribute, character_cards` | Character sheet (C). |
| 7 | `*::display` for every window | Refresh window contents from `Network`. |
| 8 | `combat::input/display`, `npcs::sync → enemies::sync`, `enemies::glow`, `skill_effects::display`, `actionbar::input/display`, `warehouse::hints` | Combat hotkeys, spawn/despawn NPC & monster models, hotbar. |
| 9 | `camera_input` | Mouse orbit / zoom. |
| 10 | `click_destination → target::display → dialogue::display` | Left click: select actor or pick ground point. |
| 11 | `move_player` | WASD / path following (offline local, online sends `Move`). |
| 12 | `character::initialize … action_fx::spawn` (chain) | Prepare skeletons, equipment, body shape, poses for all models. |
| 13 | `character::animate` | Choose idle/walk/run animation. |
| 14 | `update_camera` | Place camera **after** the player moved, so it never lags a frame. |
| 15 | `scene::update_visibility → npcs::labels`, `ui::update`, `ui::button_feedback` | Hide roofs when inside, nameplates, HUD bars. |
| 16 | `shutdown::update → capture_and_exit` | Clean exit, screenshots. |

### 3.4 PostUpdate (bone overrides)
These systems overwrite skeleton bone `Transform`s **after** Bevy's animation player (`AnimationSystems`) and **before** `TransformSystems::Propagate` computes final world transforms for rendering:

```
AnimationSystems → ancestry::pose → { enemies::reset_pose, player_motion::reset } → weapon_pose::pose
   → player_motion::pose → action_fx::animate
   weapon_pose::pose → enemies::pose, residents::pose
   ancestry::pose → ancestry::frame_preview
→ TransformSystems::Propagate
```
If one ran before the animation player, the animation would overwrite it; if after Propagate, the change would show one frame late or not at all. Do not reorder these without a reason.

### 3.5 Server startup (for reference)
`crates/server/src/main.rs::main()` → `Server::new()` (connect DB, run migrations, create `Simulation`, spawn the 50 ms tick task that calls `Simulation::update()` and publishes a `Snapshot`) → `axum::serve(router)`. On Ctrl+C it stops accepting, waits until all players are saved, closes the DB pool.

---

## 4. Client ↔ server data flow

### 4.1 Transport
- WebSocket, JSON text frames. Address: env `MORTOWN_SERVER`, default `ws://127.0.0.1:3000/world`.
- Client side (`network.rs::Network::begin`): spawns an OS thread with a small Tokio runtime that owns the socket.
  - Outgoing: Bevy systems call `Network` helpers (`request`, `action`, `intent`, `chat_request`, …) → Tokio channel (capacity 8) → socket.
  - Incoming: socket → `std::sync::mpsc` channel (capacity 64) → `network::receive` drains it every frame. If that channel fills up, the client disconnects ("client can't keep up").
  - Liveness: client gives up after 15 s of silence; server pings every 5 s and drops peers after 20 s.
- Versions: `PROTOCOL_VERSION` and `WORLD_VERSION` (`protocol/src/lib.rs`) must match on both sides; mismatch → disconnect.

### 4.2 Session lifecycle
```
Login/Register{version,world,login,password}
  → server::session → Characters{list}          (Register → Registered, then reconnect)
CreateCharacter / SelectCharacter
  → server::character_lobby → Welcome{character, actor}
  → server::play_character loop:
       Snapshot every tick (20 Hz) + replies to commands
LeaveWorld → back to Characters;  Logout → Saved → socket closed
```
Client `network::Mode`: `Menu → Connecting → Selection → Online`. `Offline` exists only for the visual preview (`--view` flag, set in `lobby::setup`).

### 4.3 Main messages
**ClientMessage** (client → server): `Login`, `Register`, `CreateCharacter`, `SelectCharacter`, `Move{seq,target,walking}`, `Teleport` (admin), `Attack{target}`, `UseSkill`, `UseSecondary`, `UseAbility{skill,target}`, `Respawn`, `UsePotion`, `Trade`, `Craft`, `Specialize`, `Quest`, `LearnSkill/LearnSecondary/LearnAbility`, `SpendAttribute`, `Arrange` (move items between bag/hotbar/equipment), `BindHotbar`, `SetEquipment`, `Chat{command}`, `Party`, `Alliance`, `Mail`, `Exchange`, `LeaveWorld`, `Logout`.

**ServerMessage** (server → client): `Registered`, `Characters`, `Welcome`, `Snapshot{tick, actors, monsters, residents, areas}`, `EconomyUpdated{character, actor}` (after any inventory/stat transaction), `CharacterUpdated`, `Chat/Party/Alliance/Mail/Exchange{reply}`, `Error{message, fatal}`, `Saved`.

Types: `Character` = persistent sheet (stats map, inventory, loot). `Actor` = live in-world state of a player (position, yaw, hp/mana/stamina, cooldowns, effects, progress). `Monster` = live mob state.

### 4.4 Who is authoritative

| Area | Authority | How |
|---|---|---|
| Player position | **Server** | Client sends `Move` with a *target point* (≤ every 45 ms). Server `simulation::advance` moves at `RUN_SPEED`/`WALK_SPEED` per 50 ms tick with collision via `World::segment_clear`; input older than 10 ticks stops the player. Client smoothly pulls the hero toward the server position in `network::receive` (no client-side prediction). |
| Path planning (click-to-move) | Client | `find_path` runs on the client; the server only validates each step. |
| Target selection | Client | `combat::CombatUi` (`selected`, `player`, `npc`). The server re-checks range/target on every attack/skill. |
| Combat, damage, death, XP, loot | **Server** | `simulation.rs` + `combat.rs`; results arrive in `Snapshot` (hp, effects, actions) and `EconomyUpdated`. |
| Inventory, gold, crafting, trade, quests, skills learning | **Server + DB transaction** | `play_character` → `store::exchange_at` → `economy.rs`. Client sends `expected` revisions so stale requests are rejected. |
| Chat | **Server** | Validates, rate-limits, persists, then broadcasts to recipients (`server/chat.rs`, `console.rs` for slash commands). |
| NPC dialogue | Client UI | Purely local window; its buttons send real requests (`Trade`, `Quest`) that the server checks (NPC exists, distance). |
| Mail, exchange, party, alliance | **Server** | Dedicated request/reply messages, DB transactions with escrow. |
| Admin powers | **Server** | Role checked on every action (`console::check_access`). |

---

## 5. World, terrain and navigation

- Lives in `crates/world`. `World::new()` (`world/src/lib.rs`) deterministically generates the whole map: natural height (`height()`), city wall, gates, 4 civic halls (`HALL_SPECS`), streets, building blocks, ramps/stairs, bridges, trees, scenery. Units are **meters**; Y is up; the map spans `±WORLD_HALF` (8000 m), the city is scaled by `CITY_SCALE` (0.66).
- Main queries: `surface_height(x,z)` (ground or floor/stair height), `walkable(x,z)`, `segment_clear(a,b)` (can a player step from a to b — used for all collision), `sight_clear(a,b)` (line of sight), `hall_at(x,z)` (which hall you are inside).
- `navigation.rs::find_path`: A* over a 2 m grid (`CELL`), gives up after 400 000 expanded nodes.
- `npcs.rs`: static `RESIDENTS` with role, hall, counter and talk range (`TALK_RANGE` 6 m).

How the client uses it (through the `Terrain` resource):
- `scene::setup` turns `World` data into meshes and spawns the `Player` entity, its glTF model and the destination marker.
- `main.rs::click_destination` → `terrain_hit` ray-marches the mouse ray against `surface_height`; then `find_path` runs on a background task.
- `main.rs::move_player` (offline) and `network::receive` (online smoothing) use `segment_clear` + `surface_height`.
- `main.rs::update_camera` pulls the camera in front of walls (`camera_solid` blocks) and hills.
- `npcs.rs`, `residents.rs` place NPCs using `Npc::feet`, `same_room`, `available`.

The server builds the same `World` in `Server::new` → `Simulation`. Any geometry change affects both — bump `WORLD_VERSION` when collision changes.

---

## 6. Assets

- Folder: `assets/`. Chosen at startup by `main.rs::asset_directory()`: an `assets/` folder next to the executable if it contains `characters/adventurer.glb`, otherwise the repo's `assets/`.
- Loaded via Bevy `AssetServer` (`assets.load(...)`, asynchronous, returns a `Handle`):
  - `characters/adventurer.glb` — the single humanoid model. Scene 0 + animations in `character.rs`; reused for remote players (`network::receive`), NPCs (`npcs.rs`), enemies (`enemies.rs`), inventory preview. Different looks come from procedural clothing/weapon meshes and bone scaling.
  - `characters/adventurer-female-v1.png` — `ancestry.rs::setup`.
  - `icons/1.png … 40.png` — `actionbar::setup` builds the shared `Icons` resource; `actionbar::item_icon` maps item kind → icon.
  - `environment/reference_city_v1/glb/*.glb` — props listed in `scene/props.rs::MODELS`.
  - `fonts/DejaVuSerif.ttf` — headings in `skin.rs`.
- Compiled into the binary: `fonts/DejaVuSans.ttf` via `include_bytes!` (several `setup` functions); `protocol/data/*.json` via `include_str!`.
- Most of the city (buildings, walls, roads, terrain) is **procedural** — generated in `scene.rs` from `mortown_world`, not loaded from files.
- `tools/*.py` regenerate assets offline (Blender); `assets/icons/generate.py` makes icons.

---

## 7. Where do I change X?

| I want to change… | File → function / item | Notes |
|---|---|---|
| Player run/walk speed (online) | `protocol/src/lib.rs` → `RUN_SPEED`, `WALK_SPEED` (used by `server/src/simulation.rs::advance`) | Server-side, real speed. |
| Player speed (offline preview) | `client/src/main.rs::move_player` → literals `5.0` / `10.0` | Separate copy; keep in sync. |
| Camera start angle/distance | `client/src/main.rs::main` → `Orbit { yaw, pitch, distance }` | |
| Mouse sensitivity, zoom limits, F/Home keys | `client/src/main.rs::camera_input` | `0.004` sensitivity, zoom `4..2200`. |
| Camera height, smoothing, wall collision | `client/src/main.rs::update_camera`, `ray_box` | Look-at height `1.5`, smoothing `8.0`. |
| Click-to-move / ground picking | `client/src/main.rs::click_destination`, `terrain_hit`; `world/src/navigation.rs::find_path` | |
| Click-to-select radius / max range | `client/src/main.rs::click_destination` (`60.` m, score `< 24.`) | |
| Combat hotkeys (Tab, Space, 1, 2, G, R) | `client/src/combat.rs::input` | |
| A skill's numbers/effect | `protocol/src/skills.rs` (`definition`, `ability`, `starter`, `ranked`) + server `combat/abilities.rs`, `simulation.rs::use_ability` | Visuals: `action_fx.rs`, `skill_effects.rs`, `player_motion/poses.rs`. |
| Hotbar (F1–F12) behavior | `client/src/actionbar.rs::input`, `display` | |
| Items, prices, merchant stock | `protocol/src/items.rs` (`definition`, `MERCHANT_STOCK`, `buy_price`, `sell_price`) | Icon: `actionbar.rs::item_icon`. |
| Monster stats / kinds | `protocol/src/monsters.rs` (`Kind`, `Definition`); AI & spawns in `server/src/combat.rs` (`Mob`, `Combat`) | Rewards: `combat/rewards.rs`; look: `client/src/enemies.rs`. |
| NPC position, role, talk range | `world/src/npcs.rs` → `RESIDENTS`, `TALK_RANGE` | |
| NPC services (which buttons) | `client/src/npcs.rs` → `Service`, `offers`, `profession` | |
| NPC dialogue window text/buttons | `client/src/dialogue.rs` → `setup`, `display`, `input`, `open` | Quests: `protocol/src/quests.rs`, server `economy.rs::quest`. |
| A UI window (layout / clicks / refresh) | that window's module: `setup` (build), `input` (clicks/hotkeys), `display` (refresh from `Network`) | Colors: `ui.rs` constants `GOLD, INK, PANEL…`; theme: `skin.rs`. |
| Add a new window | new module + `mod` line + register `setup`/`input`/`display` in `main.rs::main` chains | Make it draggable via `movable.rs`. |
| HUD bars (HP/mana/stamina, name, level) | `client/src/ui.rs::setup`, `update` | |
| Chat box behavior | `client/src/chat.rs::input`, `display`, `Log` | |
| A chat / slash command | `protocol/src/chat/commands.rs` (`parse`, `help`) → `server/src/console.rs::execute` (+ `console/admin*.rs`) | Docs: `docs/CHAT_COMMANDS.md`, `ADMIN_PANEL.md`. |
| A network message | `protocol/src/lib.rs` (`ClientMessage`/`ServerMessage`, bump `PROTOCOL_VERSION`) → `server/src/lib.rs::play_character` (match arm) → `client/src/network.rs::receive` + a `Network` helper | All three crates must agree. |
| Server tick rate | `server/src/lib.rs::Server::new` (50 ms interval) **and** `protocol` `TICK_SECONDS` | Must match. |
| Server address (client) | env `MORTOWN_SERVER`, default in `network.rs::Network::default` | |
| Window size / title / sky color | `client/src/main.rs::main` (`--size`, `title`, `ClearColor`) | |
| City layout / collision | `world/src/lib.rs::World::new` | Bump `WORLD_VERSION`. |
| Character starting stats | `protocol/data/character_rules.json`, `protocol/src/lib.rs::starting_stats` | |
| Race body proportions | `client/src/ancestry.rs` | |
| Database schema | add a new file in `server/migrations/` | Never edit applied migrations. |

---

## 8. Known rough edges

*(Filled in at the end of Phase 4.)*
