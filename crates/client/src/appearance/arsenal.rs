//! Plain level-one weapons, modelled in metres around the primary grip.
use super::{Piece, blade, piece};
use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology, prelude::*};
use mortown_protocol::weapons::*;
use std::collections::BTreeMap;
pub(super) struct Palette<'a> {
    pub wood: &'a Handle<StandardMaterial>,
    pub iron: &'a Handle<StandardMaterial>,
    pub leather: &'a Handle<StandardMaterial>,
    pub edge: &'a Handle<StandardMaterial>,
}
pub(super) fn add(weapons: &mut BTreeMap<u8, Vec<Piece>>, meshes: &mut Assets<Mesh>, p: Palette) {
    for kind in [
        AXE, HAMMER, GREATSWORD, GREATAXE, GREATCLUB, PIKE, BOW, DAGGER,
    ] {
        let mut parts = Vec::new();
        match kind {
            AXE | GREATAXE => {
                let large = kind == GREATAXE;
                let top = if large { 0.91 } else { 0.49 };
                let bottom = if large { -0.27 } else { -0.11 };
                shaft(&mut parts, meshes, 0.023, bottom, top + 0.07, p.wood);
                shaft(&mut parts, meshes, 0.026, bottom + 0.035, 0.11, p.leather);
                let size = if large { 1.40 } else { 1. };
                let outline: Vec<_> = [
                    (0., -0.075),
                    (0.14, -0.20),
                    (0.24, -0.16),
                    (0.285, -0.075),
                    (0.285, 0.09),
                    (0.23, 0.16),
                    (0., 0.075),
                ]
                .into_iter()
                .map(|(x, y)| [x * size, top + y * size])
                .collect();
                piece(
                    &mut parts,
                    meshes,
                    extrude(&outline, 0.032),
                    p.iron,
                    Transform::default(),
                );
                piece(
                    &mut parts,
                    meshes,
                    extrude(
                        &[
                            [0.24 * size, top - 0.16 * size],
                            [0.285 * size, top - 0.075 * size],
                            [0.285 * size, top + 0.09 * size],
                            [0.23 * size, top + 0.16 * size],
                            [0.25 * size, top],
                        ],
                        0.008,
                    ),
                    p.edge,
                    Transform::default(),
                );
                for y in [top - 0.045, top + 0.045] {
                    band(&mut parts, meshes, 0.035, y, p.iron);
                }
            }
            HAMMER => {
                shaft(&mut parts, meshes, 0.024, -0.13, 0.49, p.wood);
                shaft(&mut parts, meshes, 0.027, -0.09, 0.11, p.leather);
                let shape = [
                    [-0.15, 0.395],
                    [-0.125, 0.37],
                    [0.125, 0.37],
                    [0.15, 0.395],
                    [0.15, 0.50],
                    [0.125, 0.525],
                    [-0.125, 0.525],
                    [-0.15, 0.50],
                ];
                piece(
                    &mut parts,
                    meshes,
                    extrude(&shape, 0.13),
                    p.iron,
                    Transform::default(),
                );
                for x in [-0.151, 0.151] {
                    piece(
                        &mut parts,
                        meshes,
                        Cuboid::new(0.008, 0.10, 0.106).into(),
                        p.edge,
                        Transform::from_xyz(x, 0.448, 0.),
                    );
                }
                band(&mut parts, meshes, 0.034, 0.33, p.iron);
            }
            GREATSWORD | DAGGER => {
                let large = kind == GREATSWORD;
                piece(
                    &mut parts,
                    meshes,
                    blade(
                        if large { 1.22 } else { 0.36 },
                        if large { 0.046 } else { 0.027 },
                    ),
                    p.edge,
                    Transform::default(),
                );
                shaft(
                    &mut parts,
                    meshes,
                    if large { 0.024 } else { 0.020 },
                    if large { -0.25 } else { -0.09 },
                    0.08,
                    p.leather,
                );
                piece(
                    &mut parts,
                    meshes,
                    Cuboid::new(if large { 0.32 } else { 0.14 }, 0.030, 0.042).into(),
                    p.iron,
                    Transform::from_xyz(0., 0.095, 0.),
                );
                piece(
                    &mut parts,
                    meshes,
                    Sphere::new(if large { 0.038 } else { 0.027 })
                        .mesh()
                        .uv(10, 6),
                    p.iron,
                    Transform::from_xyz(0., if large { -0.275 } else { -0.11 }, 0.)
                        .with_scale(Vec3::new(0.9, 1., 0.7)),
                );
            }
            GREATCLUB => {
                shaft(&mut parts, meshes, 0.032, -0.27, 0.48, p.wood);
                shaft(&mut parts, meshes, 0.035, -0.23, 0.13, p.leather);
                piece(
                    &mut parts,
                    meshes,
                    ConicalFrustum {
                        radius_top: 0.095,
                        radius_bottom: 0.042,
                        height: 0.56,
                    }
                    .mesh()
                    .resolution(10)
                    .build(),
                    p.wood,
                    Transform::from_xyz(0., 0.65, 0.),
                );
                for (radius, y) in [(0.060, 0.49), (0.093, 0.86)] {
                    band(&mut parts, meshes, radius, y, p.iron);
                }
                piece(
                    &mut parts,
                    meshes,
                    Sphere::new(0.095).mesh().uv(10, 6),
                    p.wood,
                    Transform::from_xyz(0., 0.928, 0.).with_scale(Vec3::new(1., 0.28, 1.)),
                );
            }
            PIKE => {
                shaft(&mut parts, meshes, 0.021, -0.68, 1.36, p.wood);
                shaft(&mut parts, meshes, 0.024, -0.17, 0.23, p.leather);
                shaft(&mut parts, meshes, 0.025, 1.24, 1.42, p.iron);
                piece(
                    &mut parts,
                    meshes,
                    blade(0.46, 0.038),
                    p.edge,
                    Transform::from_xyz(0., 1.27, 0.),
                );
                piece(
                    &mut parts,
                    meshes,
                    Cone::new(0.027, 0.095).mesh().resolution(10).build(),
                    p.iron,
                    Transform::from_xyz(0., -0.69, 0.)
                        .with_rotation(Quat::from_rotation_z(std::f32::consts::PI)),
                );
            }
            BOW => {
                for sign in [-1., 1.] {
                    let limb: Vec<_> = (0..=18)
                        .map(|i| {
                            let t = i as f32 / 18.;
                            Vec3::new(
                                0.,
                                sign * t * 0.69,
                                0.20 * (std::f32::consts::PI * t).sin() - 0.11 * t,
                            )
                        })
                        .collect();
                    for (i, pair) in limb.windows(2).enumerate() {
                        line(
                            &mut parts,
                            meshes,
                            pair[0],
                            pair[1],
                            0.021 - i as f32 * 0.0007,
                            p.wood,
                        );
                    }
                }
                shaft(&mut parts, meshes, 0.025, -0.08, 0.08, p.leather);
                // String and nocked arrow are animated attachments in action_fx.
            }
            _ => unreachable!(),
        }
        weapons.insert(kind, parts);
    }
}
fn shaft(
    parts: &mut Vec<Piece>,
    meshes: &mut Assets<Mesh>,
    radius: f32,
    bottom: f32,
    top: f32,
    material: &Handle<StandardMaterial>,
) {
    piece(
        parts,
        meshes,
        Cylinder::new(radius, top - bottom)
            .mesh()
            .resolution(12)
            .build(),
        material,
        Transform::from_xyz(0., (top + bottom) * 0.5, 0.),
    );
}
fn band(
    parts: &mut Vec<Piece>,
    meshes: &mut Assets<Mesh>,
    radius: f32,
    y: f32,
    material: &Handle<StandardMaterial>,
) {
    shaft(parts, meshes, radius, y - 0.018, y + 0.018, material);
}
fn line(
    parts: &mut Vec<Piece>,
    meshes: &mut Assets<Mesh>,
    a: Vec3,
    b: Vec3,
    radius: f32,
    material: &Handle<StandardMaterial>,
) {
    piece(
        parts,
        meshes,
        Cylinder::new(radius, a.distance(b))
            .mesh()
            .resolution(8)
            .build(),
        material,
        Transform::from_translation((a + b) * 0.5)
            .with_rotation(Quat::from_rotation_arc(Vec3::Y, (b - a).normalize())),
    );
}
fn extrude(outline: &[[f32; 2]], depth: f32) -> Mesh {
    let center = outline
        .iter()
        .fold(Vec2::ZERO, |a, p| a + Vec2::from_array(*p))
        / outline.len() as f32;
    let signed_area: f32 = outline
        .iter()
        .zip(outline.iter().cycle().skip(1))
        .take(outline.len())
        .map(|(a, b)| a[0] * b[1] - b[0] * a[1])
        .sum();
    let mut vertices = Vec::<[f32; 3]>::new();
    for (a, b) in outline
        .iter()
        .zip(outline.iter().cycle().skip(1))
        .take(outline.len())
    {
        let a = Vec3::new(a[0], a[1], depth * 0.5);
        let b = Vec3::new(b[0], b[1], depth * 0.5);
        let back = Vec3::Z * depth;
        for p in [
            Vec3::new(center.x, center.y, depth * 0.5),
            a,
            b,
            Vec3::new(center.x, center.y, -depth * 0.5),
            b - back,
            a - back,
            a,
            a - back,
            b - back,
            a,
            b - back,
            b,
        ] {
            vertices.push(p.to_array());
        }
    }
    if signed_area < 0. {
        for t in vertices.chunks_exact_mut(3) {
            t.swap(1, 2);
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
    .with_computed_flat_normals()
}
