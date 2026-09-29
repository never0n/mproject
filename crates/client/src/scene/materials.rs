//! Metre-scaled architectural surfaces. Patterns are baked once into ordinary PBR maps.
use bevy::{
    asset::RenderAssetUsages,
    image::{ImageAddressMode, ImageFilterMode, ImageSampler, ImageSamplerDescriptor},
    mesh::VertexAttributeValues,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};
use mortown_world::{Block, Shape, Surface};
use std::{collections::HashMap, f32::consts::TAU};

const TILE_METRES: f32 = 4.;
const RESOLUTION: usize = 512;
#[derive(Clone, Copy)]
enum Pattern {
    Masonry,
    CutStone,
    Plaster,
    Wood,
    Slate,
    Paving,
    Brick,
    Fabric,
}
struct Sample {
    color: [f32; 3],
    height: f32,
    roughness: f32,
    occlusion: f32,
}
struct Maps {
    color: Handle<Image>,
    normal: Handle<Image>,
    surface: Handle<Image>,
}
pub(super) struct Palette {
    pub flat: Vec<Handle<StandardMaterial>>,
    textured: Vec<Handle<StandardMaterial>>,
    dressed_stone: [Handle<StandardMaterial>; 2],
}
impl Palette {
    pub fn for_block(&self, block: &Block) -> Handle<StandardMaterial> {
        let index = index(block.surface);
        // A tabletop hide or a small metal blank shares an old color tag with walls.
        // Do not turn these small props into brickwork, or put masonry on carved trim.
        if matches!(block.surface, Surface::Brick) && block.size[1] < 2. {
            return self.flat[index].clone();
        }
        if matches!(block.surface, Surface::Stone | Surface::DarkStone)
            && (block.detail
                || matches!(block.shape, Shape::Arch)
                || block.size[1] < 0.8
                || block.size[0].min(block.size[2]) < 0.3)
        {
            return self.dressed_stone[usize::from(matches!(block.surface, Surface::DarkStone))]
                .clone();
        }
        self.textured[index].clone()
    }
    pub fn paving(&self) -> Handle<StandardMaterial> {
        self.textured[index(Surface::Paving)].clone()
    }
}
fn index(surface: Surface) -> usize {
    match surface {
        Surface::Stone => 0,
        Surface::Plaster => 1,
        Surface::Roof => 2,
        Surface::Wood => 3,
        Surface::Gold => 4,
        Surface::Water => 5,
        Surface::Paving => 6,
        Surface::Brick => 7,
        Surface::BluePlaster => 8,
        Surface::RedRoof => 9,
        Surface::GreenRoof => 10,
        Surface::Glass => 11,
        Surface::Banner => 12,
        Surface::DarkStone => 13,
    }
}
pub(super) fn palette(
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
) -> Palette {
    let patterns = [
        Pattern::Masonry,
        Pattern::CutStone,
        Pattern::Plaster,
        Pattern::Wood,
        Pattern::Slate,
        Pattern::Paving,
        Pattern::Brick,
        Pattern::Fabric,
    ];
    let maps: Vec<_> = patterns.into_iter().map(|p| bake(p, images)).collect();
    let colors = [
        [0.61, 0.61, 0.58],
        [0.83, 0.77, 0.63],
        [0.27, 0.34, 0.40],
        [0.38, 0.25, 0.15],
        [0.77, 0.54, 0.20],
        [0.10, 0.36, 0.43],
        [0.67, 0.66, 0.62],
        [0.66, 0.43, 0.31],
        [0.59, 0.69, 0.67],
        [0.56, 0.29, 0.19],
        [0.25, 0.40, 0.34],
        [0.54, 0.81, 0.94],
        [0.45, 0.09, 0.19],
        [0.36, 0.39, 0.41],
    ];
    let mut flat = vec![];
    let mut textured = vec![];
    for (i, [r, g, b]) in colors.into_iter().enumerate() {
        let base = StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness: if i == 5 {
                0.25
            } else if i == 4 {
                0.42
            } else {
                0.88
            },
            metallic: if i == 4 { 0.78 } else { 0. },
            emissive: if i == 11 {
                LinearRgba::rgb(0.15, 0.35, 0.5)
            } else {
                LinearRgba::BLACK
            },
            ..default()
        };
        flat.push(materials.add(base.clone()));
        let pattern = match i {
            0 | 13 => Some(0),
            1 | 8 => Some(2),
            2 | 9 | 10 => Some(4),
            3 => Some(3),
            6 => Some(5),
            7 => Some(6),
            12 => Some(7),
            _ => None,
        };
        textured.push(if let Some(p) = pattern {
            materials.add(mapped(base, &maps[p]))
        } else {
            flat[i].clone()
        });
    }
    let dressed_stone = [0, 13].map(|i| {
        let base = materials.get(&flat[i]).unwrap().clone();
        materials.add(mapped(base, &maps[1]))
    });
    Palette {
        flat,
        textured,
        dressed_stone,
    }
}
fn mapped(mut material: StandardMaterial, maps: &Maps) -> StandardMaterial {
    material.base_color_texture = Some(maps.color.clone());
    material.normal_map_texture = Some(maps.normal.clone());
    material.metallic_roughness_texture = Some(maps.surface.clone());
    material.occlusion_texture = Some(maps.surface.clone());
    material.perceptual_roughness = 1.;
    material
}
fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ seed;
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    (h ^ (h >> 16)) as f32 / u32::MAX as f32
}
fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
/// Integer frequency and wrapped lattice give matching edges, including the normal map.
fn noise(u: f32, v: f32, freq: i32, seed: u32) -> f32 {
    let x = u * freq as f32;
    let y = v * freq as f32;
    let ix = x.floor() as i32;
    let iy = y.floor() as i32;
    let tx = smooth(0., 1., x.rem_euclid(1.));
    let ty = smooth(0., 1., y.rem_euclid(1.));
    let h = |dx: i32, dy: i32| hash((ix + dx).rem_euclid(freq), (iy + dy).rem_euclid(freq), seed);
    (h(0, 0) * (1. - tx) + h(1, 0) * tx) * (1. - ty) + (h(0, 1) * (1. - tx) + h(1, 1) * tx) * ty
}
fn sample(pattern: Pattern, u: f32, v: f32) -> Sample {
    let grain = noise(u, v, 128, 117);
    let broad = noise(u, v, 8, 29);
    let (value, height, roughness, occlusion) = match pattern {
        Pattern::Masonry | Pattern::Brick | Pattern::Paving | Pattern::Slate => {
            let (cols, rows, bevel, relief) = match pattern {
                Pattern::Masonry => (6., 12., 0.025, 0.012),
                Pattern::Brick => (12., 24., 0.012, 0.006),
                Pattern::Paving => (10., 14., 0.030, 0.018),
                _ => (12., 18., 0.009, 0.008),
            };
            let uneven = if matches!(pattern, Pattern::Slate) {
                0.
            } else {
                (grain - 0.5) * 0.012
            };
            let row = (v * rows).floor() as i32;
            let stagger = (row % 2) as f32 * 0.5;
            let x = u * cols + stagger;
            let column = (x.floor() as i32).rem_euclid(cols as i32);
            let cell = hash(column, row, 611);
            let p = Vec2::new(
                (x.rem_euclid(1.) - 0.5) * TILE_METRES / cols,
                ((v * rows).fract() - 0.5) * TILE_METRES / rows,
            );
            let half = Vec2::new(TILE_METRES / cols, TILE_METRES / rows) * 0.5;
            let q = p.abs() - half + Vec2::splat(bevel);
            let distance = bevel - (q.max(Vec2::ZERO).length() + q.x.max(q.y).min(0.)) + uneven;
            let face = smooth(0.004, bevel, distance);
            let mineral = 0.76 + cell * 0.20 + (broad - 0.5) * 0.07 + (grain - 0.5) * 0.045;
            let dirt = 0.36 + broad * 0.10;
            let overlap = if matches!(pattern, Pattern::Slate) {
                (1. - (v * rows).fract()) * 0.004
            } else {
                0.
            };
            (
                dirt * (1. - face) + mineral * face,
                (relief + (cell - 0.5) * relief * 0.22 + overlap) * face + (grain - 0.5) * 0.0008,
                0.94 - face * (0.05 + cell * 0.08),
                0.80 + face * 0.20,
            )
        }
        Pattern::Wood => {
            let board = (u * 16.).floor() as i32;
            let edge = (u * 16.).fract().min(1. - (u * 16.).fract()) * 0.25;
            let face = smooth(0.001, 0.005, edge);
            let fibre = ((u * 192. + noise(u, v, 16, 41) * 0.6) * TAU).sin() * 0.5 + 0.5;
            let knot_v = hash(board, 0, 65);
            let dy = ((v - knot_v + 0.5).rem_euclid(1.) - 0.5) * 4.;
            let dx = ((u * 16.).fract() - 0.5) * 0.25;
            let knot = (1. - smooth(0.025, 0.075, (dx * 1.9).hypot(dy * 0.35))) * 0.12;
            let color = 0.77 + hash(board, 0, 923) * 0.13 + broad * 0.045 - fibre * 0.055 - knot;
            (
                0.30 * (1. - face) + color * face,
                face * 0.003 + fibre * 0.00035,
                0.87 + (grain - 0.5) * 0.1,
                0.85 + face * 0.15,
            )
        }
        Pattern::Plaster => (
            0.87 + broad * 0.10 + (grain - 0.5) * 0.045,
            broad * 0.0015 + grain * 0.0007,
            0.96,
            1.,
        ),
        Pattern::CutStone => (
            0.82 + broad * 0.12 + (grain - 0.5) * 0.08,
            broad * 0.002 + grain * 0.0008,
            0.89 + (grain - 0.5) * 0.08,
            1.,
        ),
        Pattern::Fabric => {
            let weave = (u * TAU * 128.).sin() * (v * TAU * 128.).sin();
            (
                0.91 + weave * 0.022 + broad * 0.025,
                weave * 0.00010,
                0.98,
                1.,
            )
        }
    };
    Sample {
        color: [value, value * 0.994, value * 0.975],
        height,
        roughness,
        occlusion,
    }
}
fn byte(v: f32) -> u8 {
    (v.clamp(0., 1.) * 255. + 0.5) as u8
}
fn bake(pattern: Pattern, images: &mut Assets<Image>) -> Maps {
    let size = RESOLUTION;
    let samples: Vec<_> = (0..size * size)
        .map(|i| {
            sample(
                pattern,
                (i % size) as f32 / size as f32,
                (i / size) as f32 / size as f32,
            )
        })
        .collect();
    let mut color = Vec::with_capacity(size * size * 4);
    let mut normal = Vec::with_capacity(size * size * 4);
    let mut surface = Vec::with_capacity(size * size * 4);
    let height = |x: usize, y: usize| samples[(y % size) * size + x % size].height;
    let delta = TILE_METRES / size as f32 * 2.;
    for y in 0..size {
        for x in 0..size {
            let s = &samples[y * size + x];
            let dx = (height(x + 1, y) - height(x + size - 1, y)) / delta;
            let dy = (height(x, y + 1) - height(x, y + size - 1)) / delta;
            let n = Vec3::new(-dx, -dy, 1.).normalize();
            color.extend([byte(s.color[0]), byte(s.color[1]), byte(s.color[2]), 255]);
            normal.extend([
                byte(n.x * 0.5 + 0.5),
                byte(n.y * 0.5 + 0.5),
                byte(n.z * 0.5 + 0.5),
                255,
            ]);
            surface.extend([byte(s.occlusion), byte(s.roughness), 0, 255]);
        }
    }
    Maps {
        color: images.add(image(color, Map::Color)),
        normal: images.add(image(normal, Map::Normal)),
        surface: images.add(image(surface, Map::Surface)),
    }
}
#[derive(Clone, Copy)]
enum Map {
    Color,
    Normal,
    Surface,
}
fn image(mut pixels: Vec<u8>, kind: Map) -> Image {
    let mut all = pixels.clone();
    let mut size = RESOLUTION;
    while size > 1 {
        let next_size = size / 2;
        let mut next = vec![0; next_size * next_size * 4];
        for y in 0..next_size {
            for x in 0..next_size {
                let mut average = Vec3::ZERO;
                for dy in 0..2 {
                    for dx in 0..2 {
                        let at = ((y * 2 + dy) * size + x * 2 + dx) * 4;
                        let p = Vec3::new(
                            pixels[at] as f32,
                            pixels[at + 1] as f32,
                            pixels[at + 2] as f32,
                        ) / 255.;
                        average += match kind {
                            Map::Color => {
                                let c = Color::srgb(p.x, p.y, p.z).to_linear();
                                Vec3::new(c.red, c.green, c.blue)
                            }
                            Map::Normal => p * 2. - Vec3::ONE,
                            Map::Surface => p,
                        };
                    }
                }
                average *= 0.25;
                average = match kind {
                    Map::Color => {
                        let c = Color::linear_rgb(average.x, average.y, average.z).to_srgba();
                        Vec3::new(c.red, c.green, c.blue)
                    }
                    Map::Normal => average.normalize_or(Vec3::Z) * 0.5 + Vec3::splat(0.5),
                    Map::Surface => average,
                };
                let at = (y * next_size + x) * 4;
                next[at..at + 4].copy_from_slice(&[
                    byte(average.x),
                    byte(average.y),
                    byte(average.z),
                    255,
                ]);
            }
        }
        all.extend_from_slice(&next);
        pixels = next;
        size = next_size;
    }
    let mut image = Image::new(
        Extent3d {
            width: RESOLUTION as u32,
            height: RESOLUTION as u32,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        vec![255; RESOLUTION * RESOLUTION * 4],
        if matches!(kind, Map::Color) {
            TextureFormat::Rgba8UnormSrgb
        } else {
            TextureFormat::Rgba8Unorm
        },
        RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.mip_level_count = RESOLUTION.ilog2() + 1;
    image.data = Some(all);
    image.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
        address_mode_u: ImageAddressMode::Repeat,
        address_mode_v: ImageAddressMode::Repeat,
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        anisotropy_clamp: 8,
        ..default()
    });
    image
}

