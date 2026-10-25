//! Mortown desktop client entry point.
//! Builds the single Bevy `App`: global resources, Startup/Update/PostUpdate system order
//! (no custom plugins), plus the core controls: camera orbit, click-to-move/select and player movement.
//! Server state arrives via `network::receive`; world geometry comes from `mortown_world` (`Terrain`).
#![allow(clippy::type_complexity, clippy::too_many_arguments)]

mod action_fx;
mod actionbar;
mod alliance;
mod ancestry;
mod appearance;
mod character;
mod chat;
mod combat;
mod crafting;
mod enemies;
mod exchange;
mod foundry;
mod inventory;
mod lobby;
mod menu;
mod movable;
mod network;
mod objects;
mod party;
mod player_motion;
mod preview;
mod scene;
mod shutdown;
mod skill_effects;
mod skills;
mod skin;
mod stations;
mod target;
mod ui;
mod weapon_pose;

use bevy::{
    app::AppExit,
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
    tasks::{AsyncComputeTaskPool, Task, block_on, futures_lite::future},
    window::PrimaryWindow,
};
#[cfg(test)]
use mortown_world::height;
use mortown_world::{World, navigation::find_path};
use std::{collections::VecDeque, sync::Arc};

/// Shared world geometry, identical to the server's. `Arc` lets background pathfinding tasks
/// hold the same data without copying it.
#[derive(Resource, Clone)]
struct Terrain(Arc<World>);

/// Marker for the local hero entity (spawned in `scene::setup`).
#[derive(Component)]
struct Player;

/// Third-person camera state in radians/meters. `distance` is the zoom target,
/// `current_distance` is the smoothed value actually used by `update_camera`.
#[derive(Resource)]
struct Orbit {
    yaw: f32,
    pitch: f32,
    distance: f32,
    current_distance: f32,
}

/// Click-to-move state shared by many modules (combat can start or cancel routes).
/// `path` holds remaining `[x, z]` waypoints; `pending` is the background `find_path` task;
/// `status` is the route text shown in the HUD.
#[derive(Resource, Default)]
struct Movement {
    path: VecDeque<[f32; 2]>,
    pending: Option<Task<Result<Vec<[f32; 2]>, mortown_world::navigation::PathError>>>,
    destination: Option<[f32; 2]>,
    status: String,
    // Accumulated walked distance; currently written but never read.
    travelled: f32,
}

/// Ring shown on the ground at the click-to-move destination.
#[derive(Component)]
struct DestinationMarker;

/// Command-line flags for offline previews and automated screenshots
/// (`--view`, `--ui-preview`, `--capture`, `--exit-after`); `frame` counts Update runs.
#[derive(Resource, Default)]
struct RunOptions {
    view: String,
    ui_preview: Option<String>,
    capture: Option<String>,
    frame: u32,
    exit_after: Option<u32>,
}

/// Prefers an `assets/` folder next to the executable (packaged build), otherwise the repo's `assets/`.
fn asset_directory() -> String {
    let packaged = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("assets")));
    packaged
        .filter(|p| p.join("characters/adventurer.glb").is_file())
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets"))
        .to_string_lossy()
        .into_owned()
}

