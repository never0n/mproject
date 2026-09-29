//! Enemy presentation. Positions, health, casts and status effects remain server-owned.
mod model;
mod motion;
use crate::{
    Terrain,
    ancestry::BodyRig,
    appearance::{EquipmentRig, Wearer},
    character::{AnimatedActor, AnimationHold, Gait},
    combat::CombatUi,
    network::{Mode, Network},
};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::NoFrustumCulling,
    mesh::{Indices, PrimitiveTopology},
    prelude::*,
};
use mortown_protocol::{monsters::Kind, skills::StatusKind};

#[derive(Component)]
pub struct EnemyView {
    id: u32,
    hp: f64,
    hit_until: f32,
    ring: Entity,
    ring_mesh: Handle<Mesh>,
    ring_position: Option<Vec3>,
    motion: motion::Playback,
}
#[derive(Component)]
pub struct EnemyModel {
    owner: Entity,
    id: u32,
    kind: Kind,
    shape: Entity,
}
#[derive(Component)]
pub struct EnemyRig {
    chest: Entity,
    left: [Entity; 3],
    right: [Entity; 3],
    bones: Vec<Entity>,
    fall: Option<motion::Fall>,
}
#[derive(Component)]
pub struct CastGlow(u32);
#[derive(Component)]
pub struct GroundRing;
struct Costume {
    body: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    headgear: Handle<Mesh>,
}
#[derive(Resource)]
pub struct EnemyAssets {
    scene: Handle<Scene>,
    costumes: [Option<Costume>; 3],
    cloth: Handle<StandardMaterial>,
    fabric: Handle<StandardMaterial>,
    steel: Handle<StandardMaterial>,
    dark_wood: Handle<StandardMaterial>,
    bandit_cloth: Handle<Mesh>,
    robe: Handle<Mesh>,
    shield: [Handle<Mesh>; 2],
    nasal: Handle<Mesh>,
    glow_mesh: Handle<Mesh>,
    glow_material: Handle<StandardMaterial>,
    rings: [Handle<StandardMaterial>; 8],
}
pub fn setup(
    mut commands: Commands,
    server: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let rings = [
        (0.90, 0.47, 0.16),
        (1., 0.88, 0.55),
        (0.25, 0.67, 0.85),
        (0.82, 0.12, 0.15),
        (1., 0.36, 0.08),
        (0.39, 0.72, 0.15),
        (0.68, 0.44, 0.89),
        (0.77, 0.63, 0.32),
    ]
    .map(|(r, g, b)| {
        materials.add(StandardMaterial {
            base_color: Color::srgba(r, g, b, 0.85),
            unlit: true,
            alpha_mode: AlphaMode::Blend,
            cull_mode: None,
            ..default()
        })
    });
    commands.insert_resource(EnemyAssets {
        scene: server.load(GltfAssetLabel::Scene(0).from_asset("characters/adventurer.glb")),
        costumes: std::array::from_fn(|_| None),
        cloth: materials.add(StandardMaterial {
            base_color: Color::srgb(0.29, 0.25, 0.27),
            perceptual_roughness: 0.95,
            cull_mode: None,
            double_sided: true,
            ..default()
        }),
        fabric: materials.add(StandardMaterial {
            base_color: Color::WHITE,
            perceptual_roughness: 0.95,
            cull_mode: None,
            double_sided: true,
            ..default()
        }),
        steel: materials.add(StandardMaterial {
            base_color: Color::srgb(0.35, 0.40, 0.43),
            metallic: 0.80,
            perceptual_roughness: 0.48,
            cull_mode: None,
            double_sided: true,
            ..default()
        }),
        dark_wood: materials.add(StandardMaterial {
            base_color: Color::srgb(0.16, 0.095, 0.06),
            perceptual_roughness: 0.90,
            ..default()
        }),
        bandit_cloth: meshes.add(model::cloth_panels(Kind::Bandit)),
        robe: meshes.add(model::cloth_panels(Kind::Apostate)),
        shield: [
            meshes.add(model::shield(false)),
            meshes.add(model::shield(true)),
        ],
        nasal: meshes.add(Cuboid::new(0.026, 0.14, 0.020)),
        glow_mesh: meshes.add(Sphere::new(0.07).mesh().ico(2).unwrap()),
        glow_material: materials.add(StandardMaterial {
            base_color: Color::srgba(0.31, 0.65, 0.95, 0.80),
            emissive: LinearRgba::rgb(0.5, 1.8, 3.6),
            alpha_mode: AlphaMode::Blend,
            ..default()
        }),
        rings,
    });
}

