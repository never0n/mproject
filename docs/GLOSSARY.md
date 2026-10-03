# Mortown — Glossary

Each term is explained once, with a real example from this codebase. Paths are relative to `crates/`.
Bevy version: **0.18.1** (APIs differ between Bevy versions — check this before copying examples from the internet).

---

## 1. Bevy concepts

Bevy uses **ECS** (Entity–Component–System): data lives in components attached to entities, logic lives in systems (plain functions) that Bevy calls every frame.

### Entity
A bare ID ("a thing in the world") with no data of its own. Components are attached to it.
*Example:* `client/src/scene.rs::setup` — `commands.spawn((Player, AnimatedActor::default(), Transform…, Visibility…))` creates the hero entity. `network::receive` spawns one entity per other player.

### Component
A struct attached to an entity. Often empty "marker" structs that only label an entity so systems can find it.
*Example:* `client/src/main.rs` — `#[derive(Component)] struct Player;` (marker), `network.rs::Remote { id, target, yaw }` (data for another player).

### Resource
A single global value, not attached to any entity (one per app). Systems access it with `Res<T>` (read) or `ResMut<T>` (write).
*Example:* `main.rs` — `Terrain(Arc<World>)`, `Orbit`, `Movement`; `network.rs::Network`. Inserted in `main()` with `.insert_resource(...)` / `.init_resource::<T>()` (uses `Default`).

### System
An ordinary function whose parameters tell Bevy what data it needs. Bevy calls it according to the schedule.
*Example:* `main.rs::camera_input(mouse: Res<AccumulatedMouseMotion>, ..., mut orbit: ResMut<Orbit>, ...)` — reads input, writes camera angles.

### Query
A system parameter that iterates over all entities having certain components.
*Example:* `network.rs::receive` — `mut remotes: Query<(Entity, &mut Remote, &mut Transform), Without<Player>>` then `for (entity, mut remote, mut transform) in &mut remotes { … }`.

### Single<>
A query that expects **exactly one** matching entity and gives it directly. If there are zero or several, the system is skipped for that frame.
*Example:* `main.rs::move_player` — `mut player: Single<&mut Transform, With<Player>>`; `click_destination` — `window: Single<&Window, With<PrimaryWindow>>`.

### With / Without filters
Restrict a query to entities that have / don't have a component, without reading it. `Without` is also how you prove to Bevy that two mutable queries never touch the same entity (otherwise it panics at runtime with error B0001).
*Example:* `main.rs::update_camera` — camera query is `(With<Camera3d>, Without<Player>, Without<inventory::PreviewCamera>, Without<inventory::PortraitCamera>)` so it finds only the main camera and doesn't conflict with the `Player` transform query.

### Commands
A queue of world changes (spawn, despawn, insert resource) applied *later*, not immediately — because systems may run in parallel. In a `.chain()`, Bevy applies them between steps.
*Example:* `network.rs::receive` — `commands.entity(entity).despawn()` for players who left; `actionbar.rs::setup` — `commands.insert_resource(icons)`.

### Message vs Event
- **Message** (Bevy 0.17+ name for the old "buffered events"): a value written by one system with `MessageWriter<T>` and read by others with `MessageReader<T>` during the next frame(s). Must be registered with `.add_message::<T>()`.
  *Example:* `npcs.rs::ServiceRequest` (`#[derive(Message)]`), registered in `main()`; written by `dialogue.rs::input`, read by `trade::input`, `crafting::input`, `mail::input`, `alliance::input`. Also built-in: `MessageReader<KeyboardInput>` in `chat.rs::input`, `MessageWriter<AppExit>` in `main.rs::capture_and_exit`.
- **Event** (in 0.17+): triggered and handled *immediately* by an **observer** function attached with `.observe(...)`; the handler receives `On<E>`.
  *Example:* `inventory.rs::setup` — `.observe(|ready: On<SceneInstanceReady>, …| …)` runs once the preview model finishes loading; `main.rs::capture_and_exit` — `.observe(save_to_disk(path))` on a screenshot.