/// Registers everything. Order inside the `.chain()` groups is the real per-frame execution order.
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let value = |flag: &str| args.windows(2).find(|a| a[0] == flag).map(|a| a[1].clone());
    let options = RunOptions {
        view: value("--view").unwrap_or_default(),
        capture: value("--capture"),
        ui_preview: value("--ui-preview"),
        exit_after: value("--exit-after").and_then(|s| s.parse().ok()),
        ..default()
    };
    App::new()
        .insert_resource(Terrain(Arc::new(World::new())))
        .insert_resource(options)
        .insert_resource(ClearColor(Color::srgb(0.52, 0.64, 0.73)))
        .insert_resource(Orbit {
            yaw: 0.25,
            pitch: 0.24,
            distance: 18.0,
            current_distance: 18.0,
        })
        .init_resource::<Movement>()
        .init_resource::<shutdown::Requested>()
        .init_resource::<network::Network>()
        .init_resource::<lobby::Form>()
        .add_plugins(
            DefaultPlugins
                .set(AssetPlugin {
                    file_path: asset_directory(),
                    ..default()
                })
                .set(WindowPlugin {
                    primary_window: Some(Window {
                        title: "Мортаун • Прототип мира".into(),
                        resolution: value("--size")
                            .and_then(|s| {
                                let (w, h) = s.split_once('x')?;
                                Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?))
                            })
                            .unwrap_or((1440, 900))
                            .into(),
                        ..default()
                    }),
                    ..default()
                }),
        )
        // Later setups need resources/entities from earlier ones (e.g. `actionbar::setup` creates `Icons`).
        .add_systems(
            Startup,
            (
                scene::setup,
                ui::setup,
                lobby::setup,
                actionbar::setup,
                inventory::setup,
                crafting::setup,
                foundry::setup,
                combat::setup,
                skills::setup,
                menu::setup,
                skill_effects::setup,
                target::setup,
                exchange::setup,
                objects::setup,
                (chat::setup, chat::windows::setup).chain(),
                party::setup,
                alliance::setup,
                movable::setup,
            )
                .chain(),
        )
        // Model/visual setups run once all UI windows exist.
        .add_systems(
            Startup,
            (
                preview::setup,
                skin::setup,
                appearance::setup,
                ancestry::setup,
                enemies::setup,
                action_fx::setup,
                stations::setup,
            )
                .chain()
                .after(movable::setup),
        )
        // One strict chain: network state first, text input before hotkeys, movement before camera.
        .add_systems(
            Update,
            (
                lobby::buttons,
                lobby::typing,
                lobby::submit,
                network::receive,
                (
                    lobby::display,
                    lobby::layout,
                    lobby::style,
                    lobby::turn_model,
                )
                    .chain(),
                (
                    preview::gallery,
                    preview::hover,
                    // Chat must consume keystrokes before any game hotkey system sees them.
                    (chat::admin::input, chat::windows::input, chat::input).chain(),
                    alliance::input,
                    skin::close,
                    skin::scale,
                )
                    .chain(),
                (
                    movable::input,
                    target::input,
                    party::input,
                    menu::input,
                    exchange::input,
                    objects::input,
                    inventory::input,
                    skills::input,
                    crafting::input,
                    foundry::input,
                )
                    .chain(),
                (
                    ui::character_window,
                    ui::spend_attribute,
                    ui::character_cards,
                )
                    .chain(),
                (
                    crafting::display,
                    (foundry::display, foundry::preview).chain(),
                    inventory::display,
                    skills::display,
                    menu::display,
                    exchange::display,
                    objects::display,
                    objects::sync,
                    chat::display,
                    party::display,
                    alliance::display,
                )
                    .chain(),
                (
                    combat::input,
                    combat::display,
                    enemies::sync,
                    enemies::glow,
                    skill_effects::display,
                    actionbar::input,
                    actionbar::display,
                )
                    .chain(),
                camera_input,
                (click_destination, target::display).chain(),
                move_player,
                // Model preparation: skeleton, equipment, body shape, then pose data for this frame.
                (
                    character::initialize,
                    appearance::prepare,
                    weapon_pose::prepare,
                    ancestry::prepare,
                    ancestry::display,
                    appearance::display,
                    enemies::prepare,
                    player_motion::prepare,
                    player_motion::sync,
                    action_fx::prepare,
                    action_fx::spawn,
                )
                    .chain(),
                character::animate,
                // After `move_player`, so the camera follows this frame's position without lag.
                update_camera,
                scene::update_visibility,
                ui::update,
                ui::button_feedback,
                (shutdown::update, capture_and_exit).chain(),
            )
                .chain(),
        )
        // Bone overrides: after the animation player writes bones, before transforms propagate
        // to rendering. Earlier would be overwritten by animation; later would show a frame late.
        .add_systems(
            PostUpdate,
            ancestry::pose
                .after(bevy::app::AnimationSystems)
                .before(bevy::transform::TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            enemies::reset_pose
                .after(ancestry::pose)
                .before(weapon_pose::pose),
        )
        .add_systems(
            PostUpdate,
            weapon_pose::pose
                .after(ancestry::pose)
                .before(bevy::transform::TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            player_motion::reset
                .after(ancestry::pose)
                .before(weapon_pose::pose),
        )
        .add_systems(
            PostUpdate,
            player_motion::pose
                .after(weapon_pose::pose)
                .before(bevy::transform::TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            action_fx::animate
                .after(player_motion::pose)
                .before(bevy::transform::TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            enemies::pose
                .after(weapon_pose::pose)
                .before(bevy::transform::TransformSystems::Propagate),
        )
        .add_systems(
            PostUpdate,
            ancestry::frame_preview
                .after(ancestry::pose)
                .before(bevy::transform::TransformSystems::Propagate),
        )
        .run();
}

/// Mouse/keyboard camera control: RMB drag orbits, wheel zooms, F toggles overview, Home resets.
/// Only changes `Orbit`; `update_camera` applies it.
#[allow(clippy::too_many_arguments)]
fn camera_input(
    mouse: Res<AccumulatedMouseMotion>,
    scroll: Res<AccumulatedMouseScroll>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut orbit: ResMut<Orbit>,
    panel: Res<ui::CharacterWindow>,
    network: Res<network::Network>,
) {
    if !network.playing() {
        return;
    }
    if panel.open || network.chat.dialog.is_some() {
        return;
    }
    if buttons.pressed(MouseButton::Right) {
        // 0.004 rad per pixel; pitch kept between just above horizon and almost straight down.
        orbit.yaw -= mouse.delta.x * 0.004;
        orbit.pitch = (orbit.pitch + mouse.delta.y * 0.004).clamp(0.06, 1.53);
    }
    // Multiplicative zoom: each wheel notch scales distance by ~16%, so it feels even near and far.
    orbit.distance = (orbit.distance * (-scroll.delta.y * 0.16).exp()).clamp(4.0, 2200.0);
    // 500 m splits "normal" view from the bird's-eye overview.
    if keys.just_pressed(KeyCode::KeyF) {
        if orbit.distance > 500.0 {
            orbit.distance = 18.0;
            orbit.pitch = 0.24;
        } else {
            orbit.distance = 1200.0;
            orbit.pitch = 0.88;
        }
    }
    if keys.just_pressed(KeyCode::Home) {
        orbit.distance = 18.0;
        orbit.pitch = 0.24;
        orbit.yaw = 0.25;
    }
}

/// Left click in the world: selects the closest monster/player under the cursor,
/// otherwise picks a ground point and starts background pathfinding.
#[allow(clippy::too_many_arguments)]
fn click_destination(
    buttons: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    camera: Single<
        (&Camera, &GlobalTransform),
        (
            Without<inventory::PreviewCamera>,
            Without<inventory::PortraitCamera>,
        ),
    >,
    player: Single<&Transform, With<Player>>,
    world: Res<Terrain>,
    mut movement: ResMut<Movement>,
    mut network: ResMut<network::Network>,
    panel: Res<ui::CharacterWindow>,
    mut combat: ResMut<combat::CombatUi>,
    drag: Res<movable::DragState>,
    hud: Query<
        (&ComputedNode, &UiGlobalTransform),
        Or<(With<ui::Hud>, With<ui::WorldInputBlocker>)>,
    >,
) {
    if network.chat.dialog.is_some()
        || drag.blocked
        || panel.open
        || !network.playing()
        || !buttons.just_pressed(MouseButton::Left)
    {
        return;
    }
    let Some(cursor) = window.cursor_position() else {
        return;
    };
    // Non-interactive HUD panels still own their screen area.
    if hud.iter().any(|(node, transform)| {
        actionbar::contains(node, transform, cursor * window.scale_factor())
    }) {
        return;
    }
    // Pick the closest visible actor under the cursor.
    // Tuple: (score, monster id, player id); exactly one id is set. Lower score wins.
    let mut candidates: Vec<(f32, Option<u32>, Option<i64>)> = Vec::new();
    for m in network.monsters.iter().filter(|m| m.hp > 0.) {
        let feet = Vec3::from_array(m.position);
        if feet.distance(player.translation) > 60.
            || !world.0.sight_clear(
                camera.1.translation().to_array(),
                (feet + Vec3::Y).to_array(),
            )
        {
            continue;
        }
        if let (Ok(bottom), Ok(top)) = (
            camera.0.world_to_viewport(camera.1, feet + Vec3::Y * 0.1),
            camera.0.world_to_viewport(camera.1, feet + Vec3::Y * 2.05),
        ) {
            // Distance from cursor to the on-screen feet→head segment; hit radius is 20% of its
            // length (8–36 px). Score is normalized so 23 = edge of the hit radius.
            let axis = top - bottom;
            let t = ((cursor - bottom).dot(axis) / axis.length_squared().max(0.01)).clamp(0., 1.);
            let distance = cursor.distance(bottom + axis * t);
            let radius = (axis.length() * 0.20).clamp(8., 36.);
            if distance <= radius {
                candidates.push((distance / radius * 23., Some(m.id), None));
            }
        }
    }
    // Other players: raw pixel distance to their chest point (no range or sight check).
    for a in &network.roster {
        if let Ok(p) = camera
            .0
            .world_to_viewport(camera.1, Vec3::from_array(a.position) + Vec3::Y)
        {
            candidates.push((p.distance(cursor), None, Some(a.id)));
        }
    }
    if let Some((_, monster, actor)) = candidates
        .into_iter()
        .filter(|(d, ..)| *d < 24.)
        .min_by(|a, b| a.0.total_cmp(&b.0))
    {
        combat.selected = monster;
        combat.player = actor;
        // Selecting a non-enemy stops auto-attack on the server.
        if actor.is_some() {
            network.action(mortown_protocol::ClientMessage::Attack { target: None });
        }
        return;
    }
    let Ok(ray) = camera.0.viewport_to_world(camera.1, cursor) else {
        return;
    };
    let Some(hit) = terrain_hit(ray.origin, *ray.direction, &world.0) else {
        movement.status = "Выберите точку на земле".into();
        return;
    };
    // Route starts from the server position when online (the visible hero may lag slightly behind).
    let from = network
        .authoritative
        .as_ref()
        .map(|p| [p.position[0], p.position[2]])
        .unwrap_or([player.translation.x, player.translation.z]);
    let to = [hit.x, hit.z];
    if network
        .authoritative
        .as_ref()
        .is_some_and(|a| a.powers.click_teleport)
        && network.mode == network::Mode::Online
    {
        movement.path.clear();
        movement.pending = None;
        movement.destination = None;
        movement.status = "Телепорт".into();
        network.teleport(to);
        return;
    }
    let world = world.0.clone();
    movement.path.clear();
    movement.destination = Some(to);
    movement.status = "Прокладываем маршрут…".into();
    // A* can be slow on a large map, so it runs off the main thread; `move_player` polls the result.
    movement.pending =
        Some(AsyncComputeTaskPool::get().spawn(async move { find_path(&world, from, to) }));
}

/// Where a camera ray first meets the ground/floor surface, or `None` if it never does.
fn terrain_hit(origin: Vec3, direction: Vec3, world: &mortown_world::World) -> Option<Vec3> {
    let mut last = origin;
    // Fine near the camera, larger steps at distance; refine the first crossing.
    // Step = 0.8% of distance, 0.4–8 m; 24 km max covers the whole ±8 km map; 14 bisections ≈ mm precision.
    let mut distance = 0.0_f32;
    while distance < 24000.0 {
        distance += (distance * 0.008).clamp(0.4, 8.0);
        let p = origin + direction * distance;
        if p.y <= world.surface_height(p.x, p.z) {
            let mut a = last;
            let mut b = p;
            for _ in 0..14 {
                let m = (a + b) / 2.0;
                if m.y > world.surface_height(m.x, m.z) {
                    a = m;
                } else {
                    b = m;
                }
            }
            return Some((a + b) / 2.0);
        }
        last = p;
    }
    None
}

/// WASD and click-route movement. Online: only sends a target to the server (`Network::intent`);
/// the hero is moved by `network::receive`. Offline (`--view`): moves the hero locally with collision.
/// `network`/`panel` are `Option` so unit tests can run this system without those resources.
// Explicit disjoint query filters prevent conflicting mutable Transform access.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn move_player(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    orbit: Res<Orbit>,
    world: Res<Terrain>,
    mut movement: ResMut<Movement>,
    mut player: Single<&mut Transform, With<Player>>,
    mut marker: Single<
        (&mut Transform, &mut Visibility),
        (With<DestinationMarker>, Without<Player>),
    >,
    mut network: Option<ResMut<network::Network>>,
    panel: Option<Res<ui::CharacterWindow>>,
) {
    if network.as_ref().is_some_and(|n| !n.playing()) {
        return;
    }

    let input = if panel.as_ref().is_some_and(|p| p.open) {
        Vec2::ZERO
    } else {
        Vec2::new(
            (keys.pressed(KeyCode::KeyD) as i32 - keys.pressed(KeyCode::KeyA) as i32) as f32,
            (keys.pressed(KeyCode::KeyW) as i32 - keys.pressed(KeyCode::KeyS) as i32) as f32,
        )
    };
    if input != Vec2::ZERO {
        movement.pending = None;
        movement.path.clear();
        movement.destination = None;
        movement.status = "Свободное движение".into();
    } else if let Some(task) = movement.pending.as_mut()
        && let Some(result) = block_on(future::poll_once(task))
    {
        movement.pending = None;
        match result {
            Ok(path) => {
                movement.path = path.into();
                movement.status = "Следуем по маршруту".into();
            }
            Err(_) => {
                movement.destination = None;
                movement.status = "Нет прохода. Выберите улицу или вход".into();
            }
        }
    }
    let pos = Vec2::new(player.translation.x, player.translation.z);
    // Camera-relative axes on the ground plane (x, z), derived from the orbit yaw.
    let right = Vec2::new(orbit.yaw.cos(), -orbit.yaw.sin());
    let forward = Vec2::new(-orbit.yaw.sin(), -orbit.yaw.cos());
    let delta = if input != Vec2::ZERO {
        (right * input.x + forward * input.y).normalize()
    } else {
        Vec2::ZERO
    };
    // m/s; mirrors protocol WALK_SPEED/RUN_SPEED but only drives offline movement.
    // Online, `speed < 7.` just tells the server "walking".
    let speed = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        5.0
    } else {
        10.0
    };
    if let Some(net) = network.as_mut().filter(|n| n.mode == network::Mode::Online) {
        if let Some(actor) = &net.authoritative {
            let position = [actor.position[0], actor.position[2]];
            // Waypoints within 15 cm of the server position count as reached.
            while movement
                .path
                .front()
                .is_some_and(|p| mortown_world::distance(*p, position) < 0.15)
            {
                movement.path.pop_front();
            }
            // WASD sends a "carrot" 2 m ahead; far enough to keep moving between 45 ms messages.
            let target = if input != Vec2::ZERO {
                [position[0] + delta.x * 2., position[1] + delta.y * 2.]
            } else {
                movement.path.front().copied().unwrap_or(position)
            };
            net.intent(time.elapsed_secs_f64(), target, speed < 7.);
            if movement.path.is_empty() && movement.pending.is_none() && input == Vec2::ZERO {
                movement.destination = None;
            }
            if let Some(p) = movement.destination {
                marker.0.translation =
                    Vec3::new(p[0], world.0.surface_height(p[0], p[1]) + 0.15, p[1]);
                *marker.1 = Visibility::Visible;
            } else {
                *marker.1 = Visibility::Hidden;
            }
        }
        return;
    }
    // Offline path below. dt capped at 0.1 s so a frame hitch can't jump through a wall.
    let step = speed * time.delta_secs().min(0.1);
    let mut reached_waypoint = false;
    let desired = if input == Vec2::ZERO
        && let Some(target) = movement.path.front()
    {
        let target = Vec2::from_array(*target);
        let difference = target - pos;
        if difference.length() <= step {
            reached_waypoint = true;
            target
        } else {
            pos + difference.normalize() * step
        }
    } else {
        pos + delta * step
    };
    let mut next = pos;
    if world.0.segment_clear(pos.to_array(), desired.to_array()) {
        next = desired;
        if reached_waypoint {
            movement.path.pop_front();
        }
    } else if input != Vec2::ZERO {
        // Wall sliding: keep only the X or only the Z part of the step (same rule as server `advance`).
        for candidate in [Vec2::new(desired.x, pos.y), Vec2::new(pos.x, desired.y)] {
            if world.0.segment_clear(pos.to_array(), candidate.to_array()) {
                next = candidate;
                break;
            }
        }
    } else if !movement.path.is_empty() {
        movement.path.clear();
        movement.destination = None;
        movement.status = "Маршрут прерван препятствием".into();
    }
    movement.travelled += next.distance(pos);
    player.translation = Vec3::new(next.x, world.0.surface_height(next.x, next.y), next.y);
    if next.distance_squared(pos) > 0.00001 {
        player.rotation = Quat::from_rotation_y((next.x - pos.x).atan2(next.y - pos.y));
    }
    if movement.pending.is_none() && movement.path.is_empty() && movement.destination.is_some() {
        movement.destination = None;
        movement.status = "На месте".into();
    }
    if let Some(p) = movement.destination {
        marker.0.translation = Vec3::new(p[0], world.0.surface_height(p[0], p[1]) + 0.15, p[1]);
        *marker.1 = Visibility::Visible;
    } else {
        *marker.1 = Visibility::Hidden;
    }
}

/// Places the orbit camera around the hero, pulling it closer when a wall or hill
/// would block the view.
#[allow(clippy::type_complexity)]
fn update_camera(
    time: Res<Time>,
    mut orbit: ResMut<Orbit>,
    world: Res<Terrain>,
    player: Single<&Transform, With<Player>>,
    mut camera: Single<
        &mut Transform,
        (
            With<Camera3d>,
            Without<Player>,
            Without<inventory::PreviewCamera>,
            Without<inventory::PortraitCamera>,
        ),
    >,
) {
    // Frame-rate independent zoom smoothing (rate 8 per second).
    orbit.current_distance +=
        (orbit.distance - orbit.current_distance) * (1.0 - (-time.delta_secs() * 8.0).exp());
    // Look at chest height; `dir` points from the hero toward the camera (spherical coords from yaw/pitch).
    let target = player.translation + Vec3::Y * 1.5;
    let dir = Vec3::new(
        orbit.yaw.sin() * orbit.pitch.cos(),
        orbit.pitch.sin(),
        orbit.yaw.cos() * orbit.pitch.cos(),
    );
    let mut distance = orbit.current_distance;
    let in_hall = world.0.hall_at(target.x, target.z);
    // Skip small details and the roof of the hall we're standing in (it is hidden anyway).
    for block in &world.0.blocks {
        if !block.camera_solid
            || block.detail
            || (block.roof_of.is_some() && block.roof_of == in_hall)
        {
            continue;
        }
        let center = Vec3::from_array(block.center);
        // Box grown by 0.25 m so the near plane doesn't clip into the wall.
        let half = Vec3::from_array(block.size) * 0.5 + Vec3::splat(0.25);
        // Rotate the ray into the block's local frame so the box test can be axis-aligned.
        let inverse = Quat::from_rotation_y(-block.yaw);
        if let Some(hit) = ray_box(inverse * (target - center), inverse * dir, -half, half) {
            distance = distance.min((hit - 0.25).max(0.5));
        }
    }
    // Terrain clearance along the entire camera arm, including hills behind the player.
    let samples = (distance / 2.0).ceil().max(1.0) as usize;
    for i in 1..=samples {
        let d = distance * i as f32 / samples as f32;
        let p = target + dir * d;
        if p.y < world.0.surface_height(p.x, p.z) + 0.3 {
            distance = (d - 2.0).max(0.5);
            break;
        }
    }
    **camera = Transform::from_translation(target + dir * distance).looking_at(target, Vec3::Y);
}

/// Slab test: distance along the ray to an axis-aligned box, `None` if missed or behind.
/// Returns 0 when the origin is already inside the box.
fn ray_box(origin: Vec3, direction: Vec3, min: Vec3, max: Vec3) -> Option<f32> {
    let mut near = 0.0_f32;
    let mut far = f32::INFINITY;
    for axis in 0..3 {
        if direction[axis].abs() < 1e-6 {
            if origin[axis] < min[axis] || origin[axis] > max[axis] {
                return None;
            }
        } else {
            let a = (min[axis] - origin[axis]) / direction[axis];
            let b = (max[axis] - origin[axis]) / direction[axis];
            near = near.max(a.min(b));
            far = far.min(a.max(b));
            if near > far {
                return None;
            }
        }
    }
    (far >= 0.0).then_some(near)
}

/// Screenshot/auto-exit helper for previews: captures at frame 100 (and every 120 frames in the
/// gallery) or on Ctrl+Shift+F12 when `--capture` is set; quits after `--exit-after` frames.
fn capture_and_exit(
    mut commands: Commands,
    mut options: ResMut<RunOptions>,
    mut exit: MessageWriter<AppExit>,
    keys: Res<ButtonInput<KeyCode>>,
) {
    options.frame += 1;
    if ((options.ui_preview.as_deref() == Some("gallery")
        && options.frame >= 100
        && (options.frame - 100).is_multiple_of(120))
        || options.frame == 100
        || (keys.just_pressed(KeyCode::F12)
            && keys.pressed(KeyCode::ControlLeft)
            && keys.pressed(KeyCode::ShiftLeft)))
        && let Some(path) = &options.capture
    {
        commands
            .spawn(Screenshot::primary_window())
            .observe(save_to_disk(path.clone()));
    }
    if options.exit_after.is_some_and(|n| options.frame >= n) {
        exit.write(AppExit::Success);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal headless app running only `move_player` (offline path), with a fixed 100 ms frame.
    fn movement_app(at: [f32; 2]) -> App {
        let mut app = App::new();
        let mut time = Time::<()>::default();
        time.advance_by(std::time::Duration::from_millis(100));
        app.insert_resource(time)
            .insert_resource(Terrain(Arc::new(World::new())))
            .insert_resource(Orbit {
                yaw: 0.0,
                pitch: 0.24,
                distance: 18.0,
                current_distance: 18.0,
            })
            .init_resource::<Movement>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, move_player);
        app.world_mut().spawn((
            Player,
            Transform::from_xyz(at[0], height(at[0], at[1]), at[1]),
        ));
        app.world_mut()
            .spawn((DestinationMarker, Transform::default(), Visibility::Hidden));
        app
    }

    #[test]
    fn running_is_default_and_shift_walks() {
        for (walking, expected) in [(false, 1.0), (true, 0.5)] {
            let mut app = movement_app([0., 60.]);
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::KeyW);
            if walking {
                keys.press(KeyCode::ShiftLeft);
            }
            app.update();
            let mut q = app.world_mut().query_filtered::<&Transform, With<Player>>();
            let p = q.single(app.world()).unwrap().translation;
            assert!((60. - p.z - expected).abs() < 0.001);
        }
    }

    #[test]
    fn keyboard_cancels_click_route_and_moves_relative_to_camera() {
        let mut app = movement_app([0.0, 60.0]);
        let mut movement = app.world_mut().resource_mut::<Movement>();
        movement.path.push_back([10.0, 60.0]);
        movement.destination = Some([10.0, 60.0]);
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyW);
        app.update();
        let movement = app.world().resource::<Movement>();
        assert!(movement.path.is_empty());
        assert!(movement.destination.is_none());
        let mut query = app.world_mut().query_filtered::<&Transform, With<Player>>();
        let p = query.single(app.world()).unwrap().translation;
        assert!(p.z < 60.0 && p.x.abs() < 0.001);
    }

    #[test]
    fn sustained_manual_movement_cannot_cross_fortifications() {
        let world = World::new();
        let wall = world.obstacles[0];
        let start = mortown_world::rotate(wall.center, 0., 12., wall.yaw);
        let mut app = movement_app(start);
        app.world_mut().resource_mut::<Orbit>().yaw = wall.yaw;
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.press(KeyCode::KeyW);
        keys.press(KeyCode::ShiftLeft);
        for _ in 0..30 {
            app.update();
        }
        let mut query = app.world_mut().query_filtered::<&Transform, With<Player>>();
        let p = query.single(app.world()).unwrap().translation;
        assert!(mortown_world::inside_city([p.x, p.z]));
        assert!(mortown_world::distance([p.x, p.z], start) > 1.);
        assert!(app.world().resource::<Terrain>().0.walkable(p.x, p.z));
    }

    #[test]
    fn wasd_crosses_every_civic_stairway_without_sticking() {
        let world = World::new();
        for hall in world.halls.iter().take(4) {
            let start = mortown_world::rotate(hall.center, 0., hall.half[1] + 12., hall.yaw);
            let mut app = movement_app(start);
            app.world_mut().resource_mut::<Orbit>().yaw = hall.yaw;
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::KeyW);
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(KeyCode::ShiftLeft);
            for _ in 0..40 {
                app.update();
            }
            let mut q = app.world_mut().query_filtered::<&Transform, With<Player>>();
            let p = q.single(app.world()).unwrap().translation;
            let expected = mortown_world::rotate(start, 0., -20., hall.yaw);
            assert!(
                Vec2::new(p.x - expected[0], p.z - expected[1]).length() < 0.1,
                "WASD stuck at {}: {p:?}",
                hall.name
            );
            assert!((p.y - world.surface_height(p.x, p.z)).abs() < 0.001);
        }
    }
    #[test]
    fn click_route_reaches_each_hall_using_the_actual_movement_system() {
        let world = World::new();
        for hall in &world.halls {
            let mut app = movement_app(mortown_world::SPAWN);
            let path = find_path(&world, mortown_world::SPAWN, hall.center).unwrap();
            app.world_mut().resource_mut::<Movement>().path = path.into();
            app.world_mut().resource_mut::<Movement>().destination = Some(hall.center);
            for _ in 0..4000 {
                app.update();
                if app.world().resource::<Movement>().path.is_empty() {
                    break;
                }
            }
            let mut q = app.world_mut().query_filtered::<&Transform, With<Player>>();
            let p = q.single(app.world()).unwrap().translation;
            assert!(
                Vec2::new(p.x - hall.center[0], p.z - hall.center[1]).length() < 0.2,
                "click movement stopped at {}: {p:?}",
                hall.name
            );
        }
    }

    #[test]
    fn camera_ray_handles_parallel_and_behind_boxes() {
        assert_eq!(
            ray_box(
                Vec3::ZERO,
                Vec3::Z,
                Vec3::new(-1.0, -1.0, 5.0),
                Vec3::new(1.0, 1.0, 7.0)
            ),
            Some(5.0)
        );
        assert_eq!(
            ray_box(Vec3::ZERO, Vec3::Z, Vec3::splat(-3.0), Vec3::splat(-1.0)),
            None
        );
    }
}
