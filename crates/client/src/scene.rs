mod batching;
mod materials;
mod props;
use crate::{DestinationMarker, Orbit, Player, RunOptions, Terrain};
use bevy::{
    asset::RenderAssetUsages, light::CascadeShadowConfigBuilder, mesh::Indices, prelude::*,
    render::render_resource::PrimitiveTopology,
};
use mortown_world::{PLAZA, inside_city, river_x};

#[derive(Component)]
pub struct Detail;
#[derive(Component)]
pub struct Roof(usize);

#[allow(clippy::too_many_arguments)]
pub fn setup(
    mut commands: Commands,
    assets: Res<AssetServer>,
    mut graphs: ResMut<Assets<AnimationGraph>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut images: ResMut<Assets<Image>>,
    world: Res<Terrain>,
    options: Res<RunOptions>,
    mut orbit: ResMut<Orbit>,
) {
    let palette = materials::palette(&mut images, &mut materials);
    let pavement = palette.paving();
    let cube = Mesh::from(Cuboid::default());
    let gable = gabled_roof();
    let unit_cylinder = Cylinder::new(0.5, 1.).mesh().resolution(12).build();
    let unit_cone = Cone::new(0.5, 1.).mesh().resolution(12).build();
    let arch = pointed_arch();
    batching::spawn_blocks(
        &mut commands,
        &mut meshes,
        &palette,
        &world.0.blocks,
        [&cube, &gable, &unit_cylinder, &unit_cone, &arch],
    );
    props::spawn(&mut commands, &assets, &world.0);
    let grass = materials.add(StandardMaterial {
        base_color: Color::WHITE,
        perceptual_roughness: 1.,
        ..default()
    });
    // Rendering and movement share the same grid and triangle diagonal.
    // Paving is a material of the ground itself, never a second floating surface.
    for z in -18..18 {
        for x in -18..18 {
            let meshes_for_tile = ground_patch(x as f32 * 512., z as f32 * 512., &world.0);
            for (part, material) in meshes_for_tile
                .into_iter()
                .zip([grass.clone(), pavement.clone()])
            {
                if let Some(part) = part {
                    commands.spawn((Mesh3d(meshes.add(part)), MeshMaterial3d(material)));
                }
            }
        }
    }
    for ramp in &world.0.ramps {
        commands.spawn((
            Mesh3d(meshes.add(ramp_mesh(ramp))),
            MeshMaterial3d(pavement.clone()),
        ));
    }
    commands.spawn((
        Mesh3d(meshes.add(river_mesh())),
        MeshMaterial3d(palette.flat[5].clone()),
    ));
    let cylinder = meshes.add(unit_cylinder);
    let cone = meshes.add(unit_cone);
    let rock = meshes.add(Sphere::new(1.).mesh().ico(1).unwrap());
    let tree_mats: Vec<_> = [
        Color::srgb(0.08, 0.21, 0.16),
        Color::srgb(0.13, 0.27, 0.18),
        Color::srgb(0.27, 0.34, 0.14),
        Color::srgb(0.42, 0.28, 0.12),
    ]
    .into_iter()
    .map(|c| materials.add(c))
    .collect();
    for tree in &world.0.trees {
        let [x, y, z] = tree.position;
        let scale = tree.scale;
        let tint = tree.tint;
        commands.spawn((
            Mesh3d(cylinder.clone()),
            MeshMaterial3d(palette.flat[3].clone()),
            Transform::from_xyz(x, y + 4. * scale, z).with_scale(Vec3::new(1.1, 8., 1.1) * scale),
            Detail,
        ));
        if tint < 2 {
            for layer in 0..3 {
                let l = layer as f32;
                commands.spawn((
                    Mesh3d(cone.clone()),
                    MeshMaterial3d(tree_mats[tint].clone()),
                    Transform::from_xyz(x, y + (7. + l * 4.) * scale, z)
                        .with_scale(Vec3::new(11. - l * 2.5, 10. - l, 11. - l * 2.5) * scale),
                ));
            }
        } else {
            for (dx, dy, dz, s) in [(-2., 9., 0., 5.), (2., 11., 1., 5.), (0., 14., -1., 4.)] {
                commands.spawn((
                    Mesh3d(rock.clone()),
                    MeshMaterial3d(tree_mats[tint].clone()),
                    Transform::from_xyz(x + dx * scale, y + dy * scale, z + dz * scale)
                        .with_scale(Vec3::splat(s * scale)),
                ));
            }
        }
    }
    let mut spawn = mortown_world::SPAWN;
    if options.view == "overview" {
        orbit.distance = 1200.;
        orbit.current_distance = 1200.;
        orbit.pitch = 0.88;
        orbit.yaw = -0.35;
    }
    if options.view == "interior" {
        spawn = world.0.halls[2].center;
        orbit.distance = 10.;
        orbit.current_distance = 10.;
        orbit.yaw = world.0.halls[2].yaw + std::f32::consts::FRAC_PI_2;
        orbit.distance = 20.;
        orbit.current_distance = 20.;
    }
    if options.view == "forest" {
        spawn = [-780., 460.];
        orbit.distance = 90.;
        orbit.current_distance = 90.;
        orbit.pitch = 0.23;
        orbit.yaw = -1.0;
    }
    if options.view == "skyline" {
        spawn = world.0.nearest_street([280., 620.]).0;
        orbit.distance = 300.;
        orbit.current_distance = 300.;
        orbit.pitch = 0.20;
        orbit.yaw = 0.45;
    }
    if options.view == "street" {
        let s = &world.0.streets[4];
        spawn = s.points[s.points.len() / 3];
        orbit.distance = 24.;
        orbit.current_distance = 24.;
        orbit.pitch = 0.2;
        orbit.yaw = -1.2;
    }
    if options.view == "stairs" {
        let hall = &world.0.halls[2];
        spawn = hall.entrance;
        orbit.distance = 24.;
        orbit.current_distance = 24.;
        orbit.pitch = 0.28;
        orbit.yaw = hall.yaw;
    }
    if options.view == "region" {
        spawn = [-2200., 1600.];
        orbit.distance = 2100.;
        orbit.current_distance = 2100.;
        orbit.pitch = 0.45;
        orbit.yaw = -0.4;
    }
    if options.view == "mountains" {
        spawn = [-4800., 950.];
        orbit.distance = 180.;
        orbit.current_distance = 180.;
        orbit.pitch = 0.22;
        orbit.yaw = 1.8;
    }
    if options.view == "bridge" {
        let b = world.0.bridge.unwrap();
        spawn = [b.center[0] - 105., b.center[1]];
        orbit.distance = 110.;
        orbit.current_distance = 110.;
        orbit.pitch = 0.35;
        orbit.yaw = -2.4;
    }
    if options.view == "hero" {
        orbit.distance = 4.;
        orbit.current_distance = 4.;
        orbit.pitch = 0.12;
        orbit.yaw = 0.3;
    }
    if options.view == "house" {
        spawn = world.0.halls[4].center;
        orbit.distance = 15.;
        orbit.current_distance = 15.;
        orbit.pitch = 0.7;
    }
    if options.view == "run" {
        orbit.distance = 6.;
        orbit.current_distance = 6.;
        orbit.pitch = 0.15;
        orbit.yaw = 1.57;
        let to = [spawn[0] + 50., spawn[1]];
        if let Ok(path) = mortown_world::navigation::find_path(&world.0, spawn, to) {
            commands.insert_resource(crate::Movement {
                path: path.into(),
                destination: Some(to),
                ..default()
            });
        }
    }
    let model = crate::character::load(&mut commands, &assets, &mut graphs);
    commands
        .spawn((
            Player,
            crate::character::AnimatedActor::default(),
            Transform::from_xyz(
                spawn[0],
                world.0.surface_height(spawn[0], spawn[1]),
                spawn[1],
            ),
            Visibility::default(),
        ))
        .with_children(|p| {
            p.spawn((
                SceneRoot(model),
                Transform::default(),
                crate::appearance::Wearer::Local,
            ));
        });
    commands.spawn((
        DestinationMarker,
        Mesh3d(meshes.add(Torus::new(0.9, 1.15))),
        MeshMaterial3d(palette.flat[4].clone()),
        Transform::default(),
        Visibility::Hidden,
    ));
    commands.insert_resource(GlobalAmbientLight {
        color: Color::srgb(0.70, 0.78, 1.),
        brightness: 900.,
        ..default()
    });
    commands.spawn((
        DirectionalLight {
            illuminance: 13500.,
            shadows_enabled: true,
            color: Color::srgb(1., 0.84, 0.65),
            ..default()
        },
        CascadeShadowConfigBuilder {
            first_cascade_far_bound: 35.,
            maximum_distance: 3000.,
            ..default()
        }
        .build(),
        Transform::from_xyz(-350., 520., 250.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.spawn((
        Camera3d::default(),
        IsDefaultUiCamera,
        bevy::core_pipeline::tonemapping::Tonemapping::AcesFitted,
        Msaa::Sample4,
        Projection::Perspective(PerspectiveProjection {
            near: 0.1,
            far: 26000.,
            fov: 60_f32.to_radians(),
            ..default()
        }),
        Transform::from_xyz(0., 25., 78.),
        DistanceFog {
            color: Color::srgb(0.52, 0.64, 0.73),
            falloff: FogFalloff::Linear {
                start: 5000.,
                end: 23000.,
            },
            ..default()
        },
    ));
    info!(
        "Fantasy Mortown: {} houses, {} streets, {} public halls",
        world.0.houses,
        world.0.streets.len(),
        world.0.halls.len()
    );
}
fn mesh(p: Vec<[f32; 3]>, n: Vec<[f32; 3]>, c: Vec<[f32; 4]>, i: Vec<u32>) -> Mesh {
    let uv: Vec<[f32; 2]> = p.iter().map(|v| [v[0] / 4., v[2] / 4.]).collect();
    // UVs run along world X/Z. Keep the tangent frame consistent on slopes as well.
    let tangents: Vec<[f32; 4]> = n
        .iter()
        .map(|n| {
            let t = Vec3::new(1., -n[0] / n[1].max(0.001), 0.).normalize();
            [t.x, t.y, t.z, -1.]
        })
        .collect();
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_TANGENT, tangents)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uv)
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, p)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, n)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, c)
    .with_inserted_indices(Indices::U32(i))
}
fn gabled_roof() -> Mesh {
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![
            [-0.5, -0.5, -0.5],
            [0.5, -0.5, -0.5],
            [0., 0.5, -0.5],
            [-0.5, -0.5, 0.5],
            [0.5, -0.5, 0.5],
            [0., 0.5, 0.5],
        ],
    )
    .with_inserted_indices(Indices::U32(vec![
        0, 2, 1, 3, 4, 5, 0, 3, 5, 0, 5, 2, 1, 2, 5, 1, 5, 4, 0, 1, 4, 0, 4, 3,
    ]))
    .with_duplicated_vertices()
    .with_computed_flat_normals()
}
fn pointed_arch() -> Mesh {
    let outer = [
        [-0.5, -0.5],
        [-0.5, 0.04],
        [-0.42, 0.24],
        [-0.25, 0.41],
        [0., 0.5],
        [0.25, 0.41],
        [0.42, 0.24],
        [0.5, 0.04],
        [0.5, -0.5],
    ];
    let inner = [
        [-0.36, -0.5],
        [-0.36, 0.01],
        [-0.28, 0.15],
        [-0.15, 0.27],
        [0., 0.35],
        [0.15, 0.27],
        [0.28, 0.15],
        [0.36, 0.01],
        [0.36, -0.5],
    ];
    let mut positions = vec![];
    let mut indices = vec![];
    for z in [-0.5, 0.5] {
        for i in 0..9 {
            positions.push([outer[i][0], outer[i][1], z]);
            positions.push([inner[i][0], inner[i][1], z]);
        }
    }
    for i in 0..8 {
        let a = i * 2;
        let b = a + 2;
        indices.extend_from_slice(&[
            a,
            b,
            a + 1,
            a + 1,
            b,
            b + 1,
            a + 18,
            a + 19,
            b + 18,
            a + 19,
            b + 19,
            b + 18,
            a,
            a + 18,
            b,
            b,
            b + 18,
            a + 18,
            a + 1,
            b + 1,
            a + 19,
            b + 1,
            b + 19,
            a + 19,
        ]);
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_indices(Indices::U32(indices))
    .with_duplicated_vertices()
    .with_computed_flat_normals()
}
fn ground_patch(x: f32, z: f32, world: &mortown_world::World) -> [Option<Mesh>; 2] {
    let size = mortown_world::terrain::TILE_SIZE;
    let step = mortown_world::terrain::grid_step(x + size / 2., z + size / 2.);
    let cells = (size / step) as usize;
    let mut positions = Vec::with_capacity((cells + 1) * (cells + 1));
    let mut normals = vec![];
    let mut colors = vec![];
    let mut paved_colors = vec![];
    for iz in 0..=cells {
        for ix in 0..=cells {
            let px = x + ix as f32 * step;
            let pz = z + iz as f32 * step;
            let y = world.terrain_vertex(px, pz);
            positions.push([px, y, pz]);
            let normal = Vec3::new(
                world.terrain_vertex(px - step, pz) - world.terrain_vertex(px + step, pz),
                2. * step,
                world.terrain_vertex(px, pz - step) - world.terrain_vertex(px, pz + step),
            )
            .normalize();
            normals.push(normal.to_array());
            let t = 0.025 * mortown_world::noise(px, pz, 180., 9);
            let c = if y > 1750. && normal.y > 0.65 {
                [0.76 + t, 0.80 + t, 0.82 + t]
            } else if normal.y < 0.82 || y > 500. {
                [0.34 + t, 0.37 + t, 0.39 + t]
            } else {
                [0.22 + t, 0.31 + t, 0.16 + t]
            };
            colors.push(Color::srgb(c[0], c[1], c[2]).to_linear().to_f32_array());
            let on_plaza = ((px - PLAZA[0]) / 90.).hypot((pz - PLAZA[1]) / 72.) < 1.;
            let shade = if on_plaza {
                1.
            } else if world.on_road(px, pz, 0.) {
                0.83
            } else {
                0.92
            };
            paved_colors.push([shade, shade, shade, 1.]);
        }
    }
    let mut geometry = GroundGeometry {
        positions,
        normals,
        colors,
        paved_colors,
        indices: [vec![], vec![]],
    };
    for iz in 0..cells {
        for ix in 0..cells {
            let a = (iz * (cells + 1) + ix) as u32;
            let b = a + (cells + 1) as u32;
            for tri in [[a, b, a + 1], [a + 1, b, b + 1]] {
                let center = tri
                    .iter()
                    .map(|i| Vec3::from_array(geometry.positions[*i as usize]))
                    .sum::<Vec3>()
                    / 3.;
                let refine =
                    !inside_city([center.x, center.z]) && world.on_road(center.x, center.z, step);
                let depth = if refine {
                    (step as u32).ilog2() as u8
                } else {
                    0
                };
                geometry.triangle(tri, depth, world);
            }
        }
    }
    let GroundGeometry {
        positions,
        normals,
        colors,
        paved_colors,
        indices: [earth, paving],
    } = geometry;
    [
        (!earth.is_empty()).then(|| mesh(positions.clone(), normals.clone(), colors, earth)),
        (!paving.is_empty()).then(|| mesh(positions, normals, paved_colors, paving)),
    ]
}
struct GroundGeometry {
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    colors: Vec<[f32; 4]>,
    paved_colors: Vec<[f32; 4]>,
    indices: [Vec<u32>; 2],
}
impl GroundGeometry {
    fn midpoint(&mut self, a: u32, b: u32) -> u32 {
        let a = a as usize;
        let b = b as usize;
        let index = self.positions.len() as u32;
        self.positions.push(std::array::from_fn(|i| {
            (self.positions[a][i] + self.positions[b][i]) / 2.
        }));
        self.normals.push(
            (Vec3::from_array(self.normals[a]) + Vec3::from_array(self.normals[b]))
                .normalize()
                .to_array(),
        );
        self.colors.push(std::array::from_fn(|i| {
            (self.colors[a][i] + self.colors[b][i]) / 2.
        }));
        self.paved_colors.push(std::array::from_fn(|i| {
            (self.paved_colors[a][i] + self.paved_colors[b][i]) / 2.
        }));
        index
    }
    fn triangle(&mut self, tri: [u32; 3], depth: u8, world: &mortown_world::World) {
        if depth > 0 {
            // Subdivide on the original plane. Road edges become finer without
            // changing the ground height or introducing a floating road mesh.
            let [a, b, c] = tri;
            let ab = self.midpoint(a, b);
            let bc = self.midpoint(b, c);
            let ca = self.midpoint(c, a);
            for t in [[a, ab, ca], [ab, b, bc], [ca, bc, c], [ab, bc, ca]] {
                self.triangle(t, depth - 1, world);
            }
        } else {
            let p = tri
                .iter()
                .map(|i| Vec3::from_array(self.positions[*i as usize]))
                .sum::<Vec3>()
                / 3.;
            let paved = inside_city([p.x, p.z]) || world.on_road(p.x, p.z, 0.);
            self.indices[usize::from(paved)].extend(tri);
        }
    }
}
fn ramp_mesh(ramp: &mortown_world::Ramp) -> Mesh {
    let mut p = vec![];
    for (x, z, y) in [
        (-ramp.width / 2., -ramp.length / 2., ramp.rise),
        (-ramp.width / 2., ramp.length / 2., 0.),
        (ramp.width / 2., -ramp.length / 2., ramp.rise),
        (ramp.width / 2., ramp.length / 2., 0.),
    ] {
        let q = mortown_world::rotate(ramp.center, x, z, ramp.yaw);
        p.push([q[0], ramp.base + y, q[1]]);
    }
    let normal = (Quat::from_rotation_y(ramp.yaw)
        * Vec3::new(0., 1., ramp.rise / ramp.length).normalize())
    .to_array();
    mesh(p, vec![normal; 4], vec![[1.; 4]; 4], vec![0, 1, 2, 2, 1, 3])
}

fn river_mesh() -> Mesh {
    let mut p = vec![];
    let mut n = vec![];
    let mut c = vec![];
    let mut indices = vec![];
    for i in 0..=1000 {
        let z = -5000. + i as f32 * 10.;
        let x = river_x(z);
        for dx in [-23., 23.] {
            p.push([x + dx, 8., z]);
            n.push([0., 1., 0.]);
            c.push([1.; 4]);
        }
        if i < 1000 {
            let a = i * 2;
            indices.extend_from_slice(&[a, a + 2, a + 1, a + 1, a + 2, a + 3]);
        }
    }
    mesh(p, n, c, indices)
}
#[allow(clippy::type_complexity)]
pub fn update_visibility(
    world: Res<Terrain>,
    player: Single<&Transform, With<Player>>,
    camera: Single<
        &Transform,
        (
            With<Camera3d>,
            Without<crate::inventory::PreviewCamera>,
            Without<crate::inventory::PortraitCamera>,
        ),
    >,
    mut details: Query<(&Transform, &mut Visibility), (With<Detail>, Without<Roof>)>,
    mut roofs: Query<(&Roof, &mut Visibility)>,
) {
    let inside = world.0.hall_at(player.translation.x, player.translation.z);
    for (roof, mut v) in &mut roofs {
        *v = if Some(roof.0) == inside {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
    for (t, mut v) in &mut details {
        *v = if t.translation.distance_squared(camera.translation) > 550_f32.powi(2) {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;
    #[test]
    fn actual_rendered_ground_matches_character_support() {
        let world = mortown_world::World::new();
        for (x, z) in [(0., 0.), (-512., 0.), (1024., 0.), (-4096., 1536.)] {
            for mesh in ground_patch(x, z, &world).into_iter().flatten() {
                let Some(VertexAttributeValues::Float32x3(vertices)) =
                    mesh.attribute(Mesh::ATTRIBUTE_POSITION)
                else {
                    panic!("positions");
                };
                let Some(Indices::U32(indices)) = mesh.indices() else {
                    panic!("indices");
                };
                for tri in indices.chunks_exact(3).step_by(173) {
                    let p = Vec3::from_array(vertices[tri[0] as usize]) * 0.2
                        + Vec3::from_array(vertices[tri[1] as usize]) * 0.3
                        + Vec3::from_array(vertices[tri[2] as usize]) * 0.5;
                    assert!(
                        (p.y - world.ground_height(p.x, p.z)).abs() < 0.005,
                        "render/support mismatch at {p:?}"
                    );
                }
            }
        }
    }
}
