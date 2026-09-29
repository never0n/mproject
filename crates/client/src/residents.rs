//! Fixed residents: fitted costumes, small work tools and local ambient work loops.
mod model;
mod motion;
use crate::{
    Terrain,
    ancestry::BodyRig,
    appearance::{EquipmentRig, Wearer},
    weapon_pose::GripRig,
};
use bevy::{camera::visibility::NoFrustumCulling, prelude::*};
use mortown_world::npcs::{Npc, Role};
pub use motion::pose;
use std::collections::BTreeMap;

#[derive(Component)]
pub struct Desk(pub u8);
struct Costume {
    body: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    garment: Handle<Mesh>,
}
#[derive(Clone)]
struct Part {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    transform: Transform,
}
#[derive(Resource)]
pub struct ResidentAssets {
    costumes: BTreeMap<u8, Costume>,
    fabric: Handle<StandardMaterial>,
    tools: BTreeMap<u8, Vec<Part>>,
    desk: BTreeMap<u8, Vec<Part>>,
}
#[derive(Component)]
pub struct WorkRig {
    npc: u8,
    head: Entity,
    back: Entity,
    tools: [Entity; 2],
    clock: f32,
    attention: f32,
}

fn part(
    meshes: &mut Assets<Mesh>,
    material: &Handle<StandardMaterial>,
    shape: impl Into<Mesh>,
    p: [f32; 3],
) -> Part {
    Part {
        mesh: meshes.add(shape),
        material: material.clone(),
        transform: Transform::from_translation(Vec3::from_array(p)),
    }
}
fn cuboid(
    meshes: &mut Assets<Mesh>,
    material: &Handle<StandardMaterial>,
    size: [f32; 3],
    p: [f32; 3],
) -> Part {
    part(meshes, material, Cuboid::new(size[0], size[1], size[2]), p)
}
pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut material = |rgb: [f32; 3], metallic| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(rgb[0], rgb[1], rgb[2]),
            metallic,
            perceptual_roughness: if metallic > 0. { 0.45 } else { 0.86 },
            ..default()
        })
    };
    let iron = material([0.31, 0.35, 0.39], 0.72);
    let oak = material([0.34, 0.19, 0.09], 0.);
    let parchment = material([0.78, 0.71, 0.51], 0.);
    let ink = material([0.075, 0.09, 0.12], 0.);
    let gold = material([0.62, 0.43, 0.17], 0.73);
    let wax = material([0.46, 0.10, 0.08], 0.);
    let cloth = material([0.29, 0.22, 0.13], 0.);
    let rune = materials.add(StandardMaterial {
        base_color: Color::srgb(0.23, 0.58, 0.69),
        emissive: LinearRgba::rgb(0.07, 0.25, 0.34),
        ..default()
    });
    let fabric = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        cull_mode: None,
        double_sided: true,
        perceptual_roughness: 0.93,
        ..default()
    });
    let mut tools = BTreeMap::new();
    // Tool frame: palm at origin, +Y along the handle. Key poses use the real tip.
    tools.insert(
        0,
        vec![
            cuboid(&mut meshes, &oak, [0.029, 0.32, 0.029], [0., 0.065, 0.]),
            cuboid(&mut meshes, &iron, [0.10, 0.075, 0.105], [0., 0.22, 0.]),
        ],
    );
    let mut tongs = vec![];
    for side in [-1., 1.] {
        tongs.push(cuboid(
            &mut meshes,
            &iron,
            [0.012, 0.30, 0.012],
            [side * 0.015, 0.07, 0.],
        ));
    }
    tongs.push(cuboid(
        &mut meshes,
        &iron,
        [0.04, 0.018, 0.018],
        [0., 0.075, 0.],
    ));
    tools.insert(1, tongs);
    tools.insert(
        2,
        vec![
            cuboid(&mut meshes, &oak, [0.017, 0.10, 0.017], [0., 0.035, 0.]),
            part(
                &mut meshes,
                &iron,
                Cylinder::new(0.004, 0.07).mesh().resolution(6).build(),
                [0., -0.05, 0.],
            ),
        ],
    );
    tools.insert(
        3,
        vec![
            part(
                &mut meshes,
                &oak,
                Cylinder::new(0.004, 0.26).mesh().resolution(6).build(),
                [0., 0., 0.],
            ),
            Part {
                transform: Transform::from_xyz(0.017, 0.11, 0.)
                    .with_scale(Vec3::new(0.032, 0.12, 0.008)),
                ..part(
                    &mut meshes,
                    &parchment,
                    Sphere::new(1.).mesh().uv(8, 6),
                    [0.; 3],
                )
            },
        ],
    );
    tools.insert(
        4,
        vec![part(
            &mut meshes,
            &gold,
            Cylinder::new(0.022, 0.004).mesh().resolution(12).build(),
            [0.; 3],
        )],
    );
    tools.insert(
        5,
        vec![
            cuboid(&mut meshes, &parchment, [0.13, 0.005, 0.085], [0., 0., 0.]),
            part(
                &mut meshes,
                &wax,
                Cylinder::new(0.013, 0.004).mesh().resolution(8).build(),
                [0., 0.005, 0.],
            ),
        ],
    );
    tools.insert(
        6,
        vec![
            cuboid(&mut meshes, &oak, [0.035, 0.065, 0.035], [0., 0.01, 0.]),
            part(
                &mut meshes,
                &gold,
                Cylinder::new(0.027, 0.02).mesh().resolution(10).build(),
                [0., -0.03, 0.],
            ),
        ],
    );
    let mut desks = BTreeMap::new();
    for npc in mortown_world::npcs::RESIDENTS {
        let mut parts = vec![];
        match npc.role {
            Role::Smith => {
                parts.push(cuboid(
                    &mut meshes,
                    &iron,
                    [0.11, 0.01, 0.045],
                    [0., 0., 0.],
                ));
            }
            Role::Leatherworker => {
                // Visible seams and spool on the existing hide; no new collision box.
                for row in 0..9 {
                    parts.push(cuboid(
                        &mut meshes,
                        &parchment,
                        [0.008, 0.004, 0.015],
                        [-0.13 + row as f32 * 0.028, 0.003, 0.],
                    ));
                }
                parts.push(part(
                    &mut meshes,
                    &cloth,
                    Cylinder::new(0.025, 0.06).mesh().resolution(10).build(),
                    [0.23, 0.03, 0.10],
                ));
            }
            Role::Runekeeper => {
                for (x, angle) in [(-0.055, -0.5), (0.055, 0.5)] {
                    let mut line =
                        cuboid(&mut meshes, &rune, [0.009, 0.004, 0.17], [x, 0.004, 0.015]);
                    line.transform.rotation = Quat::from_rotation_y(angle);
                    parts.push(line);
                }
                parts.push(cuboid(
                    &mut meshes,
                    &rune,
                    [0.12, 0.004, 0.009],
                    [0., 0.004, 0.015],
                ));
            }
            Role::Merchant => {
                parts.push(cuboid(
                    &mut meshes,
                    &cloth,
                    [0.54, 0.016, 0.29],
                    [0., -0.03, 0.04],
                ));
                for i in 0..5 {
                    parts.push(part(
                        &mut meshes,
                        &gold,
                        Cylinder::new(0.023, 0.009 * (i % 3 + 1) as f32)
                            .mesh()
                            .resolution(10)
                            .build(),
                        [-0.14 + i as f32 * 0.07, -0.013, 0.08],
                    ));
                }
            }
            Role::Postmaster => {
                for i in 0..3 {
                    parts.push(cuboid(
                        &mut meshes,
                        &parchment,
                        [0.13, 0.012, 0.09],
                        [-0.16 + i as f32 * 0.16, 0.006, 0.08],
                    ));
                    parts.push(part(
                        &mut meshes,
                        &wax,
                        Cylinder::new(0.014, 0.004).mesh().resolution(8).build(),
                        [-0.16 + i as f32 * 0.16, 0.015, 0.08],
                    ));
                }
            }
            Role::GuildSteward => {
                for row in 0..6 {
                    parts.push(cuboid(
                        &mut meshes,
                        &ink,
                        [0.14, 0.003, 0.002],
                        [-0.075, 0.004, -0.04 + row as f32 * 0.022],
                    ));
                }
                parts.push(part(
                    &mut meshes,
                    &ink,
                    Cylinder::new(0.026, 0.045).mesh().resolution(10).build(),
                    [0.23, 0.023, 0.12],
                ));
            }
        }
        desks.insert(npc.id, parts);
    }
    commands.insert_resource(ResidentAssets {
        costumes: BTreeMap::new(),
        fabric,
        tools,
        desk: desks,
    });
}

