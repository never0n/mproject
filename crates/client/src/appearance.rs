//! Equipment uses the same skeleton in the world, portrait and inventory preview.
use crate::network::Network;
mod arsenal;
use bevy::{
    asset::RenderAssetUsages,
    camera::visibility::{NoFrustumCulling, RenderLayers},
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues, skinning::SkinnedMesh},
    prelude::*,
};
use mortown_protocol::items::EquipmentAppearance;
use std::{collections::BTreeMap, f32::consts::FRAC_PI_2};

#[derive(Component)]
pub enum Wearer {
    Local,
    Remote(i64),
    Enemy(mortown_protocol::monsters::Kind),
    Preview,
}
#[derive(Component)]
pub struct EquipmentRig {
    hand: Entity,
    left_hand: Entity,
    pub(crate) body: Entity,
    pub(crate) source: Handle<Mesh>,
    pub(crate) skin: SkinnedMesh,
    pub(crate) layer: RenderLayers,
    applied: Option<EquipmentAppearance>,
    weapon: Option<Entity>,
    armor: Option<Entity>,
}
impl EquipmentRig {
    pub(crate) fn weapon_entity(&self) -> Option<Entity> {
        self.weapon
    }
    pub(crate) fn weapon_kind(&self) -> Option<u8> {
        self.applied.and_then(|look| look.weapon)
    }
}
pub(crate) fn weapon_socket() -> Transform {
    Transform::from_xyz(0., 0.055, 0.015).with_rotation(Quat::from_rotation_x(FRAC_PI_2))
}
#[derive(Clone)]
struct Piece {
    mesh: Handle<Mesh>,
    material: Handle<StandardMaterial>,
    transform: Transform,
}
#[derive(Resource)]
pub struct EquipmentAssets {
    weapons: BTreeMap<u8, Vec<Piece>>,
    armor: Option<[(Handle<Mesh>, Handle<StandardMaterial>); 2]>,
}