/// Unit meshes are cached on the CPU until the static town batches have been built.
#[derive(Default)]
pub(super) struct GeometryCache(HashMap<[u32; 5], Mesh>);
impl GeometryCache {
    pub fn get(&mut self, source: &Mesh, block: &Block) -> &Mesh {
        let shape = match block.shape {
            Shape::Box => 0,
            Shape::Gable => 1,
            Shape::Cylinder => 2,
            Shape::Cone => 3,
            Shape::Arch => 4,
        };
        let wood = matches!(block.surface, Surface::Wood);
        let key = [
            shape,
            block.size[0].to_bits(),
            block.size[1].to_bits(),
            block.size[2].to_bits(),
            u32::from(wood),
        ];
        self.0
            .entry(key)
            .or_insert_with(|| unwrap(source, block, wood))
    }
}
fn unwrap(source: &Mesh, block: &Block, wood: bool) -> Mesh {
    // Flat face vertices also give the arch and roof a clean UV seam at each corner.
    let mut result = source.clone().with_duplicated_vertices();
    let Some(VertexAttributeValues::Float32x3(p)) = result.attribute(Mesh::ATTRIBUTE_POSITION)
    else {
        return result;
    };
    let Some(VertexAttributeValues::Float32x3(normals)) = result.attribute(Mesh::ATTRIBUTE_NORMAL)
    else {
        return result;
    };
    let dimensions = Vec3::from_array(block.size).max(Vec3::splat(0.001));
    let mut uv = Vec::with_capacity(p.len());
    for (triangle, face_normals) in p.chunks_exact(3).zip(normals.chunks_exact(3)) {
        let normal = (Vec3::from_array(face_normals[0]) / dimensions).normalize();
        let cylindrical =
            matches!(block.shape, Shape::Cylinder | Shape::Cone) && normal.y.abs() < 0.98;
        if cylindrical {
            let mut angles: Vec<_> = triangle
                .iter()
                .map(|p| p[0].atan2(p[2]) / TAU + 0.5)
                .collect();
            let min = angles.iter().copied().fold(f32::INFINITY, f32::min);
            let max = angles.iter().copied().fold(f32::NEG_INFINITY, f32::max);
            if max - min > 0.5 {
                for angle in &mut angles {
                    if *angle < 0.5 {
                        *angle += 1.;
                    }
                }
            }
            // Cone apex inherits its face angle, instead of twisting to an arbitrary atan2(0,0).
            let edge: Vec<_> = triangle
                .iter()
                .zip(&angles)
                .filter(|(p, _)| p[0].hypot(p[2]) > 0.01)
                .map(|(_, a)| *a)
                .collect();
            for (p, angle) in triangle.iter().zip(angles) {
                let angle = if p[0].hypot(p[2]) < 0.01 && !edge.is_empty() {
                    edge.iter().sum::<f32>() / edge.len() as f32
                } else {
                    angle
                };
                uv.push([
                    angle * TAU * (dimensions.x + dimensions.z) * 0.25 / TILE_METRES,
                    p[1] * dimensions.y / TILE_METRES,
                ]);
            }
        } else {
            let mut vertical = if normal.y.abs() > 0.98 {
                Vec3::Z
            } else {
                Vec3::Y
            };
            if wood {
                vertical = [Vec3::X, Vec3::Y, Vec3::Z]
                    .into_iter()
                    .max_by(|a, b| {
                        ((Vec3::ONE - normal.abs()).dot(*a) * dimensions.dot(*a))
                            .total_cmp(&((Vec3::ONE - normal.abs()).dot(*b) * dimensions.dot(*b)))
                    })
                    .unwrap();
            }
            let v = (vertical - normal * normal.dot(vertical)).normalize_or(Vec3::Z);
            let u = v.cross(normal).normalize_or(Vec3::X);
            for p in triangle {
                let p = Vec3::from_array(*p) * dimensions;
                uv.push([p.dot(u) / TILE_METRES, p.dot(v) / TILE_METRES]);
            }
        }
    }
    result.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    if let Err(error) = result.generate_tangents() {
        warn!("Architectural tangents: {error}");
    }
    result
}