fn ring_mesh(feet: Vec3, world: &mortown_world::World) -> Mesh {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for i in 0..=48 {
        let angle = i as f32 / 48. * std::f32::consts::TAU;
        for r in [0.62, 0.67] {
            let x = angle.cos() * r;
            let z = angle.sin() * r;
            vertices.push([
                x,
                world.surface_height(feet.x + x, feet.z + z) - feet.y + 0.045,
                z,
            ]);
        }
        if i < 48 {
            let a = i * 2;
            indices.extend_from_slice(&[a, a + 2, a + 1, a + 1, a + 2, a + 3]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0., 1., 0.]; 98])
    .with_inserted_indices(Indices::U32(indices))
}
fn effect_color(m: &mortown_protocol::Monster, selected: bool, hit: bool) -> Option<usize> {
    if hit {
        return Some(1);
    }
    if let Some(effect) = m.effects.first() {
        return Some(match effect.kind {
            StatusKind::Bleeding => 3,
            StatusKind::Burning => 4,
            StatusKind::Poisoned | StatusKind::Corroded => 5,
            StatusKind::Blinded | StatusKind::Stunned => 6,
            StatusKind::Crippled | StatusKind::Quicksand => 7,
            StatusKind::Smoke => 2,
        });
    }
    if m.slow_ms > 0 {
        Some(2)
    } else if selected {
        Some(0)
    } else {
        None
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn sync(
    mut commands: Commands,
    network: Res<Network>,
    world: Res<Terrain>,
    ui: Res<CombatUi>,
    assets: Res<EnemyAssets>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut views: Query<(
        Entity,
        &mut EnemyView,
        &mut Transform,
        &mut Visibility,
        &mut AnimationHold,
    )>,
    mut rings: Query<
        (
            &mut MeshMaterial3d<StandardMaterial>,
            &mut Transform,
            &mut Visibility,
        ),
        (With<GroundRing>, Without<EnemyView>),
    >,
) {
    let mut seen = std::collections::HashSet::new();
    for (entity, mut view, mut transform, mut visibility, mut hold) in &mut views {
        let Some(m) = network
            .monsters
            .iter()
            .find(|m| m.id == view.id)
            .filter(|_| network.mode == Mode::Online)
        else {
            commands.entity(entity).despawn();
            continue;
        };
        seen.insert(m.id);
        let respawned = view.hp <= 0. && m.hp > 0.;
        let died = view.hp > 0. && m.hp <= 0.;
        view.motion.observe(m, time.elapsed_secs());
        hold.0 = m.hp <= 0.;
        if m.hp < view.hp {
            view.hit_until = time.elapsed_secs() + 0.22;
        }
        if respawned {
            view.hit_until = 0.;
        }
        view.hp = m.hp;
        let target = Vec3::from_array(m.position);
        if respawned || died || transform.translation.distance(target) > 3. {
            transform.translation = target;
        } else {
            let blend = 1. - (-time.delta_secs() * 22.).exp();
            transform.translation = transform.translation.lerp(target, blend);
            // Interpolating two heights can put the feet inside a hill between snapshots.
            transform.translation.y = world
                .0
                .surface_height(transform.translation.x, transform.translation.z);
        }
        if died || respawned {
            transform.rotation = Quat::from_rotation_y(m.yaw);
        } else if m.hp > 0. {
            transform.rotation = transform.rotation.slerp(
                Quat::from_rotation_y(m.yaw),
                1. - (-time.delta_secs() * 18.).exp(),
            );
        }
        *visibility = if m.hp > 0.
            || view
                .motion
                .death_age(time.elapsed_secs())
                .is_some_and(|age| age < motion::CORPSE_SECONDS)
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Ok((mut material, mut ring_transform, mut visible)) = rings.get_mut(view.ring) {
            let color = effect_color(
                m,
                Some(m.id) == ui.selected,
                time.elapsed_secs() < view.hit_until,
            );
            *visible = if m.hp > 0. && color.is_some() {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if let Some(color) = color {
                material.0 = assets.rings[color].clone();
                ring_transform.rotation = transform.rotation.inverse();
                if view
                    .ring_position
                    .is_none_or(|p| p.distance(transform.translation) > 0.015)
                {
                    if let Some(mesh) = meshes.get_mut(&view.ring_mesh) {
                        *mesh = ring_mesh(transform.translation, &world.0);
                    }
                    view.ring_position = Some(transform.translation);
                }
            }
        }
    }
    if network.mode != Mode::Online {
        return;
    }
    for m in network.monsters.iter().filter(|m| !seen.contains(&m.id)) {
        let feet = Vec3::from_array(m.position);
        let root = commands
            .spawn((
                Name::new(format!("{} #{}", m.name, m.id)),
                Transform::from_translation(feet).with_rotation(Quat::from_rotation_y(m.yaw)),
                if m.hp > 0. {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                },
                AnimatedActor::default(),
                AnimationHold(m.hp <= 0.),
                Gait {
                    walk_speed: 1.7,
                    run_speed: 4.,
                },
            ))
            .id();
        let ring_mesh = meshes.add(ring_mesh(feet, &world.0));
        let ring = commands
            .spawn((
                GroundRing,
                Mesh3d(ring_mesh.clone()),
                MeshMaterial3d(assets.rings[0].clone()),
                Transform::from_rotation(Quat::from_rotation_y(-m.yaw)),
                Visibility::Hidden,
                ChildOf(root),
            ))
            .id();
        let mut motion = motion::Playback::default();
        motion.observe(m, time.elapsed_secs());
        commands.entity(root).insert(EnemyView {
            id: m.id,
            hp: m.hp,
            hit_until: 0.,
            ring,
            ring_mesh,
            ring_position: Some(feet),
            motion,
        });
        let scale = match m.kind {
            Kind::Bandit => Vec3::new(0.96, 0.98, 0.96),
            Kind::Marauder => Vec3::new(1.10, 1.04, 1.07),
            Kind::Apostate => Vec3::new(0.94, 1.02, 0.97),
        };
        let shape = commands
            .spawn((
                Transform::from_scale(scale),
                Visibility::Inherited,
                ChildOf(root),
            ))
            .id();
        commands.spawn((
            SceneRoot(assets.scene.clone()),
            Transform::default(),
            Wearer::Enemy(m.kind),
            EnemyModel {
                owner: root,
                id: m.id,
                kind: m.kind,
                shape,
            },
            ChildOf(shape),
        ));
    }
}

#[allow(clippy::too_many_arguments)]
pub fn prepare(
    mut commands: Commands,
    mut assets: ResMut<EnemyAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    roots: Query<(Entity, &EnemyModel, &EquipmentRig), (With<BodyRig>, Without<EnemyRig>)>,
    bodies: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>)>,
    children: Query<&Children>,
    names: Query<&Name>,
) {
    for (root, model, rig) in &roots {
        let find = |name: &str| {
            children
                .iter_descendants(root)
                .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == name))
        };
        let (
            Some(head),
            Some(chest),
            Some(upper),
            Some(forearm),
            Some(hand),
            Some(right_upper),
            Some(right_forearm),
            Some(right_hand),
        ) = (
            find("Head"),
            find("Spine1"),
            find("LeftArm"),
            find("LeftForeArm"),
            find("LeftHand"),
            find("RightArm"),
            find("RightForeArm"),
            find("RightHand"),
        )
        else {
            continue;
        };
        let i = model.kind.index();
        if assets.costumes[i].is_none() {
            let Ok((body_mesh, body_material)) = bodies.get(rig.body) else {
                continue;
            };
            let (Some(body_source), Some(full_source), Some(material)) = (
                meshes.get(&body_mesh.0),
                meshes.get(&rig.source),
                materials.get(&body_material.0),
            ) else {
                continue;
            };
            let (Some(body), Some(headgear)) = (
                model::clothing(body_source, model.kind),
                model::headgear(full_source, model.kind),
            ) else {
                continue;
            };
            let mut material = material.clone();
            material.base_color = Color::WHITE;
            material.metallic = 0.;
            material.perceptual_roughness = 0.87;
            assets.costumes[i] = Some(Costume {
                body: meshes.add(body),
                headgear: meshes.add(headgear),
                material: materials.add(material),
            });
        }
        let costume = assets.costumes[i].as_ref().unwrap();
        commands.entity(rig.body).insert((
            Mesh3d(costume.body.clone()),
            MeshMaterial3d(costume.material.clone()),
        ));
        commands.spawn((
            Name::new("Enemy fitted headgear"),
            Mesh3d(costume.headgear.clone()),
            MeshMaterial3d(if model.kind == Kind::Marauder {
                assets.steel.clone()
            } else {
                assets.cloth.clone()
            }),
            Transform::default(),
            rig.skin.clone(),
            NoFrustumCulling,
            ChildOf(rig.body),
        ));
        if model.kind != Kind::Marauder {
            // A white material preserves the panels' woven-color vertex palette.
            commands.spawn((
                Name::new("Enemy cloth panels"),
                Mesh3d(if model.kind == Kind::Bandit {
                    assets.bandit_cloth.clone()
                } else {
                    assets.robe.clone()
                }),
                MeshMaterial3d(assets.fabric.clone()),
                Transform::default(),
                rig.skin.clone(),
                NoFrustumCulling,
                ChildOf(rig.body),
            ));
        } else {
            commands.spawn((
                Name::new("Helmet nasal guard"),
                Mesh3d(assets.nasal.clone()),
                MeshMaterial3d(assets.steel.clone()),
                Transform::from_xyz(0., 0.11, 0.158),
                ChildOf(head),
            ));
            let shield = commands
                .spawn((
                    Name::new("Marauder kite shield"),
                    Transform::from_xyz(0., 0.05, -0.08)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                    Visibility::Inherited,
                    ChildOf(hand),
                ))
                .id();
            for j in 0..2 {
                commands.spawn((
                    Mesh3d(assets.shield[j].clone()),
                    MeshMaterial3d(if j == 0 {
                        assets.dark_wood.clone()
                    } else {
                        assets.steel.clone()
                    }),
                    Transform::default(),
                    ChildOf(shield),
                ));
            }
        }
        if model.kind == Kind::Apostate {
            commands.spawn((
                CastGlow(model.id),
                Mesh3d(assets.glow_mesh.clone()),
                MeshMaterial3d(assets.glow_material.clone()),
                Transform::from_xyz(0., 0.14, 0.02),
                Visibility::Hidden,
                ChildOf(hand),
            ));
        }
        commands.entity(root).insert(EnemyRig {
            chest,
            left: [upper, forearm, hand],
            right: [right_upper, right_forearm, right_hand],
            bones: rig.skin.joints.clone(),
            fall: None,
        });
    }
}