fn spawn_parts(commands: &mut Commands, parent: Entity, parts: &[Part]) {
    for part in parts {
        commands.spawn((
            Mesh3d(part.mesh.clone()),
            MeshMaterial3d(part.material.clone()),
            part.transform,
            ChildOf(parent),
        ));
    }
}
fn tool_indices(npc: &Npc) -> [Option<u8>; 2] {
    match npc.role {
        Role::Smith => [Some(0), Some(1)],
        Role::Leatherworker => [Some(2), None],
        Role::Runekeeper | Role::GuildSteward => [Some(3), None],
        Role::Merchant => [Some(4), None],
        Role::Postmaster => [Some(6), Some(5)],
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn prepare(
    mut commands: Commands,
    world: Res<Terrain>,
    mut assets: ResMut<ResidentAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    roots: Query<
        (Entity, &Wearer, &EquipmentRig, &GripRig),
        (
            With<BodyRig>,
            Without<WorkRig>,
            Without<crate::npcs::Spawned>,
        ),
    >,
    bodies: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>)>,
    children: Query<&Children>,
    names: Query<&Name>,
) {
    for (root, wearer, rig, grip) in &roots {
        let Wearer::Npc(id) = wearer else { continue };
        let Some(npc) = mortown_world::npcs::definition(*id) else {
            continue;
        };
        let Some(head) = children
            .iter_descendants(root)
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == "Head"))
        else {
            continue;
        };
        let Some(back) = children
            .iter_descendants(root)
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == "LowerBack"))
        else {
            continue;
        };
        if !assets.costumes.contains_key(id) {
            let Ok((body, material)) = bodies.get(rig.body) else {
                continue;
            };
            let (Some(mesh), Some(material)) = (meshes.get(&body.0), materials.get(&material.0))
            else {
                continue;
            };
            let Some(body) = model::clothing(mesh, npc) else {
                continue;
            };
            let mut material = material.clone();
            material.base_color = Color::WHITE;
            material.metallic = 0.;
            material.perceptual_roughness = 0.9;
            assets.costumes.insert(
                *id,
                Costume {
                    body: meshes.add(body),
                    material: materials.add(material),
                    garment: meshes.add(model::garments(npc)),
                },
            );
        }
        let costume = &assets.costumes[id];
        commands.entity(rig.body).insert((
            Mesh3d(costume.body.clone()),
            MeshMaterial3d(costume.material.clone()),
        ));
        commands.spawn((
            Name::new("Resident fitted garments"),
            Mesh3d(costume.garment.clone()),
            MeshMaterial3d(assets.fabric.clone()),
            Transform::default(),
            rig.skin.clone(),
            NoFrustumCulling,
            rig.layer.clone(),
            ChildOf(rig.body),
        ));
        let mut tool_entities = [Entity::PLACEHOLDER; 2];
        for (side, index) in tool_indices(npc).into_iter().enumerate() {
            let hand = if side == 0 {
                grip.right[2]
            } else {
                grip.left[2]
            };
            let tool = commands
                .spawn((
                    Name::new("Resident work tool"),
                    Transform::from_rotation(motion::tool_socket()),
                    Visibility::Inherited,
                    ChildOf(hand),
                ))
                .id();
            if let Some(index) = index {
                spawn_parts(&mut commands, tool, &assets.tools[&index]);
            }
            tool_entities[side] = tool;
        }
        let desk = commands
            .spawn((
                Desk(npc.id),
                Name::new(format!("Рабочие детали · {}", npc.name)),
                Transform::from_translation(Vec3::from_array(npc.work_surface(&world.0)))
                    .with_rotation(Quat::from_rotation_y(npc.yaw())),
                Visibility::Inherited,
            ))
            .id();
        spawn_parts(&mut commands, desk, &assets.desk[id]);
        commands.entity(root).insert(WorkRig {
            npc: *id,
            head,
            back,
            tools: tool_entities,
            clock: *id as f32 * 1.37,
            attention: 0.,
        });
    }
}