pub fn setup(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut mat = |color, metallic, roughness| {
        materials.add(StandardMaterial {
            base_color: color,
            metallic,
            perceptual_roughness: roughness,
            ..default()
        })
    };
    let oak = mat(Color::srgb(0.32, 0.16, 0.065), 0., 0.82);
    let pale_wood = mat(Color::srgb(0.57, 0.35, 0.15), 0., 0.72);
    let dark_wood = mat(Color::srgb(0.12, 0.065, 0.045), 0., 0.76);
    let leather = mat(Color::srgb(0.10, 0.052, 0.032), 0., 0.90);
    let iron = mat(Color::srgb(0.29, 0.33, 0.37), 0.82, 0.43);
    let steel = mat(Color::srgb(0.62, 0.72, 0.80), 0.94, 0.25);
    let brass = mat(Color::srgb(0.60, 0.40, 0.16), 0.80, 0.36);
    let crystal = materials.add(StandardMaterial {
        base_color: Color::srgb(0.12, 0.58, 0.70),
        emissive: LinearRgba::rgb(0.12, 0.72, 1.05),
        metallic: 0.25,
        perceptual_roughness: 0.20,
        ..default()
    });
    let mut weapons = BTreeMap::new();
    for kind in [1, 4, 17] {
        let crafted = kind == 17;
        let wooden = kind == 1;
        let metal = if wooden {
            &pale_wood
        } else if crafted {
            &steel
        } else {
            &iron
        };
        let trim = if wooden {
            &oak
        } else if crafted {
            &brass
        } else {
            &iron
        };
        let length = if crafted {
            0.93
        } else if wooden {
            0.69
        } else {
            0.81
        };
        let mut parts = Vec::new();
        // A diamond-section blade with a tapered point, measured from the grip centre.
        piece(
            &mut parts,
            &mut meshes,
            blade(length, if crafted { 0.041 } else { 0.033 }),
            metal,
            Transform::default(),
        );
        piece(
            &mut parts,
            &mut meshes,
            Cylinder::new(0.021, 0.16).mesh().resolution(12).build(),
            if wooden { &oak } else { &leather },
            Transform::default(),
        );
        piece(
            &mut parts,
            &mut meshes,
            Cuboid::new(if crafted { 0.25 } else { 0.21 }, 0.031, 0.044).into(),
            trim,
            Transform::from_xyz(0., 0.10, 0.),
        );
        piece(
            &mut parts,
            &mut meshes,
            Sphere::new(0.034).mesh().uv(12, 8),
            trim,
            Transform::from_xyz(0., -0.103, 0.).with_scale(Vec3::new(0.85, 1.15, 0.70)),
        );
        for y in [-0.067, 0.069] {
            piece(
                &mut parts,
                &mut meshes,
                Cylinder::new(0.024, 0.014).mesh().resolution(12).build(),
                trim,
                Transform::from_xyz(0., y, 0.),
            );
        }
        if crafted {
            piece(
                &mut parts,
                &mut meshes,
                Sphere::new(0.015).mesh().uv(10, 6),
                &crystal,
                Transform::from_xyz(0., 0.10, 0.027).with_scale(Vec3::new(1., 1., 0.45)),
            );
        }
        weapons.insert(kind, parts);
    }
    for kind in [2, 5, 18] {
        let carved = kind != 2;
        let runic = kind == 18;
        let wood = if runic { &dark_wood } else { &oak };
        let mut parts = Vec::new();
        // The lower end is only 68 cm below the palm; the old polearm exceeded leg length.
        piece(
            &mut parts,
            &mut meshes,
            Cylinder::new(0.024, 1.48).mesh().resolution(12).build(),
            wood,
            Transform::from_xyz(0., 0.06, 0.),
        );
        piece(
            &mut parts,
            &mut meshes,
            Cylinder::new(0.029, 0.22).mesh().resolution(12).build(),
            &leather,
            Transform::default(),
        );
        piece(
            &mut parts,
            &mut meshes,
            Cylinder::new(0.030, 0.10).mesh().resolution(12).build(),
            if carved { &brass } else { &iron },
            Transform::from_xyz(0., -0.64, 0.),
        );
        for y in [-0.12, 0.12, 0.64, 0.75] {
            piece(
                &mut parts,
                &mut meshes,
                Cylinder::new(0.033, 0.024).mesh().resolution(12).build(),
                if carved { &brass } else { &pale_wood },
                Transform::from_xyz(0., y, 0.),
            );
        }
        if carved {
            for sign in [-1., 1.] {
                piece(
                    &mut parts,
                    &mut meshes,
                    Cuboid::new(0.022, 0.19, 0.033).into(),
                    &brass,
                    Transform::from_xyz(sign * 0.065, 0.84, 0.)
                        .with_rotation(Quat::from_rotation_z(-sign * 0.32)),
                );
            }
            piece(
                &mut parts,
                &mut meshes,
                Sphere::new(0.065).mesh().uv(6, 4),
                if runic { &crystal } else { &pale_wood },
                Transform::from_xyz(0., 0.88, 0.).with_scale(Vec3::new(0.8, 1.55, 0.8)),
            );
        } else {
            piece(
                &mut parts,
                &mut meshes,
                Sphere::new(0.047).mesh().uv(10, 6),
                &pale_wood,
                Transform::from_xyz(0., 0.81, 0.).with_scale(Vec3::new(1., 1.3, 1.)),
            );
        }
        if runic {
            for y in [0.30, 0.38, 0.46, 0.54] {
                piece(
                    &mut parts,
                    &mut meshes,
                    Torus::new(0.025, 0.031)
                        .mesh()
                        .major_resolution(12)
                        .minor_resolution(6)
                        .build(),
                    &crystal,
                    Transform::from_xyz(0., y, 0.),
                );
            }
        }
        weapons.insert(kind, parts);
    }
    arsenal::add(
        &mut weapons,
        &mut meshes,
        arsenal::Palette {
            wood: &oak,
            iron: &iron,
            leather: &leather,
            edge: &steel,
        },
    );
    commands.insert_resource(EquipmentAssets {
        weapons,
        armor: None,
    });
}
fn piece(
    parts: &mut Vec<Piece>,
    meshes: &mut Assets<Mesh>,
    mesh: Mesh,
    material: &Handle<StandardMaterial>,
    transform: Transform,
) {
    let mut mesh = mesh.transformed_by(transform);
    if mesh.indices().is_none() {
        mesh.insert_indices(Indices::U32((0..mesh.count_vertices() as u32).collect()));
    }
    if mesh.attribute(Mesh::ATTRIBUTE_UV_0).is_none() {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0., 0.]; mesh.count_vertices()]);
    }
    // These untextured props share one mesh per material, including staff rings.
    mesh.remove_attribute(Mesh::ATTRIBUTE_TANGENT);
    if let Some(part) = parts.iter().find(|p| p.material == *material) {
        meshes
            .get_mut(&part.mesh)
            .unwrap()
            .merge(&mesh)
            .expect("equipment primitives share a triangle layout");
        return;
    }
    parts.push(Piece {
        mesh: meshes.add(mesh),
        material: material.clone(),
        transform: Transform::default(),
    });
}
fn blade(length: f32, half_width: f32) -> Mesh {
    let ring = |y: f32, w: f32| {
        [
            Vec3::new(-w, y, 0.),
            Vec3::new(0., y, -0.009),
            Vec3::new(w, y, 0.),
            Vec3::new(0., y, 0.009),
        ]
    };
    let base = ring(0.11, half_width);
    let shoulder = ring(length - 0.12, half_width * 0.72);
    let tip = Vec3::new(0., length, 0.);
    let mut positions = Vec::<[f32; 3]>::new();
    for i in 0..4 {
        let j = (i + 1) % 4;
        for p in [
            base[i],
            base[j],
            shoulder[j],
            base[i],
            shoulder[j],
            shoulder[i],
            shoulder[i],
            shoulder[j],
            tip,
        ] {
            positions.push(p.to_array());
        }
    }
    for i in [0, 2, 1, 0, 3, 2] {
        positions.push(base[i].to_array());
    }
    for triangle in positions.chunks_exact_mut(3) {
        triangle.swap(1, 2);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_computed_flat_normals()
}

/// Fit the cuirass to the existing rest mesh and keep its original joint weights.
/// Hands, face and legs are excluded; no rigid chest primitive intersects the body.
fn cuirass(source: &Mesh, reinforced: bool) -> Option<Mesh> {
    let VertexAttributeValues::Float32x3(positions) = source.attribute(Mesh::ATTRIBUTE_POSITION)?
    else {
        return None;
    };
    let VertexAttributeValues::Float32x3(normals) = source.attribute(Mesh::ATTRIBUTE_NORMAL)?
    else {
        return None;
    };
    let VertexAttributeValues::Float32x2(uvs) = source.attribute(Mesh::ATTRIBUTE_UV_0)? else {
        return None;
    };
    let VertexAttributeValues::Uint16x4(joints) = source.attribute(Mesh::ATTRIBUTE_JOINT_INDEX)?
    else {
        return None;
    };
    let VertexAttributeValues::Float32x4(weights) =
        source.attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT)?
    else {
        return None;
    };
    // Joint indices in adventurer.glb: Hips, LowerBack, Spine, Spine1, Neck.
    let selected = |i: usize| {
        (1.13..1.66).contains(&positions[i][1])
            && joints[i]
                .iter()
                .zip(weights[i])
                .filter(|(j, _)| matches!(j, 0 | 11 | 12 | 13 | 14))
                .map(|(_, w)| w)
                .sum::<f32>()
                > 0.65
    };
    let original: Vec<_> = source.indices()?.iter().collect();
    let mut remap = BTreeMap::<usize, u32>::new();
    let mut used = Vec::new();
    let mut indices = Vec::new();
    for triangle in original
        .chunks_exact(3)
        .filter(|t| t.iter().all(|&i| selected(i)))
    {
        for &i in triangle {
            let next = remap.len() as u32;
            let index = *remap.entry(i).or_insert_with(|| {
                used.push(i);
                next
            });
            indices.push(index);
        }
    }
    if indices.is_empty() {
        return None;
    }
    let vertices: Vec<_> = used
        .iter()
        .map(|&i| {
            (Vec3::from_array(positions[i])
                + Vec3::from_array(normals[i]) * if reinforced { 0.016 } else { 0.008 })
            .to_array()
        })
        .collect();
    let colors: Vec<[f32; 4]> = used
        .iter()
        .map(|&i| {
            let [x, y, _] = positions[i];
            let trim = (y - 1.19).abs() < 0.025
                || (y - 1.57).abs() < 0.018
                || (x.abs() - 0.115).abs() < 0.014;
            if trim && reinforced {
                [0.83, 0.60, 0.27, 1.]
            } else if reinforced {
                [0.30, 0.33, 0.36, 1.]
            } else if trim {
                [0.57, 0.36, 0.17, 1.]
            } else {
                [0.39, 0.20, 0.10, 1.]
            }
        })
        .collect();
    let mut result = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_NORMAL,
        used.iter().map(|&i| normals[i]).collect::<Vec<_>>(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_UV_0,
        used.iter().map(|&i| uvs[i]).collect::<Vec<_>>(),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_JOINT_INDEX,
        VertexAttributeValues::Uint16x4(used.iter().map(|&i| joints[i]).collect()),
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_JOINT_WEIGHT,
        used.iter().map(|&i| weights[i]).collect::<Vec<_>>(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices));
    if let Some(VertexAttributeValues::Float32x4(tangents)) =
        source.attribute(Mesh::ATTRIBUTE_TANGENT)
    {
        result.insert_attribute(
            Mesh::ATTRIBUTE_TANGENT,
            used.iter().map(|&i| tangents[i]).collect::<Vec<_>>(),
        );
    }
    Some(result)
}

