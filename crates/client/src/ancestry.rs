//! Visual gender profiles of the human adventurer rig. No gameplay dimensions change.
use crate::{
    appearance::{EquipmentRig, Wearer},
    lobby::Form,
    network::Network,
};
use bevy::{
    camera::visibility::NoFrustumCulling,
    mesh::{Indices, VertexAttributeValues},
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
}
fn profile(look: BodyAppearance) -> Profile {
    let female = look.gender == 2;
    Profile {
        height: if female { 0.95 } else { 1. },
        width: if female { 0.94 } else { 1. },
        depth: if female { 0.95 } else { 1. },
        chest: if female { 0.94 } else { 1. },
        head: Vec3::new(if female { 0.96 } else { 1. }, 1., 1.),
    }
}
#[derive(Component)]
pub struct BodyRig {
    head: Entity,
    face: Entity,
    hair: Entity,
    bones: Vec<(Entity, Vec3, Bone)>,
    applied: Option<BodyAppearance>,
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
struct BodyMeshes {
    clothed: Handle<Mesh>,
    face: Handle<Mesh>,
    hair: Handle<Mesh>,
    faces: [Handle<StandardMaterial>; 2],
    hair_material: Handle<StandardMaterial>,
}
#[derive(Resource)]
pub struct BodyAssets {
    female_texture: Handle<Image>,
    meshes: Option<BodyMeshes>,
}
pub fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.insert_resource(BodyAssets {
        female_texture: assets.load("characters/adventurer-female-v1.png"),
        meshes: None,
    });
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
            let faces = std::array::from_fn(|gender| {
                let mut material = original.clone();
                material.base_color = Color::WHITE;
                material.metallic = 0.;
                material.perceptual_roughness = 0.86;
                if gender == 1 {
                    material.base_color_texture = Some(assets.female_texture.clone());
                    // The original normal map contains beard relief as well as skin detail.
                    material.normal_map_texture = None;
                }
                materials.add(material)
            });
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
                MeshMaterial3d(model.faces[0].clone()),
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
        Wearer::Enemy(_) => Some(BodyAppearance { gender: 1 }),
        Wearer::Remote(id) => network
            .roster
            .iter()
            .find(|a| a.id == *id)
            .map(|a| a.appearance),
    }
    .unwrap_or_default();
    BodyAppearance {
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
            meshes.faces[look.gender as usize - 1].clone(),
        ));
        commands.entity(rig.hair).insert(if look.gender == 2 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        });
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