- ⚠ Name clash: `network.rs` has its own private `enum Event { Message(ServerMessage), Closed(String) }`. It's just a Rust enum for the socket channel, **not** a Bevy Event.

### Plugin
A bundle of systems/resources added with `.add_plugins(...)`. This project uses only Bevy's `DefaultPlugins` (window, rendering, assets, input, UI, animation…) configured with `.set(AssetPlugin{…})` and `.set(WindowPlugin{…})` in `main()`. There are **no custom plugins**: all game systems are registered directly in `main()`.

### SystemSet
A named group of systems that can be ordered as a unit. The project defines none, but orders against Bevy's built-in sets.
*Example:* `main.rs::main` — `ancestry::pose.after(bevy::app::AnimationSystems).before(bevy::transform::TransformSystems::Propagate)`.

### Schedules: Startup / Update / PostUpdate
- `Startup` — runs once at launch (build scene, UI windows). *Example:* `scene::setup`, `ui::setup`, … in `main()`.
- `Update` — every frame: input, network, game logic, UI refresh. *Example:* `network::receive`, `move_player`, `update_camera`.
- `PostUpdate` — every frame after `Update`; Bevy runs animation and transform propagation here. *Example:* `weapon_pose::pose`, `player_motion::pose` (bone overrides).
The server also uses a `bevy_ecs` `Schedule` without the full app: `server/src/simulation.rs::Simulation::update` calls `self.schedule.run(&mut self.world)`, which runs `advance`.

### .chain()
Runs a tuple of systems strictly one after another, in the written order (and applies `Commands` between them). Without it, Bevy may run systems in any order or in parallel.
*Example:* `main.rs::main` — the whole `Update` tuple ends with `.chain()`, so `network::receive` always runs before `move_player`, which runs before `update_camera`.

### .after() / .before()
Order one system relative to another system or set, without chaining everything.
*Example:* `main.rs::main` — `enemies::reset_pose.after(ancestry::pose).before(weapon_pose::pose)`; the second `Startup` group uses `.after(movable::setup)`.

### Run conditions
`.run_if(condition)` skips a system when a condition is false. **Not used in this project.** Instead systems start with a manual guard that returns early.
*Example:* `main.rs::camera_input` — `if !network.playing() { return; }`; `combat.rs::input` — `if network.mode != Mode::Online { … return; }`.

### Change detection
Bevy tracks when a component was added or modified. Filters `Changed<T>` / `Added<T>` keep only those entities in a query.
*Example:* almost every window's `input` uses `Query<(&Action, &Interaction), Changed<Interaction>>` (e.g. `combat.rs::input`) so a button reacts once when pressed, not every frame. `character.rs::initialize` uses `Added<AnimationPlayer>` to set up animations only for newly loaded models.

### Assets and Handles
`AssetServer::load("path")` starts loading in the background and returns a `Handle<T>` (a cheap reference). The real data appears later.
*Example:* `actionbar.rs::setup` — `assets.load(format!("icons/{n}.png"))`; `network.rs::receive` — `GltfAssetLabel::Scene(0).from_asset("characters/adventurer.glb")`.

---

## 2. Rust as used here

### Option / Result
`Option<T>` = a value or nothing (`Some(x)` / `None`). `Result<T, E>` = success or error (`Ok(x)` / `Err(e)`).
*Example:* `network.rs::Network` — `authoritative: Option<Actor>` (no actor until you enter the world). `world/src/navigation.rs::find_path` returns `Result<Vec<[f32; 2]>, PathError>`.

### The `?` operator
"If this is an error / `None`, return it from the current function right now; otherwise unwrap the value."
*Example:* `server/src/simulation.rs::Simulation::input` — `let entity = *self.entities.get(&id).ok_or("Персонаж не в мире")?;`.

### let-else
Unwrap a value or leave the function in the `else` branch.
*Example:* `main.rs::click_destination` — `let Some(cursor) = window.cursor_position() else { return; };`.

### if-let chains
Several `let` matches and conditions joined with `&&` in one `if` (Rust 2024).
*Example:* `main.rs::move_player` — `else if let Some(task) = movement.pending.as_mut() && let Some(result) = block_on(future::poll_once(task)) { … }`.

