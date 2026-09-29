//! Dynamic bow string, nocked arrow and cosmetic flights for confirmed actions.
use crate::{
    appearance::EquipmentRig,
    network::{Mode, Network},
    player_motion::{self, PlayerMotion},
    weapon_pose::{self, GripRig},
};
use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology, prelude::*};
use mortown_protocol::{
    actions::{Kind, Motion},
    weapons::BOW,
};
use std::collections::HashSet;

#[derive(Resource)]
pub struct AssetsFx {
    arrow: Handle<Mesh>,
    arrow_mat: Handle<StandardMaterial>,
    string: Handle<Mesh>,
    string_mat: Handle<StandardMaterial>,
    orb: Handle<Mesh>,
    magic: Handle<StandardMaterial>,
}
#[derive(Resource, Default)]
pub struct Seen(HashSet<(i64, u64)>);
#[derive(Component)]
pub struct BowVisual {
    owner: Entity,
    strings: [Entity; 2],
    arrow: Entity,
}
#[derive(Component)]
pub struct HandGlow {
    owner: Entity,
}
#[derive(Component)]
pub struct EffectsReady;
#[derive(Component)]
pub struct Flight {
    actor: i64,
    action: Motion,
    received_at: f32,
    launch: Option<Vec3>,
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.init_resource::<Seen>();
    commands.insert_resource(AssetsFx {
        arrow: meshes.add(arrow_mesh()),
        arrow_mat: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.72,
            cull_mode: None,
            ..default()
        }),
        string: meshes.add(Cylinder::new(0.0018, 1.)),
        string_mat: materials.add(Color::srgb(0.48, 0.40, 0.28)),
        orb: meshes.add(Sphere::new(0.075).mesh().ico(2).unwrap()),
        magic: materials.add(StandardMaterial {
            base_color: Color::srgb(0.25, 0.63, 0.95),
            emissive: LinearRgba::rgb(0.3, 1.3, 2.9),
            ..default()
        }),
    });
}
fn arrow_mesh() -> Mesh {
    let mut vertices = Vec::new();
    let mut colors = Vec::new();
    let mut triangle = |points: [[f32; 3]; 3], color: [f32; 4]| {
        vertices.extend(points);
        colors.extend([color; 3]);
    };
    let wood = [0.30, 0.15, 0.06, 1.];
    let iron = [0.45, 0.50, 0.54, 1.];
    let feather = [0.58, 0.48, 0.31, 1.];
    for i in 0..4 {
        let a = i as f32 * std::f32::consts::FRAC_PI_2;
        let b = a + std::f32::consts::FRAC_PI_2;
        let v = |a: f32, z: f32, r: f32| [a.cos() * r, a.sin() * r, z];
        triangle([v(a, 0., 0.004), v(b, 0., 0.004), v(a, 0.76, 0.004)], wood);
        triangle(
            [v(b, 0., 0.004), v(b, 0.76, 0.004), v(a, 0.76, 0.004)],
            wood,
        );
        triangle([v(a, 0.76, 0.016), v(b, 0.76, 0.016), [0., 0., 0.83]], iron);
        triangle(
            [v(a, 0.03, 0.004), v(a, 0.04, 0.032), v(a, 0.15, 0.004)],
            feather,
        );
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_computed_flat_normals()
}
pub fn prepare(
    mut commands: Commands,
    assets: Res<AssetsFx>,
    roots: Query<(
        Entity,
        &EquipmentRig,
        &GripRig,
        Option<&PlayerMotion>,
        Option<&EffectsReady>,
    )>,
    bows: Query<(), With<BowVisual>>,
    glows: Query<(Entity, &HandGlow, &ChildOf)>,
) {
    for (owner, equipment, grip, player, ready) in &roots {
        let free_hand = if equipment.weapon_kind() == Some(BOW) {
            grip.right[2]
        } else {
            grip.left[2]
        };
        if let Some((entity, _, parent)) = glows.iter().find(|(_, glow, _)| glow.owner == owner)
            && parent.parent() != free_hand
        {
            commands.entity(entity).insert(ChildOf(free_hand));
        }
        if player.is_some() && ready.is_none() {
            commands.spawn((
                HandGlow { owner },
                Mesh3d(assets.orb.clone()),
                MeshMaterial3d(assets.magic.clone()),
                Transform::from_xyz(0., 0.14, 0.02),
                Visibility::Hidden,
                ChildOf(free_hand),
            ));
            commands.entity(owner).insert(EffectsReady);
        }
        if equipment.weapon_kind() != Some(BOW) {
            continue;
        }
        let Some(weapon) = equipment.weapon_entity().filter(|&e| !bows.contains(e)) else {
            continue;
        };
        let strings = std::array::from_fn(|_| {
            commands
                .spawn((
                    Mesh3d(assets.string.clone()),
                    MeshMaterial3d(assets.string_mat.clone()),
                    Transform::default(),
                    equipment.layer.clone(),
                    ChildOf(weapon),
                ))
                .id()
        });
        let arrow = commands
            .spawn((
                Mesh3d(assets.arrow.clone()),
                MeshMaterial3d(assets.arrow_mat.clone()),
                Transform::from_xyz(0.025, 0., -0.11),
                equipment.layer.clone(),
                ChildOf(weapon),
            ))
            .id();
        commands.entity(weapon).insert(BowVisual {
            owner,
            strings,
            arrow,
        });
    }
}
fn actors(network: &Network) -> impl Iterator<Item = &mortown_protocol::Actor> {
    network.authoritative.iter().chain(
        network
            .roster
            .iter()
            .filter(|a| Some(a.id) != network.authoritative.as_ref().map(|a| a.id)),
    )
}
fn is_flight(action: &Motion) -> bool {
    action.target.is_some()
        && (action.kind == Kind::Spell
            || (action.kind == Kind::Weapon && action.weapon == Some(BOW)))
}
pub fn spawn(
    mut commands: Commands,
    network: Res<Network>,
    time: Res<Time>,
    assets: Res<AssetsFx>,
    mut seen: ResMut<Seen>,
    flights: Query<Entity, With<Flight>>,
) {
    if network.mode != Mode::Online {
        for entity in &flights {
            commands.entity(entity).despawn();
        }
        seen.0.clear();
        return;
    }
    let current: HashSet<_> = actors(&network)
        .flat_map(|a| a.actions.iter().map(move |m| (a.id, m.sequence)))
        .collect();
    seen.0.retain(|id| current.contains(id));
    for actor in actors(&network).filter(|a| a.hp > 0.) {
        for action in actor.actions.iter().filter(|a| is_flight(a)) {
            if !seen.0.insert((actor.id, action.sequence)) {
                continue;
            }
            let arrow = action.kind == Kind::Weapon;
            commands.spawn((
                Flight {
                    actor: actor.id,
                    action: *action,
                    received_at: time.elapsed_secs(),
                    launch: None,
                },
                Mesh3d(if arrow {
                    assets.arrow.clone()
                } else {
                    assets.orb.clone()
                }),
                MeshMaterial3d(if arrow {
                    assets.arrow_mat.clone()
                } else {
                    assets.magic.clone()
                }),
                Transform::from_translation(Vec3::from_array(action.origin) + Vec3::Y * 1.4),
                Visibility::Hidden,
            ));
        }
    }
}
#[allow(clippy::too_many_arguments)]
pub fn animate(
    mut commands: Commands,
    network: Res<Network>,
    time: Res<Time>,
    players: Query<&PlayerMotion>,
    bows: Query<(Entity, &BowVisual)>,
    glows: Query<(Entity, &HandGlow)>,
    mut flights: Query<(Entity, &mut Flight)>,
    mut transforms: Query<&mut Transform>,
    mut visible: Query<&mut Visibility>,
    parents: Query<&ChildOf>,
) {
    let now = time.elapsed_secs();
    for (_, bow) in &bows {
        let sample = players
            .get(bow.owner)
            .ok()
            .and_then(|p| p.sample(now))
            .filter(|s| s.action.kind == Kind::Weapon && s.action.weapon == Some(BOW));
        let pull = sample.map_or(0., |s| player_motion::bow_draw(s.t));
        for (i, entity) in bow.strings.iter().enumerate() {
            let tip = Vec3::new(0., if i == 0 { -0.69 } else { 0.69 }, -0.11);
            let nock = Vec3::new(0., 0., -0.11 - pull);
            let delta = nock - tip;
            if let Ok(mut transform) = transforms.get_mut(*entity) {
                *transform = Transform::from_translation((tip + nock) * 0.5)
                    .with_rotation(Quat::from_rotation_arc(Vec3::Y, delta.normalize()))
                    .with_scale(Vec3::new(1., delta.length(), 1.));
            }
        }
        if let Ok(mut transform) = transforms.get_mut(bow.arrow) {
            transform.translation = Vec3::new(0.025, 0., -0.11 - pull);
        }
        if let Ok(mut visibility) = visible.get_mut(bow.arrow) {
            *visibility = if sample.is_some_and(|s| (0.48..0.90).contains(&s.t)) {
                Visibility::Hidden
            } else {
                Visibility::Inherited
            };
        }
    }
    for (entity, glow) in &glows {
        let sample = players
            .get(glow.owner)
            .ok()
            .and_then(|p| p.sample(now))
            .filter(|s| s.action.kind == Kind::Spell && s.t < 0.64);
        if let Ok(mut visibility) = visible.get_mut(entity) {
            *visibility = if sample.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
        }
        if let Some(sample) = sample
            && let Ok(mut transform) = transforms.get_mut(entity)
        {
            transform.scale = Vec3::splat(if sample.t < 0.48 {
                0.3 + sample.t * 3.
            } else {
                (0.64 - sample.t) * 10.
            });
        }
    }
    for (entity, mut flight) in &mut flights {
        let actor = actors(&network).find(|a| a.id == flight.actor);
        if network.mode != Mode::Online || actor.is_none() {
            commands.entity(entity).despawn();
            continue;
        }
        if let Some(update) = actor.and_then(|a| {
            a.actions
                .iter()
                .find(|m| m.sequence == flight.action.sequence)
        }) && update.elapsed_ms != flight.action.elapsed_ms
        {
            flight.action = *update;
            flight.received_at = now;
        }
        let age = flight.action.elapsed_ms as f32 / 1000. + (now - flight.received_at).max(0.);
        let release = flight.action.duration_ms as f32 / 1000. * 0.48;
        if age < release {
            if actor.is_some_and(|a| a.hp <= 0.) {
                commands.entity(entity).despawn();
            }
            continue;
        }
        let arrow = flight.action.kind == Kind::Weapon;
        if flight.launch.is_none() {
            let attachment = if arrow {
                bows.iter()
                    .find(|(_, b)| {
                        players.get(b.owner).is_ok_and(|p| {
                            p.actor_id() == Some(flight.actor)
                                && p.sample(now)
                                    .is_some_and(|s| s.action.sequence == flight.action.sequence)
                        })
                    })
                    .and_then(|(weapon, _)| {
                        weapon_pose::global(weapon, &transforms, &parents)
                            .map(|m| m.transform_point3(Vec3::new(0.025, 0., -0.36)))
                    })
            } else {
                glows
                    .iter()
                    .find(|(_, g)| {
                        players.get(g.owner).is_ok_and(|p| {
                            p.actor_id() == Some(flight.actor)
                                && p.sample(now)
                                    .is_some_and(|s| s.action.sequence == flight.action.sequence)
                        })
                    })
                    .and_then(|(e, _)| {
                        weapon_pose::global(e, &transforms, &parents)
                            .map(|m| m.transform_point3(Vec3::ZERO))
                    })
            };
            // Late observers use the captured server origin, not a new shot from a moved actor.
            flight.launch = Some(
                if age - release < 0.10 {
                    attachment
                } else {
                    None
                }
                .unwrap_or(Vec3::from_array(flight.action.origin) + Vec3::Y * 1.4),
            );
        }
        let from = flight.launch.unwrap();
        let target = Vec3::from_array(flight.action.target.unwrap()) + Vec3::Y;
        let travel = (from.distance(target) / if arrow { 42. } else { 28. }).clamp(0.12, 0.65);
        let t = (age - release) / travel;
        if t >= 1. {
            commands.entity(entity).despawn();
            continue;
        }
        if let Ok(mut visibility) = visible.get_mut(entity) {
            *visibility = Visibility::Inherited;
        }
        if let Ok(mut transform) = transforms.get_mut(entity) {
            transform.translation = from.lerp(target, t);
            transform.rotation =
                Quat::from_rotation_arc(Vec3::Z, (target - from).normalize_or(Vec3::Z));
        }
    }
}
