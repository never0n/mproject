//! Fitted costume shells retain the shipped character's skinning and attribution.
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use mortown_protocol::monsters::Kind;

pub fn clothing(source: &Mesh, kind: Kind) -> Option<Mesh> {
    let VertexAttributeValues::Float32x3(positions) = source.attribute(Mesh::ATTRIBUTE_POSITION)?
    else {
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
    let cloth = match kind {
        Kind::Bandit => [0.27, 0.22, 0.17, 1.],
        Kind::Marauder => [0.29, 0.32, 0.35, 1.],
        Kind::Apostate => [0.27, 0.24, 0.34, 1.],
    };
    let colors: Vec<_> = positions
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let skin = joints[i]
                .iter()
                .zip(weights[i])
                .filter(|(j, _)| matches!(j, 20..=23 | 27..=30))
                .map(|(_, w)| w)
                .sum::<f32>();
            if skin > 0.55 {
                [1.; 4]
            } else if p[1] < 0.52 {
                [0.24, 0.20, 0.17, 1.]
            } else {
                cloth
            }
        })
        .collect();
    let mut mesh = source.clone();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    Some(mesh)
}

pub fn headgear(source: &Mesh, kind: Kind) -> Option<Mesh> {
    let VertexAttributeValues::Float32x3(positions) = source.attribute(Mesh::ATTRIBUTE_POSITION)?
    else {
        return None;
    };
    let VertexAttributeValues::Float32x3(normals) = source.attribute(Mesh::ATTRIBUTE_NORMAL)?
    else {
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
    let selected = |i: usize| {
        let [x, y, z] = positions[i];
        let head = joints[i]
            .iter()
            .zip(weights[i])
            .filter(|(j, _)| matches!(j, 14..=16))
            .map(|(_, w)| w)
            .sum::<f32>()
            > 0.6;
        let shoulder = joints[i]
            .iter()
            .zip(weights[i])
            .filter(|(j, _)| matches!(j, 17..=18 | 24..=25))
            .map(|(_, w)| w)
            .sum::<f32>()
            > 0.6;
        match kind {
            Kind::Bandit => head && y > 1.68 && y < 1.795 && z > 0.025,
            Kind::Marauder => {
                (head && (y > 1.835 || (y > 1.735 && z < 0.075)))
                    || (shoulder && y > 1.54 && x.abs() < 0.38)
            }
            Kind::Apostate => {
                head && y > 1.66 && (y > 1.835 || z < 0.08 || (x.abs() > 0.075 && z < 0.13))
            }
        }
    };
    let indices: Vec<_> = source
        .indices()?
        .iter()
        .collect::<Vec<_>>()
        .chunks_exact(3)
        .filter(|t| t.iter().all(|&i| selected(i)))
        .flat_map(|t| t.iter().map(|&i| i as u32))
        .collect();
    if indices.is_empty() {
        return None;
    }
    let vertices: Vec<_> = positions
        .iter()
        .zip(normals)
        .map(|(&p, &n)| {
            let mut p = Vec3::from_array(p);
            p += Vec3::from_array(n)
                * match kind {
                    Kind::Bandit => 0.013,
                    Kind::Marauder => 0.025,
                    Kind::Apostate => 0.045,
                };
            if kind == Kind::Apostate && p.y > 1.84 {
                // A cloth crown, with room above the scalp and a slightly pointed back.
                p.y += ((p.y - 1.84) * 0.7).min(0.08);
                p.z -= (p.y - 1.84).max(0.) * 0.25;
            }
            p.to_array()
        })
        .collect();
    let mut mesh = source.clone();
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, vertices);
    mesh.insert_indices(Indices::U32(indices));
    // These shells use fabric/metal materials, not a copy of the face atlas.
    mesh.remove_attribute(Mesh::ATTRIBUTE_TANGENT);
    Some(mesh.with_computed_smooth_normals())
}

/// Open, folded panels instead of a solid robe around moving legs.
/// Coordinates and joint indices are the rest space of adventurer.glb.
pub fn cloth_panels(kind: Kind) -> Mesh {
    const COLS: u32 = 8;
    const ROWS: u32 = 10;
    let mut positions = Vec::new();
    let mut joints = Vec::new();
    let mut weights = Vec::new();
    let mut colors = Vec::new();
    let mut indices = Vec::new();
    let robe = kind == Kind::Apostate;
    let panels = if robe { 4 } else { 1 };
    for panel in 0..panels {
        let base = positions.len() as u32;
        for row in 0..=ROWS {
            let t = row as f32 / ROWS as f32;
            for col in 0..=COLS {
                let u = col as f32 / COLS as f32;
                let fold = (u * std::f32::consts::TAU * 2.).cos() * 0.012;
                let (x, y, z, bone, weight) = if robe {
                    let side = if panel % 2 == 0 { -1. } else { 1. };
                    let front = if panel < 2 { 1. } else { -1. };
                    (
                        side * (0.035 + u * (0.20 + 0.075 * t)),
                        1.19 - 0.81 * t + 0.015 * (u * 5.).sin() * t,
                        front * (0.175 + 0.065 * t + fold - u * u * 0.07),
                        // LeftUpLeg lies on +X in this rig's rest pose.
                        if side > 0. { 2 } else { 7 },
                        t * 0.80,
                    )
                } else {
                    (
                        (u - 0.5) * (0.40 + 0.10 * t),
                        1.62 - 0.56 * t + 0.022 * (u * 6.).sin() * t,
                        -0.145 - 0.09 * t + fold,
                        13,
                        1. - t,
                    )
                };
                positions.push([x, y, z]);
                joints.push([0u16, bone, 0, 0]);
                weights.push([1. - weight, weight, 0., 0.]);
                let trim = col == 0 || col == COLS || row == ROWS;
                colors.push(if trim {
                    [0.43, 0.34, 0.20, 1.]
                } else if robe {
                    [0.17, 0.12, 0.22, 1.]
                } else {
                    [0.20, 0.15, 0.10, 1.]
                });
            }
        }
        for row in 0..ROWS {
            for col in 0..COLS {
                let a = base + row * (COLS + 1) + col;
                let b = a + COLS + 1;
                indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_JOINT_INDEX,
        VertexAttributeValues::Uint16x4(joints),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, weights)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_indices(Indices::U32(indices))
    .with_computed_smooth_normals()
}

/// Convex kite shield, with a raised central ridge and a separate narrow rim.
pub fn shield(rim: bool) -> Mesh {
    let outline = [
        (-0.28, 0.27),
        (-0.23, 0.43),
        (0., 0.48),
        (0.23, 0.43),
        (0.28, 0.27),
        (0.19, -0.17),
        (0., -0.49),
        (-0.19, -0.17),
    ];
    let mut positions = Vec::new();
    let mut indices = Vec::new();
    if rim {
        for &(x, y) in &outline {
            positions.push([x, y, 0.036]);
            positions.push([x * 0.87, y * 0.93, 0.044]);
        }
        for i in 0..8u32 {
            let a = i * 2;
            let b = ((i + 1) % 8) * 2;
            indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    } else {
        positions.push([0., 0., 0.085]);
        positions.push([0., 0., 0.]);
        for &(x, y) in &outline {
            positions.push([x, y, 0.025]);
        }
        for i in 0..8u32 {
            let a = i + 2;
            let b = (i + 1) % 8 + 2;
            indices.extend_from_slice(&[0, b, a, 1, a, b]);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_indices(Indices::U32(indices))
    .with_duplicated_vertices()
    .with_computed_flat_normals()
}
