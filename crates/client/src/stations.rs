//! Small props on the fixed work counters (seams, coins, letters…); pure decoration, no collision.
use crate::Terrain;
use bevy::prelude::*;
use mortown_world::stations::{Furnishing, STATIONS};

fn part(
    meshes: &mut Assets<Mesh>,
    material: &Handle<StandardMaterial>,
    shape: impl Into<Mesh>,
    p: [f32; 3],
) -> (Mesh3d, MeshMaterial3d<StandardMaterial>, Transform) {
    (
        Mesh3d(meshes.add(shape)),
        MeshMaterial3d(material.clone()),
        Transform::from_translation(Vec3::from_array(p)),
    )
}
fn cuboid(
    meshes: &mut Assets<Mesh>,
    material: &Handle<StandardMaterial>,
    size: [f32; 3],
    p: [f32; 3],
) -> (Mesh3d, MeshMaterial3d<StandardMaterial>, Transform) {
    part(meshes, material, Cuboid::new(size[0], size[1], size[2]), p)
}

pub fn setup(
    mut commands: Commands,
    world: Res<Terrain>,
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
    for station in STATIONS {
        let mut parts = vec![];
        match station.furnishing {
            Furnishing::Smithy => {
                parts.push(cuboid(
                    &mut meshes,
                    &iron,
                    [0.11, 0.01, 0.045],
                    [0., 0., 0.],
                ));
            }
            Furnishing::Leatherwork => {
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
            Furnishing::Runes => {
                for (x, angle) in [(-0.055, -0.5), (0.055, 0.5)] {
                    let mut line =
                        cuboid(&mut meshes, &rune, [0.009, 0.004, 0.17], [x, 0.004, 0.015]);
                    line.2.rotation = Quat::from_rotation_y(angle);
                    parts.push(line);
                }
                parts.push(cuboid(
                    &mut meshes,
                    &rune,
                    [0.12, 0.004, 0.009],
                    [0., 0.004, 0.015],
                ));
            }
            Furnishing::Market => {
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
            Furnishing::Letters => {
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
            Furnishing::Ledger => {
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
        commands
            .spawn((
                Name::new("Counter props"),
                Transform::from_translation(Vec3::from_array(station.work_surface(&world.0)))
                    .with_rotation(Quat::from_rotation_y(station.yaw())),
                Visibility::default(),
            ))
            .with_children(|desk| {
                for p in parts {
                    desk.spawn(p);
                }
            });
    }
}