### match
Branch on the shape of a value; must cover all cases (`_` = everything else).
*Example:* `network.rs::receive` — `match event { Event::Message(ServerMessage::Snapshot { … }) if network.mode == Mode::Online => { … }, … , _ => {} }`. The `if …` after a pattern is a *guard*.

### Arc
"Atomically reference-counted" shared pointer: many owners of the same read-only data, safe across threads. Cloning an `Arc` copies a pointer, not the data.
*Example:* `main.rs` — `Terrain(Arc<World>)`; in `click_destination` `let world = world.0.clone();` gives the background pathfinding task its own handle to the same huge `World`.

### clone vs borrow
Borrow (`&x`, `&mut x`) = temporary access without copying. `clone()` = make an independent copy (cheap for `Arc`, expensive for big structs).
*Example:* `network.rs::receive` — `network.authoritative.clone()` copies the actor so `network` can be mutated afterwards; `dialogue.rs::open(npc, &mut panel, &mut network, …)` borrows instead.

### Closures
Anonymous inline functions `|args| body` that can capture surrounding variables.
*Example:* `main.rs::main` — `let value = |flag: &str| args.windows(2).find(|a| a[0] == flag).map(|a| a[1].clone());` reads a CLI flag value.

### Iterators (map / filter / min_by / collect)
Chains that process sequences lazily.
*Example:* `main.rs::click_destination` — `candidates.into_iter().filter(|(d, ..)| *d < 24.).min_by(|a, b| a.0.total_cmp(&b.0))` picks the closest clickable actor. `network.rs::receive` — `network.roster.iter().map(|p| p.id).collect::<HashSet<_>>()`.

### Traits and impl
A trait is a set of methods a type promises to provide (like an interface).
*Example:* `network.rs` — `impl Default for Network { fn default() -> Self { … } }` (used by `.init_resource::<Network>()`); `protocol/src/lib.rs` — `impl From<&Character> for BodyAppearance`.

### derive
Auto-generate standard trait implementations.
*Example:* `protocol/src/lib.rs` — `#[derive(Debug, Clone, Serialize, Deserialize)] pub struct Actor` (printable, copyable, JSON-convertible). `ClientMessage` intentionally has **no** `Debug`, so passwords can't be printed by accident.

### cfg(test)
Code compiled only when running `cargo test`.
*Example:* `main.rs` — `#[cfg(test)] mod tests { … }` with movement tests like `keyboard_cancels_click_route_and_moves_relative_to_camera`.

### Visibility: pub, pub(crate), pub(super)
`pub` = visible everywhere; `pub(crate)` = only inside this crate; `pub(super)` = only the parent module. No keyword = private to the file/module.
*Example:* `client/src/scene/batching.rs` — `pub(super) fn spawn_blocks` is callable only from `scene.rs`. `appearance.rs` — `pub(crate) fn weapon_socket()`.

### async / Tokio (networking only)
`async fn` returns a task that runs on an executor (Tokio). Used by the server and the client's socket thread, not by Bevy systems.
*Example:* `client/src/network.rs::Network::begin` — `std::thread::spawn` + `tokio::runtime::Builder::new_current_thread()` + `tokio::select!` waiting for socket frames, outgoing commands and a heartbeat.

---

## 3. Math seen in the code

Coordinates: **meters**, **Y is up**, the ground plane is X/Z. Many world functions use 2D `[x, z]` arrays.

### Vectors (Vec2 / Vec3)
A direction or position. Length = distance; `normalize()` makes length 1 (pure direction); `dot` measures how aligned two vectors are.
*Example:* `main.rs::move_player` — `let right = Vec2::new(yaw.cos(), -yaw.sin()); let forward = Vec2::new(-yaw.sin(), -yaw.cos()); (right * input.x + forward * input.y).normalize()` turns WASD into a camera-relative direction.