pub fn glow(
    network: Res<Network>,
    time: Res<Time>,
    mut glows: Query<(&CastGlow, &mut Transform, &mut Visibility)>,
) {
    for (glow, mut transform, mut visible) in &mut glows {
        let monster = network
            .monsters
            .iter()
            .find(|m| m.id == glow.0 && m.hp > 0.)
            .filter(|_| network.mode == Mode::Online);
        let cast = monster.and_then(|m| m.cast.as_ref());
        let burst = monster
            .and_then(|m| m.attack)
            .filter(|a| a.released && a.elapsed_ms.saturating_sub(a.impact_ms) < 180)
            .map(|a| 1. - a.elapsed_ms.saturating_sub(a.impact_ms) as f32 / 180.);
        *visible = if cast.is_some() || burst.is_some() {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if let Some(cast) = cast {
            let progress =
                (1. - cast.remaining_ms as f32 / cast.duration_ms.max(1) as f32).clamp(0., 1.);
            transform.scale =
                Vec3::splat(0.6 + 1.3 * progress + 0.08 * (time.elapsed_secs() * 18.).sin());
        } else if let Some(burst) = burst {
            transform.scale = Vec3::splat(0.4 + 2.1 * burst);
        }
    }
}

/// Restore the upright model before the ordinary weapon IK sees a respawned actor.
pub fn reset_pose(
    mut rigs: Query<(&EnemyModel, &mut EnemyRig)>,
    views: Query<&EnemyView>,
    mut transforms: Query<&mut Transform>,
) {
    for (model, mut rig) in &mut rigs {
        if views.get(model.owner).is_ok_and(|v| v.hp > 0.)
            && let Some(fall) = rig.fall.take()
        {
            fall.restore_shape(model.shape, &mut transforms);
        }
    }
}

/// Authored attack/collapse poses run after animation, body scales and ordinary weapon grips.
pub fn pose(
    mut rigs: Query<(&EnemyModel, &mut EnemyRig)>,
    views: Query<&EnemyView>,
    world: Res<Terrain>,
    time: Res<Time>,
    mut transforms: Query<&mut Transform>,
    parents: Query<&ChildOf>,
) {
    for (model, mut rig) in &mut rigs {
        let Ok(view) = views.get(model.owner) else {
            continue;
        };
        if let Some(age) = view.motion.death_age(time.elapsed_secs()) {
            if age >= motion::CORPSE_SECONDS {
                continue;
            }
            if rig.fall.is_none() {
                rig.fall = motion::Fall::capture(
                    &rig.bones,
                    model.shape,
                    model.owner,
                    &transforms,
                    &parents,
                    &world.0,
                );
            }
            if let Some(fall) = &rig.fall {
                fall.apply(
                    model.shape,
                    model.owner,
                    rig.chest,
                    age,
                    &mut transforms,
                    &parents,
                    &world.0,
                );
                // Let the arms and held objects settle with the torso, instead of
                // remaining in a raised attack/casting pose on a motionless corpse.
                let weight = (age / 0.65).clamp(0., 1.);
                if let Some(chest) = crate::weapon_pose::global(rig.chest, &transforms, &parents) {
                    place_hand(
                        rig.right,
                        chest,
                        motion::Hand {
                            position: Vec3::new(-0.32, -0.43, 0.12),
                            rotation: Quat::from_rotation_z(-0.7),
                        },
                        weight,
                        -1.,
                        &mut transforms,
                        &parents,
                    );
                    place_hand(
                        rig.left,
                        chest,
                        motion::Hand {
                            position: Vec3::new(0.35, -0.38, 0.10),
                            rotation: Quat::from_rotation_z(0.8),
                        },
                        weight,
                        1.,
                        &mut transforms,
                        &parents,
                    );
                }
            }
            continue;
        }
        let stroke = view.motion.stroke(model.kind, time.elapsed_secs());
        if let Some(stroke) = stroke
            && let Ok(mut chest) = transforms.get_mut(rig.chest)
        {
            chest.rotation *= Quat::IDENTITY.slerp(stroke.chest, stroke.weight);
        }
        let remaining = (view.hit_until - time.elapsed_secs()).max(0.);
        if remaining > 0.
            && let Ok(mut chest) = transforms.get_mut(rig.chest)
        {
            chest.rotation *=
                Quat::from_rotation_x(-0.12 * (remaining / 0.22 * std::f32::consts::PI).sin());
        }
        let Some(chest) = crate::weapon_pose::global(rig.chest, &transforms, &parents) else {
            continue;
        };
        if model.kind == Kind::Marauder {
            place_hand(
                rig.left,
                chest,
                motion::Hand {
                    position: Vec3::new(0.29, -0.18, 0.32),
                    rotation: Quat::IDENTITY,
                },
                1.,
                1.,
                &mut transforms,
                &parents,
            );
        }
        if let Some(stroke) = stroke {
            let left = model.kind == Kind::Apostate;
            place_hand(
                if left { rig.left } else { rig.right },
                chest,
                stroke.hand,
                stroke.weight,
                if left { 1. } else { -1. },
                &mut transforms,
                &parents,
            );
        }
    }
}

fn place_hand(
    arm: [Entity; 3],
    chest: Mat4,
    hand: motion::Hand,
    weight: f32,
    side: f32,
    transforms: &mut Query<&mut Transform>,
    parents: &Query<&ChildOf>,
) {
    let (_, rotation, _) = chest.to_scale_rotation_translation();
    let Some(current) = crate::weapon_pose::global(arm[2], transforms, parents) else {
        return;
    };
    let (_, current_rotation, current_position) = current.to_scale_rotation_translation();
    let orientation =
        rotation * hand.rotation * crate::appearance::weapon_socket().rotation.inverse();
    crate::weapon_pose::solve(
        arm,
        current_position.lerp(chest.transform_point3(hand.position), weight),
        current_rotation.slerp(orientation, weight),
        chest.transform_vector3(Vec3::new(side, -0.5, -0.1)),
        transforms,
        parents,
    );
}
