//! Keep UV variants from creating a draw entity for every stone/beam in town.
use super::{
    Detail, Roof,
    materials::{GeometryCache, Palette},
};
use bevy::prelude::*;
use mortown_world::{Block, Shape};
use std::collections::{HashMap, hash_map::Entry};

#[derive(Hash, PartialEq, Eq)]
struct Key {
    cell: IVec3,
    material: AssetId<StandardMaterial>,
    roof: Option<usize>,
    detail: bool,
}
struct Batch {
    mesh: Mesh,
    material: Handle<StandardMaterial>,
    origin: Vec3,
}
fn spawn(commands: &mut Commands, meshes: &mut Assets<Mesh>, key: &Key, batch: Batch) {
    let mut entity = commands.spawn((
        Name::new("Town surface batch"),
        Mesh3d(meshes.add(batch.mesh)),
        MeshMaterial3d(batch.material),
        Transform::from_translation(batch.origin),
    ));
    if key.detail {
        entity.insert(Detail);
    }
    if let Some(hall) = key.roof {
        entity.insert(Roof(hall));
    }
}
pub(super) fn spawn_blocks(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    palette: &Palette,
    blocks: &[Block],
    sources: [&Mesh; 5],
) {
    let mut cache = GeometryCache::default();
    let mut batches: HashMap<Key, Batch> = HashMap::new();
    for block in blocks {
        let source = sources[match block.shape {
            Shape::Box => 0,
            Shape::Gable => 1,
            Shape::Cylinder => 2,
            Shape::Cone => 3,
            Shape::Arch => 4,
        }];
        if block.hidden {
            continue;
        }
        let center = Vec3::from_array(block.center);
        let cell = (center / 32.).floor().as_ivec3();
        let origin = cell.as_vec3() * 32. + Vec3::splat(16.);
        let material = palette.for_block(block);
        let key = Key {
            cell,
            material: material.id(),
            roof: block.roof_of,
            detail: block.detail,
        };
        let mesh = cache.get(source, block).clone().transformed_by(
            Transform::from_translation(center - origin)
                .with_rotation(Quat::from_rotation_y(block.yaw) * Quat::from_rotation_z(block.tilt))
                .with_scale(Vec3::from_array(block.size)),
        );
        match batches.entry(key) {
            Entry::Vacant(entry) => {
                entry.insert(Batch {
                    mesh,
                    material,
                    origin,
                });
            }
            Entry::Occupied(mut entry) => {
                // Current primitive layouts match. Retain the old geometry if a later
                // primitive has different attributes, instead of losing it or panicking.
                let same_layout = entry
                    .get()
                    .mesh
                    .attributes()
                    .map(|(a, _)| (a.id, a.format))
                    .eq(mesh.attributes().map(|(a, _)| (a.id, a.format)));
                let mut merged = entry.get().mesh.clone();
                if same_layout && merged.merge(&mesh).is_ok() {
                    entry.get_mut().mesh = merged;
                } else {
                    spawn(
                        commands,
                        meshes,
                        entry.key(),
                        Batch {
                            mesh,
                            material,
                            origin,
                        },
                    );
                }
            }
        }
    }
    for (key, batch) in batches {
        spawn(commands, meshes, &key, batch);
    }
}