### Angles: yaw, pitch, atan2
Yaw = turn around the vertical axis; pitch = tilt up/down (radians). `atan2(dx, dz)` gives the yaw that faces along a movement.
*Example:* `main.rs::move_player` — `Quat::from_rotation_y((next.x - pos.x).atan2(next.y - pos.y))`; `update_camera` builds the camera direction from yaw/pitch with `sin`/`cos` (spherical coordinates).

### Quaternions and rotation
`Quat` stores a 3D rotation without gimbal problems. `Quat * Vec3` rotates a vector; `slerp` blends two rotations smoothly.
*Example:* `network.rs::receive` — `hero.rotation.slerp(Quat::from_rotation_y(actor.yaw), alpha)`; `main.rs::update_camera` — `let inverse = Quat::from_rotation_y(-block.yaw);` rotates the camera ray into a building's local space so the box test can be axis-aligned.

### lerp and exponential smoothing
`a.lerp(b, t)` = point `t` of the way from `a` to `b`. Using `t = 1 - exp(-dt * k)` makes the smoothing frame-rate independent: larger `k` = snappier.
*Example:* `network.rs::receive` — `alpha = 1. - (-time.delta_secs() * 25.).exp(); hero.translation.lerp(target, alpha)`; `main.rs::update_camera` — zoom smoothing with `k = 8.0`.

