//! Server-owned ground areas, drawn against the same terrain surface as walking.
use crate::{
    Terrain,
    network::{Mode, Network},
};
use bevy::{
    asset::RenderAssetUsages, mesh::Indices, prelude::*, render::render_resource::PrimitiveTopology,
};

#[derive(Component)]
pub struct AreaView(u64);
#[derive(Resource)]
pub struct SandMaterial(Handle<StandardMaterial>);
pub fn setup(mut commands: Commands, mut materials: ResMut<Assets<StandardMaterial>>) {
    commands.insert_resource(SandMaterial(materials.add(StandardMaterial {
        base_color: Color::srgba(0.92, 0.63, 0.20, 0.4),
        alpha_mode: AlphaMode::Blend,
        unlit: true,
        cull_mode: None,
        ..default()
    })));
}
fn area_mesh(area: &mortown_protocol::skills::Area, world: &mortown_world::World) -> Mesh {
    const SEGMENTS: u32 = 64;
    const RINGS: u32 = 8;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for r in 0..=RINGS {
        for i in 0..=SEGMENTS {
            let angle = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
            let radius = area.radius * r as f32 / RINGS as f32;
            let x = area.position[0] + angle.cos() * radius;
            let z = area.position[2] + angle.sin() * radius;
            vertices.push([x, world.surface_height(x, z) + 0.07, z]);
        }
    }
    for r in 0..RINGS {
        for i in 0..SEGMENTS {
            let a = r * (SEGMENTS + 1) + i;
            let b = a + SEGMENTS + 1;
            indices.extend_from_slice(&[a, b + 1, b]);
            if r > 0 {
                indices.extend_from_slice(&[a, a + 1, b + 1]);
            }
        }
    }
    Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::RENDER_WORLD,
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
    .with_inserted_indices(Indices::U32(indices))
    .with_duplicated_vertices()
    .with_computed_flat_normals()
}
pub fn display(
    mut commands: Commands,
    network: Res<Network>,
    world: Res<Terrain>,
    material: Res<SandMaterial>,
    mut meshes: ResMut<Assets<Mesh>>,
    views: Query<(Entity, &AreaView)>,
) {
    for (entity, view) in &views {
        if network.mode != Mode::Online || !network.areas.iter().any(|a| a.id == view.0) {
            commands.entity(entity).despawn();
        }
    }
    if network.mode != Mode::Online {
        return;
    }
    for area in &network.areas {
        if views.iter().any(|(_, view)| view.0 == area.id) {
            continue;
        }
        commands.spawn((
            AreaView(area.id),
            Mesh3d(meshes.add(area_mesh(area, &world.0))),
            MeshMaterial3d(material.0.clone()),
            Transform::default(),
        ));
    }
}