#[allow(clippy::type_complexity)]
pub fn prepare(
    mut commands: Commands,
    mut gear: ResMut<EquipmentAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    roots: Query<(Entity, &Wearer), Without<EquipmentRig>>,
    children: Query<&Children>,
    names: Query<&Name>,
    bodies: Query<(
        Entity,
        &Mesh3d,
        &SkinnedMesh,
        &MeshMaterial3d<StandardMaterial>,
    )>,
) {
    for (root, wearer) in &roots {
        let descendants: Vec<_> = children.iter_descendants(root).collect();
        let Some(hand) = descendants
            .iter()
            .copied()
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == "RightHand"))
        else {
            continue;
        };
        let Some(left_hand) = descendants
            .iter()
            .copied()
            .find(|&e| names.get(e).is_ok_and(|n| n.as_str() == "LeftHand"))
        else {
            continue;
        };
        let Some((body, mesh, skin, material)) =
            descendants.iter().find_map(|&e| bodies.get(e).ok())
        else {
            continue;
        };
        if gear.armor.is_none() {
            let Some(source) = meshes.get(&mesh.0) else {
                continue;
            };
            let (Some(plain), Some(reinforced)) = (cuirass(source, false), cuirass(source, true))
            else {
                continue;
            };
            let Some(base) = materials.get(&material.0).cloned() else {
                continue;
            };
            let mut leather = base.clone();
            leather.base_color = Color::WHITE;
            leather.metallic = 0.;
            leather.perceptual_roughness = 0.86;
            let mut reinforced_material = base;
            reinforced_material.base_color = Color::WHITE;
            reinforced_material.metallic = 0.35;
            reinforced_material.perceptual_roughness = 0.57;
            gear.armor = Some([
                (meshes.add(plain), materials.add(leather)),
                (meshes.add(reinforced), materials.add(reinforced_material)),
            ]);
        }
        commands.entity(root).insert(EquipmentRig {
            hand,
            left_hand,
            body,
            source: mesh.0.clone(),
            skin: skin.clone(),
            layer: RenderLayers::layer(if matches!(wearer, Wearer::Preview) {
                1
            } else {
                0
            }),
            applied: None,
            weapon: None,
            armor: None,
        });
    }
}