### Delta time
`time.delta_secs()` = seconds since last frame. Multiply speeds by it so movement doesn't depend on FPS.
*Example:* `main.rs::move_player` — `let step = speed * time.delta_secs().min(0.1);` (capped so a lag spike can't teleport you through a wall).

### Ray vs box intersection (slab method)
For each axis, compute where the ray enters/exits the box's two parallel planes; the ray hits if the latest entry is before the earliest exit.
*Example:* `main.rs::ray_box` — used by `update_camera` to pull the camera in front of walls.

### Ray marching + bisection
Step along a ray until you go below the ground, then halve the interval repeatedly to find the crossing precisely.
*Example:* `main.rs::terrain_hit` — growing steps (`distance * 0.008`, clamped 0.4–8 m), then 14 bisection iterations.

### Screen ↔ world projection
`world_to_viewport` = 3D point → pixel on screen. `viewport_to_world` = pixel → 3D ray from the camera.
*Example:* `main.rs::click_destination` — projects a monster's feet and head to the screen, measures cursor distance to that segment (point-to-segment projection with `dot`), and uses `viewport_to_world` for ground clicks.

### Clamping
`x.clamp(min, max)` keeps a value in range.
*Example:* `main.rs::camera_input` — `pitch.clamp(0.06, 1.53)` (just above horizontal to almost straight down), zoom `clamp(4.0, 2200.0)`.

---

## 4. Project terms and magic numbers

### Terms
| Term | Meaning | Where |
|---|---|---|
| Character | Persistent hero sheet from DB: stats map, inventory, loot. | `protocol/src/lib.rs::Character` |
| Actor | Live in-world state of a player: position, yaw, hp/mana/stamina, cooldowns, effects, progress. | `protocol/src/lib.rs::Actor` |
| authoritative | The client's copy of *its own* Actor as last reported by the server. | `network.rs::Network.authoritative` |
| roster | All players' Actors from the latest snapshot. | `Network.roster` |
| residents | Town NPCs (state from server). | `Network.residents`, `world/src/npcs.rs` |
| Monster / Mob | Enemy; `Monster` = what the client sees, `Mob` = server-side full state. | `protocol::Monster`, `server/src/combat.rs::Mob` |
| Snapshot | Full world state sent every server tick. | `ServerMessage::Snapshot` |
| intent | Client's wish ("move toward point"), never a final position. | `Network::intent` |
| seq | Increasing number on each `Move`; the server rejects older ones. | `Simulation::input` |
| Mode | Client connection state: Menu, Connecting, Selection, Online, Offline (preview). | `network.rs::Mode` |
| Hall | Enterable civic building; camera ignores its own roof when you're inside. | `world/src/lib.rs::Hall`, `hall_at` |
| Block | A box of city geometry; `camera_solid` = blocks the camera. | `world/src/lib.rs::Block` |
| expected / revision | Number the client sends so the server rejects actions based on stale inventory. | `ClientMessage::Craft/Arrange/SpendAttribute` |
| admin_revision | Counter bumped by admin item/skill edits so an old save can't overwrite them. | `store.rs`, `docs/CURSOR_HANDOFF.md` |
| loot zeros | Explicit `0` counts in loot so deleted starter items don't reappear. | `protocol/src/items.rs` |
| binding | `u64` id stored in a hotbar slot (skill id or item instance token). | `actionbar.rs::binding`, `protocol/src/layout.rs` |

### Constants
| Constant | Value | Meaning | Where |
|---|---|---|---|
| `PROTOCOL_VERSION` | 42 | Message format version; must match client↔server. | `protocol/src/lib.rs` |
| `WORLD_VERSION` | "mortown-10" | World geometry version; must match. | `protocol/src/lib.rs` |
| `TICK_SECONDS` | 0.05 | Server step (20 Hz). Also hard-coded as 50 ms in `server/src/lib.rs::Server::new`. | `protocol/src/lib.rs` |
| `RUN_SPEED` / `WALK_SPEED` | 10 / 5 m/s | Server movement speed. | `protocol/src/lib.rs` |
| `CHARACTER_LIMIT` | 12 | Heroes per account. | `protocol/src/lib.rs` |
| `WORLD_HALF` | 8000 m | Map extends ±8 km from origin. | `world/src/lib.rs` |
| `CITY_SCALE` | 0.66 | Scales city coordinates from the original design. | `world/src/lib.rs` |
| `PLAYER_RADIUS` | 0.45 m | Collision radius of a character. | `world/src/lib.rs` |
| `TALK_RANGE` | 6 m | Max distance to talk/trade with an NPC. | `world/src/npcs.rs` |
| `CELL` / `LIMIT` | 2 m / 400 000 | Pathfinding grid size / max explored cells. | `world/src/navigation.rs` |
| `TILE_SIZE`, `RENDER_HALF` | 512 / 9216 m | Terrain mesh tile size / rendered area. | `world/src/terrain.rs` |
| `CORPSE_SECONDS` | 5 s | How long a dead monster stays visible. | `client/src/enemies/motion.rs` |

### Magic numbers in client code
| Value | Meaning | Where |
|---|---|---|
| `0.004` | Mouse sensitivity (radians per pixel). | `main.rs::camera_input` |
| `0.06 .. 1.53` | Camera pitch limits (radians). | `main.rs::camera_input` |
| `4.0 .. 2200.0`, `0.16` | Zoom limits (m) and scroll zoom factor. | `main.rs::camera_input` |
| `18.0`, `0.24`, `0.25` | Default camera distance, pitch, yaw. | `main.rs::main`, `camera_input` (Home key) |
| `1200.0`, `0.88` | "F" overview camera distance and pitch. | `main.rs::camera_input` |
| `1.5` | Camera looks at a point 1.5 m above the feet. | `main.rs::update_camera` |
| `0.25` | Padding around walls for camera collision. | `main.rs::update_camera` |
| `60.` | Max distance to click-select an actor. | `main.rs::click_destination` |
| `0.20`, `8..36`, `23.`, `< 24.` | Click hit radius (20% of on-screen height, 8–36 px) and normalized score threshold. | `main.rs::click_destination` |
| `2.` | WASD "carrot" target 2 m ahead (online). | `main.rs::move_player` |
| `0.15` | Waypoint counts as reached within 15 cm. | `main.rs::move_player` |
| `0.045` | Min seconds between `Move` messages (~22/s). | `network.rs::Network::intent` |
| `25.` / `20.` | Smoothing rate for own hero / other players. | `network.rs::receive` |
| `15 s` / `20 s` | Client / server silence timeout. | `network.rs::Network::begin`, `server/src/lib.rs::PEER_TIMEOUT` |
| `64` / `8` | Incoming / outgoing channel capacity. | `network.rs::Network::begin` |
| `1.5` / `1.9` | Clickable body height for race 5 (dwarf) vs others. | `main.rs::click_destination` |
