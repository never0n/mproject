//! Visual race/gender profiles for the current adventurer rig. No gameplay dimensions change.
use crate::{
    appearance::{EquipmentRig, Wearer},
    lobby::Form,
    network::Network,
};
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::{NoFrustumCulling, RenderLayers},
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use mortown_protocol::BodyAppearance;

#[derive(Clone, Copy)]
struct Profile {
    height: f32,
    width: f32,
    depth: f32,
    chest: f32,
    head: Vec3,
    tint: Color,
}
fn profile(look: BodyAppearance) -> Profile {
    let (height, width, depth, head, tint) = match look.race {
        2 => (1.055, 0.88, 0.92, 0.94, Color::srgb(1., 0.92, 0.77)),
        3 => (1.01, 0.91, 0.93, 0.95, Color::srgb(0.85, 0.94, 0.79)),
        4 => (1.095, 1.14, 1.13, 1.02, Color::srgb(0.62, 0.83, 0.49)),
        5 => (0.775, 1.43, 1.25, 1.22, Color::srgb(1., 0.94, 0.88)),
        6 => (1.04, 1.02, 0.95, 1.0, Color::srgb(0.79, 0.74, 0.94)),
        _ => (1., 1., 1., 1., Color::WHITE),
    };
    let female = look.gender == 2;
    Profile {
        height: height * if female { 0.95 } else { 1. },
        width: width * if female { 0.94 } else { 1. },
        depth: depth * if female { 0.95 } else { 1. },
        chest: if female { 0.94 } else { 1. },
        head: Vec3::new(head * if female { 0.96 } else { 1. }, head, head),
        tint,
    }
}
#[derive(Component)]
pub struct BodyRig {
    head: Entity,
    face: Entity,
    hair: Entity,
    bones: Vec<(Entity, Vec3, Bone)>,
    applied: Option<BodyAppearance>,
    ornaments: Option<Entity>,
    layer: RenderLayers,
}
#[derive(Clone, Copy)]
enum Bone {
    Hips,
    Spine,
    Head,
}
/// Absolute rest scale avoids accumulating proportions between animation frames.
#[derive(Component)]
pub struct BodyScale(Vec3);
#[derive(Clone)]
struct Feature {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    transform: Transform,
}
struct BodyMeshes {
    clothed: Handle<Mesh>,
    face: Handle<Mesh>,
    hair: Handle<Mesh>,
    faces: Vec<[Handle<StandardMaterial>; 2]>,
    hair_material: Handle<StandardMaterial>,
}
#[derive(Resource)]
pub struct BodyAssets {
    female_texture: Handle<Image>,
    meshes: Option<BodyMeshes>,
    features: Vec<Vec<Feature>>,
}
pub fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let ivory = materials.add(StandardMaterial {
        base_color: Color::srgb(0.82, 0.75, 0.57),
        perceptual_roughness: 0.64,
        ..default()
    });
    let mut features = Vec::new();
    for race in 1..=6 {
        let mut parts = Vec::new();
        if matches!(race, 2 | 3) {
            let tint = profile(BodyAppearance { race, gender: 1 }).tint.to_srgba();
            let skin = materials.add(StandardMaterial {
                base_color: Color::srgb(0.69 * tint.red, 0.49 * tint.green, 0.36 * tint.blue),
                perceptual_roughness: 0.85,
                cull_mode: None,
                ..default()
            });
            for sign in [-1., 1.] {
                parts.push(Feature {
                    mesh: meshes.add(ear(sign, race == 2)),
                    material: skin.clone(),
                    transform: Transform::default(),
                });
            }
        }
        if race == 4 {
            for sign in [-1., 1.] {
                parts.push(Feature {
                    mesh: meshes.add(Cone::new(0.012, 0.046).mesh().resolution(10).build()),
                    material: ivory.clone(),
                    transform: Transform::from_xyz(sign * 0.039, 0.036, 0.145)
                        .with_rotation(Quat::from_rotation_z(-sign * 0.18)),
                });
            }
        }
        if race == 6 {
            for (x, length) in [(-0.062, 0.040), (-0.092, 0.025)] {
                parts.push(Feature {
                    mesh: meshes.add(Cone::new(0.014, length).mesh().resolution(8).build()),
                    material: ivory.clone(),
                    transform: Transform::from_xyz(x, 0.17, 0.08)
                        .with_rotation(Quat::from_rotation_z(0.34)),
                });
            }
        }
        features.push(parts);
    }
    commands.insert_resource(BodyAssets {
        female_texture: assets.load("characters/adventurer-female-v1.png"),
        meshes: None,
        features,
    });
}
fn ear(sign: f32, golden: bool) -> Mesh {
    let tip = if golden {
        [0.195, 0.185, 0.01]
    } else {
        [0.181, 0.143, -0.004]
    };
    let points = [
        [0.087, 0.097, 0.025],
        [0.097, 0.043, 0.018],
        tip,
        [0.119, 0.091, 0.043],
        [0.117, 0.090, -0.002],
    ];
    let mut positions = Vec::<[f32; 3]>::new();
    for triangle in [
        [0, 1, 3],
        [1, 2, 3],
        [2, 0, 3],
        [1, 0, 4],
        [2, 1, 4],
        [0, 2, 4],
    ] {
        for i in triangle {
            let [x, y, z] = points[i];
            positions.push([x * sign, y, z]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_computed_flat_normals()
}
fn split_body(source: &Mesh) -> Option<(Mesh, Mesh, Mesh)> {
    let VertexAttributeValues::Uint16x4(joints) = source.attribute(Mesh::ATTRIBUTE_JOINT_INDEX)?
    else {
        return None;
    };
    let VertexAttributeValues::Float32x4(weights) =
        source.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)?
    else {
        return None;
    };
    let VertexAttributeValues::Float32x3(positions) = source.attribute(Mesh::ATTRIBUTE_POSITION)?
    else {
        return None;
    };
    let VertexAttributeValues::Float32x3(normals) = source.attribute(Mesh::ATTRIBUTE_NORMAL)?
    else {
        return None;
    };
    // Neck, Neck1 and Head in adventurer.glb's skin (not its node indices).
    let head = |i: usize| {
        joints[i]
            .iter()
            .zip(weights[i])
            .filter(|(j, _)| matches!(j, 14..=16))
            .map(|(_, w)| w)
            .sum::<f32>()
            > 0.5
    };
    let cap =
        |i: usize| positions[i][1] > 1.855 || (positions[i][2] < -0.026 && positions[i][1] > 1.76);
    let mut body_indices = Vec::new();
    let mut face_indices = Vec::new();
    let mut hair_indices = Vec::new();
    let indices: Vec<_> = source.indices()?.iter().collect();
    for triangle in indices.chunks_exact(3) {
        if triangle.iter().all(|&i| head(i)) {
            face_indices.extend(triangle.iter().map(|&i| i as u32));
            if triangle.iter().all(|&i| cap(i)) {
                hair_indices.extend(triangle.iter().map(|&i| i as u32));
            }
        } else {
            body_indices.extend(triangle.iter().map(|&i| i as u32));
        }
    }
    if face_indices.is_empty() || hair_indices.is_empty() {
        return None;
    }
    let mut body = source.clone();
    body.insert_indices(Indices::U32(body_indices));
    let mut face = source.clone();
    face.insert_indices(Indices::U32(face_indices));
    let mut hair = source.clone();
    hair.insert_indices(Indices::U32(hair_indices));
    hair.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        positions
            .iter()
            .zip(normals)
            .map(|(p, n)| (Vec3::from_array(*p) + Vec3::from_array(*n) * 0.004).to_array())
            .collect::<Vec<_>>(),
    );
    Some((body, face, hair))
}
#[allow(clippy::type_complexity)]
pub fn prepare(
    mut commands: Commands,
    mut assets: ResMut<BodyAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    roots: Query<(Entity, &EquipmentRig), Without<BodyRig>>,
    children: Query<&Children>,
    names: Query<(&Name, &Transform)>,
    bodies: Query<(&Mesh3d, &MeshMaterial3d<StandardMaterial>)>,
) {
    for (root, gear) in &roots {
        let mut bones = Vec::new();
        let mut head = None;
        for entity in children.iter_descendants(root) {
            let Ok((name, transform)) = names.get(entity) else {
                continue;
            };
            let bone = match name.as_str() {
                "Hips" => Bone::Hips,
                "Spine" => Bone::Spine,
                "Head" => {
                    head = Some(entity);
                    Bone::Head
                }
                _ => continue,
            };
            bones.push((entity, transform.scale, bone));
        }
        let Some(head) = head else { continue };
        if assets.meshes.is_none() {
            let Ok((source, material)) = bodies.get(gear.body) else {
                continue;
            };
            let Some(source) = meshes.get(&source.0) else {
                continue;
            };
            let Some((clothed, face, hair)) = split_body(source) else {
                continue;
            };
            let Some(original) = materials.get(&material.0).cloned() else {
                continue;
            };
            let faces = (1..=6)
                .map(|race| {
                    std::array::from_fn(|gender| {
                        let mut material = original.clone();
                        material.base_color = profile(BodyAppearance {
                            race,
                            gender: gender as u8 + 1,
                        })
                        .tint;
                        material.metallic = 0.;
                        material.perceptual_roughness = 0.86;
                        if gender == 1 {
                            material.base_color_texture = Some(assets.female_texture.clone());
                            // The original normal map contains beard relief as well as skin detail.
                            material.normal_map_texture = None;
                        }
                        materials.add(material)
                    })
                })
                .collect();
            assets.meshes = Some(BodyMeshes {
                clothed: meshes.add(clothed),
                face: meshes.add(face),
                hair: meshes.add(hair),
                faces,
                hair_material: materials.add(StandardMaterial {
                    base_color: Color::srgb(0.070, 0.033, 0.018),
                    perceptual_roughness: 0.73,
                    cull_mode: None,
                    ..default()
                }),
            });
        }
        let model = assets.meshes.as_ref().unwrap();
        commands
            .entity(gear.body)
            .insert(Mesh3d(model.clothed.clone()));
        let face = commands
            .spawn((
                Name::new("Character face"),
                Mesh3d(model.face.clone()),
                MeshMaterial3d(model.faces[0][0].clone()),
                Transform::default(),
                gear.skin.clone(),
                NoFrustumCulling,
                gear.layer.clone(),
                ChildOf(gear.body),
            ))
            .id();
        let hair = commands
            .spawn((
                Name::new("Short swept hair"),
                Mesh3d(model.hair.clone()),
                MeshMaterial3d(model.hair_material.clone()),
                Transform::default(),
                Visibility::Hidden,
                gear.skin.clone(),
                NoFrustumCulling,
                gear.layer.clone(),
                ChildOf(gear.body),
            ))
            .id();
        commands.entity(root).insert(BodyRig {
            head,
            face,
            hair,
            bones,
            applied: None,
            ornaments: None,
            layer: gear.layer.clone(),
        });
    }
}
fn desired(wearer: &Wearer, network: &Network, form: &Form) -> BodyAppearance {
    let selected = match wearer {
        Wearer::Preview => form
            .model_preview(network)
            .map(|p| p.0)
            .or_else(|| network.character.as_ref().map(BodyAppearance::from)),
        Wearer::Local => network.character.as_ref().map(BodyAppearance::from),
        Wearer::Enemy(_) => Some(BodyAppearance { race: 1, gender: 1 }),
        Wearer::Npc(id) => mortown_world::npcs::definition(*id).map(|npc| BodyAppearance {
            race: npc.race,
            gender: npc.gender,
        }),
        Wearer::Remote(id) => network
            .roster
            .iter()
            .find(|a| a.id == *id)
            .map(|a| a.appearance),
    }
    .unwrap_or_default();
    BodyAppearance {
        race: if (1..=6).contains(&selected.race) {
            selected.race
        } else {
            1
        },
        gender: if selected.gender == 2 { 2 } else { 1 },
    }
}
pub fn display(
    mut commands: Commands,
    network: Res<Network>,
    form: Res<Form>,
    assets: Res<BodyAssets>,
    mut roots: Query<(Entity, &Wearer, &mut BodyRig)>,
) {
    let Some(meshes) = &assets.meshes else { return };
    for (root, wearer, mut rig) in &mut roots {
        let look = desired(wearer, &network, &form);
        if rig.applied == Some(look) {
            continue;
        }
        let p = profile(look);
        commands
            .entity(root)
            .insert(BodyScale(Vec3::splat(p.height)));
        for &(entity, rest, bone) in &rig.bones {
            let scale = match bone {
                Bone::Hips => Vec3::new(p.width, 1., p.depth),
                Bone::Spine => Vec3::new(p.chest, 1., 1.),
                Bone::Head => p.head / Vec3::new(p.width * p.chest, 1., p.depth),
            };
            commands.entity(entity).insert(BodyScale(rest * scale));
        }
        commands.entity(rig.face).insert(MeshMaterial3d(
            meshes.faces[look.race as usize - 1][look.gender as usize - 1].clone(),
        ));
        commands.entity(rig.hair).insert(if look.gender == 2 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
        if let Some(entity) = rig.ornaments.take() {
            commands.entity(entity).despawn();
        }
        let features = &assets.features[look.race as usize - 1];
        if !features.is_empty() {
            let feature_root = commands
                .spawn((
                    Name::new("Ancestry features"),
                    Transform::default(),
                    Visibility::Inherited,
                    ChildOf(rig.head),
                ))
                .id();
            for part in features {
                commands.spawn((
                    Mesh3d(part.mesh.clone()),
                    MeshMaterial3d(part.material.clone()),
                    part.transform,
                    rig.layer.clone(),
                    ChildOf(feature_root),
                ));
            }
            rig.ornaments = Some(feature_root);
        }
        rig.applied = Some(look);
    }
}
/// Runs after animation evaluation and before transform propagation, including idle poses.
pub fn pose(mut bones: Query<(&BodyScale, &mut Transform)>) {
    for (scale, mut transform) in &mut bones {
        if transform.scale != scale.0 {
            transform.scale = scale.0;
        }
    }
}
#[allow(clippy::type_complexity)]
pub fn frame_preview(
    roots: Query<(&Wearer, &BodyRig, &EquipmentRig)>,
    globals: Query<&GlobalTransform>,
    mut cameras: Query<
        (&mut Transform, Has<crate::inventory::PortraitCamera>),
        Or<(
            With<crate::inventory::PreviewCamera>,
            With<crate::inventory::PortraitCamera>,
        )>,
    >,
) {
    let Some((_, rig, equipment)) = roots
        .iter()
        .find(|(wearer, _, _)| matches!(wearer, Wearer::Preview))
    else {
        return;
    };
    let Some(look) = rig.applied else { return };
    let p = profile(look);
    let Ok(head) = globals.get(rig.head) else {
        return;
    };
    for (mut camera, portrait) in &mut cameras {
        *camera = if portrait {
            let target = head.translation() + Vec3::Y * (0.075 * p.height * p.head.y);
            Transform::from_translation(target + Vec3::Z * (0.69 * p.height * p.head.y))
                .looking_at(target, Vec3::Y)
        } else {
            let (zoom, center) = match equipment.weapon_kind() {
                Some(mortown_protocol::weapons::PIKE) => (1.36, 1.35),
                Some(
                    mortown_protocol::weapons::GREATSWORD
                    | mortown_protocol::weapons::GREATAXE
                    | mortown_protocol::weapons::GREATCLUB,
                ) => (1.16, 1.10),
                _ => (1., 0.95),
            };
            let distance = 3.1 * p.height * p.width.max(zoom);
            Transform::from_xyz(0., p.height, distance)
                .looking_at(Vec3::Y * (center * p.height), Vec3::Y)
        };
    }
}