pub fn display(
    mut commands: Commands,
    network: Res<Network>,
    form: Res<crate::lobby::Form>,
    gear: Res<EquipmentAssets>,
    mut rigs: Query<(&Wearer, &mut EquipmentRig)>,
) {
    for (wearer, mut rig) in &mut rigs {
        let desired = match wearer {
            Wearer::Preview => form.model_preview(&network).map(|p| p.1).or_else(|| {
                network
                    .character
                    .as_ref()
                    .map(|c| EquipmentAppearance::from_items(&c.items))
            }),
            Wearer::Local => network
                .character
                .as_ref()
                .map(|c| EquipmentAppearance::from_items(&c.items)),
            Wearer::Enemy(kind) => Some(EquipmentAppearance {
                weapon: Some(match kind {
                    mortown_protocol::monsters::Kind::Bandit => 26,
                    mortown_protocol::monsters::Kind::Marauder => 19,
                    mortown_protocol::monsters::Kind::Apostate => 18,
                }),
                armor: match kind {
                    mortown_protocol::monsters::Kind::Bandit => Some(3),
                    mortown_protocol::monsters::Kind::Marauder => Some(6),
                    mortown_protocol::monsters::Kind::Apostate => None,
                },
            }),
            Wearer::Remote(id) => network
                .roster
                .iter()
                .find(|a| a.id == *id)
                .map(|a| a.equipment),
        }
        .unwrap_or_default();
        if rig.applied == Some(desired) {
            continue;
        }
        if rig.applied.is_none_or(|old| old.weapon != desired.weapon) {
            if let Some(entity) = rig.weapon.take() {
                commands.entity(entity).despawn();
            }
            if let Some(parts) = desired.weapon.and_then(|kind| gear.weapons.get(&kind)) {
                // The grip follows the animated palm; local +Z crosses the closed fingers.
                let root = commands
                    .spawn((
                        Name::new("Equipped weapon"),
                        weapon_socket(),
                        Visibility::default(),
                        ChildOf(if desired.weapon == Some(mortown_protocol::weapons::BOW) {
                            rig.left_hand
                        } else {
                            rig.hand
                        }),
                    ))
                    .id();
                for part in parts {
                    commands.spawn((
                        Mesh3d(part.mesh.clone()),
                        MeshMaterial3d(part.material.clone()),
                        part.transform,
                        rig.layer.clone(),
                        ChildOf(root),
                    ));
                }
                rig.weapon = Some(root);
            }
        }
        if rig.applied.is_none_or(|old| old.armor != desired.armor) {
            if let Some(entity) = rig.armor.take() {
                commands.entity(entity).despawn();
            }
            let index = match desired.armor {
                Some(3) => Some(0),
                Some(6) => Some(1),
                _ => None,
            };
            if let Some((mesh, material)) = index.and_then(|i| gear.armor.as_ref().map(|a| &a[i])) {
                rig.armor = Some(
                    commands
                        .spawn((
                            Name::new("Equipped cuirass"),
                            Mesh3d(mesh.clone()),
                            MeshMaterial3d(material.clone()),
                            Transform::default(),
                            rig.skin.clone(),
                            NoFrustumCulling,
                            rig.layer.clone(),
                            ChildOf(rig.body),
                        ))
                        .id(),
                );
            }
        }
        rig.applied = Some(desired);
    }
}
