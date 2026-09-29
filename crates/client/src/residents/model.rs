//! Clothing in Defender rest space; skin weights retain the existing body proportions.
use bevy::{
    asset::RenderAssetUsages,
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    prelude::*,
};
use mortown_world::npcs::{Npc, Role};

fn palette(npc: &Npc) -> ([f32; 4], [f32; 4]) {
    match npc.role {
        Role::Merchant if npc.id == 1 => ([0.30, 0.16, 0.09, 1.], [0.58, 0.39, 0.16, 1.]),
        Role::Merchant => ([0.12, 0.29, 0.28, 1.], [0.57, 0.44, 0.24, 1.]),
        Role::Leatherworker => ([0.23, 0.29, 0.16, 1.], [0.48, 0.28, 0.12, 1.]),
        Role::Smith => ([0.25, 0.22, 0.20, 1.], [0.24, 0.12, 0.055, 1.]),
        Role::Runekeeper => ([0.16, 0.19, 0.34, 1.], [0.49, 0.53, 0.62, 1.]),
        Role::Postmaster => ([0.16, 0.23, 0.34, 1.], [0.47, 0.29, 0.13, 1.]),
        Role::GuildSteward => ([0.31, 0.14, 0.20, 1.], [0.62, 0.45, 0.20, 1.]),
    }
}

pub fn clothing(source: &Mesh, npc: &Npc) -> Option<Mesh> {
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
    let (cloth, trim) = palette(npc);
    let mut mesh = source.clone();
    let colors: Vec<_> = positions
        .iter()
        .enumerate()
        .map(|(i, &[x, y, z])| {
            let hands = joints[i]
                .iter()
                .zip(weights[i])
                .filter(|(j, _)| matches!(j, 20..=23 | 27..=30))
                .map(|(_, w)| w)
                .sum::<f32>();
            if hands > 0.55 {
                [1.; 4]
            } else if y < 0.5 {
                [0.22, 0.15, 0.10, 1.]
            } else if (1.14..1.21).contains(&y) {
                [0.23, 0.14, 0.07, 1.]
            } else if y > 1.22
                && z > 0.06
                && (x.abs() < 0.024
                    || (x - (y - 1.4) * 0.8).abs() < 0.025 && npc.role == Role::Postmaster)
            {
                trim
            } else if y < 1.14 {
                [0.27, 0.24, 0.21, 1.]
            } else {
                cloth
            }
        })
        .collect();
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    Some(mesh)
}

#[derive(Default)]
struct Cloth {
    positions: Vec<[f32; 3]>,
    joints: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
    colors: Vec<[f32; 4]>,
    indices: Vec<u32>,
}
impl Cloth {
    fn panel(
        &mut self,
        cols: u32,
        rows: u32,
        vertex: impl Fn(f32, f32) -> ([f32; 3], u16, f32),
        color: [f32; 4],
        trim: [f32; 4],
    ) {
        let base = self.positions.len() as u32;
        for row in 0..=rows {
            for col in 0..=cols {
                let (p, bone, w) = vertex(col as f32 / cols as f32, row as f32 / rows as f32);
                self.positions.push(p);
                self.joints.push([0, bone, 0, 0]);
                self.weights.push([1. - w, w, 0., 0.]);
                self.colors.push(if col == 0 || col == cols || row == rows {
                    trim
                } else {
                    color
                });
            }
        }
        for row in 0..rows {
            for col in 0..cols {
                let a = base + row * (cols + 1) + col;
                let b = a + cols + 1;
                self.indices
                    .extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
            }
        }
    }
    fn solid(&mut self, mesh: Mesh, center: Vec3, bone: u16, color: [f32; 4]) {
        let Some(VertexAttributeValues::Float32x3(points)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            return;
        };
        let Some(indices) = mesh.indices() else {
            return;
        };
        let base = self.positions.len() as u32;
        for &p in points {
            self.positions
                .push((center + Vec3::from_array(p)).to_array());
            self.joints.push([bone, 0, 0, 0]);
            self.weights.push([1., 0., 0., 0.]);
            self.colors.push(color);
        }
        self.indices.extend(indices.iter().map(|i| base + i as u32));
    }
    fn mesh(self) -> Mesh {
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, self.positions)
        .with_inserted_attribute(
            Mesh::ATTRIBUTE_JOINT_INDEX,
            VertexAttributeValues::Uint16x4(self.joints),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_JOINT_WEIGHT, self.weights)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, self.colors)
        .with_inserted_indices(Indices::U32(self.indices))
        .with_computed_smooth_normals()
    }
}

pub fn garments(npc: &Npc) -> Mesh {
    let (cloth, trim) = palette(npc);
    let mut shape = Cloth::default();
    let apron = matches!(npc.role, Role::Smith | Role::Leatherworker);
    if apron {
        // Bib, separate shoulder straps and a soft skirt, without a solid block around the legs.
        shape.panel(
            10,
            10,
            |u, t| {
                (
                    [
                        (u - 0.5) * (0.29 + 0.11 * t),
                        1.60 - 0.43 * t,
                        0.17 + 0.037 * (t * std::f32::consts::PI).sin(),
                    ],
                    13,
                    1. - t,
                )
            },
            trim,
            [0.58, 0.40, 0.23, 1.],
        );
        for sign in [-1., 1.] {
            shape.panel(
                2,
                5,
                |u, t| {
                    (
                        [sign * (0.12 + 0.025 * u), 1.60 + 0.075 * t, 0.17 - 0.04 * t],
                        13,
                        1.,
                    )
                },
                trim,
                trim,
            );
        }
    }
    let robe = npc.role == Role::Runekeeper;
    for side in [-1., 1.] {
        for front in if apron { vec![1.] } else { vec![1., -1.] } {
            shape.panel(
                8,
                12,
                |u, t| {
                    let length = if robe {
                        0.77
                    } else if apron {
                        0.48
                    } else {
                        0.29
                    };
                    let fold = (u * std::f32::consts::TAU * 2.).cos() * 0.011;
                    (
                        [
                            side * (0.012 + u * (0.235 + t * 0.055)),
                            1.19 - length * t,
                            front * (0.187 + t * 0.063 - u * u * 0.065 + fold),
                        ],
                        if side > 0. { 2 } else { 7 },
                        t * 0.72,
                    )
                },
                if apron { trim } else { cloth },
                if apron { [0.52, 0.35, 0.18, 1.] } else { trim },
            );
        }
    }
    let leather = [0.23, 0.12, 0.06, 1.];
    // A hanging satchel for the postmaster; belt pouches for artisans and merchants.
    let post = npc.role == Role::Postmaster;
    shape.solid(
        Cuboid::new(
            if post { 0.25 } else { 0.12 },
            if post { 0.29 } else { 0.15 },
            0.08,
        )
        .into(),
        Vec3::new(0.235, if post { 1.04 } else { 1.12 }, 0.15),
        0,
        leather,
    );
    shape.solid(
        Cuboid::new(0.048, 0.045, 0.015).into(),
        Vec3::new(0.235, if post { 1.09 } else { 1.145 }, 0.198),
        0,
        trim,
    );
    if !apron {
        for y in [1.27, 1.36, 1.45, 1.54] {
            shape.solid(
                Sphere::new(0.012).mesh().uv(8, 6),
                Vec3::new(0., y, 0.188),
                13,
                trim,
            );
        }
    }
    shape.mesh()
}
