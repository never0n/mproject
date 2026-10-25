//! Rotatable 3D view of the casting window, rendered off-screen on its own layer.
//! The drawing is shown through the same `casting::plan` cells the server pours; the blank is
//! shown from the cells the server stored, so an unfilled misrun is missing from the model.

use super::Board;
use crate::network::Network;
use bevy::{
    camera::{RenderTarget, visibility::RenderLayers},
    prelude::*,
    render::render_resource::TextureFormat,
};
use mortown_protocol::casting::{self, Span};

pub(super) const WIDTH: f32 = 640.;
pub(super) const HEIGHT: f32 = 160.;
/// Layer 1 is the character preview; the world is layer 0.
const LAYER: usize = 2;
/// Vertical field of view of the preview camera, radians.
const FOV: f32 = std::f32::consts::FRAC_PI_4;

#[derive(Component)]
pub(crate) struct View3d;
#[derive(Component)]
pub(crate) struct PreviewCamera;
#[derive(Component)]
pub(crate) struct PreviewBody;
#[derive(Component)]
pub(crate) struct PreviewNote;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub enum Source {
    #[default]
    Design,
    Blank,
}

#[derive(Clone, PartialEq)]
struct Key {
    source: Source,
    points: Vec<[i32; 2]>,
    closed: bool,
    thickness: [u16; super::STATIONS],
    revision: Option<u64>,
}

pub struct Orbit {
    pub source: Source,
    pub yaw: f32,
    pub pitch: f32,
    pub drag: Option<Vec2>,
    shown: Option<Key>,
}
impl Default for Orbit {
    fn default() -> Self {
        Self {
            source: Source::Design,
            yaw: -0.35,
            pitch: 0.45,
            drag: None,
            shown: None,
        }
    }
}
impl Orbit {
    pub fn reset(&mut self) {
        let source = self.source;
        *self = Self {
            source,
            ..default()
        };
    }

    /// Mouse drag in physical pixels turns the model.
    pub fn turn(&mut self, delta: Vec2) {
        self.yaw += delta.x * 0.01;
        self.pitch = (self.pitch + delta.y * 0.01).clamp(-1.45, 1.45);
    }
}

pub(super) fn spawn_scene(
    commands: &mut Commands,
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<Image> {
    let target = images.add(Image::new_target_texture(
        WIDTH as u32,
        HEIGHT as u32,
        TextureFormat::Rgba8Unorm,
        Some(TextureFormat::Rgba8UnormSrgb),
    ));
    commands.spawn((
        PreviewCamera,
        Camera3d::default(),
        bevy::core_pipeline::tonemapping::Tonemapping::AcesFitted,
        Camera {
            order: -3,
            is_active: false,
            clear_color: Color::srgb(0.05, 0.045, 0.035).into(),
            ..default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: FOV,
            near: 0.01,
            ..default()
        }),
        RenderTarget::Image(target.clone().into()),
        RenderLayers::layer(LAYER),
        Transform::from_xyz(0., 0., 1.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    for (from, illuminance) in [
        (Vec3::new(1., 2., 2.), 9_000.),
        (Vec3::new(-2., 1., -1.), 3_500.),
    ] {
        commands.spawn((
            DirectionalLight {
                illuminance,
                shadows_enabled: false,
                ..default()
            },
            Transform::from_translation(from).looking_at(Vec3::ZERO, Vec3::Y),
            RenderLayers::layer(LAYER),
        ));
    }
    commands.spawn((
        PreviewBody,
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.74, 0.52, 0.27),
            metallic: 0.35,
            perceptual_roughness: 0.45,
            ..default()
        })),
        Transform::default(),
        Visibility::Hidden,
        RenderLayers::layer(LAYER),
    ));
    target
}

/// Cells to show and the line under the view.
fn chosen(board: &Board, network: &Network) -> Result<(Vec<Span>, String), String> {
    match board.orbit.source {
        Source::Design => {
            if !board.closed {
                return Err("Проект: замкните контур, чтобы увидеть объём".into());
            }
            let spans = casting::plan(&board.points, &board.thickness)
                .map_err(|problem| format!("Проект: {}", problem.message()))?;
            let volume_mm3: f64 = spans
                .iter()
                .map(|span| {
                    f64::from(span.x1_mm - span.x0_mm)
                        * f64::from(span.width_mm)
                        * f64::from(span.thickness_mm)
                })
                .sum();
            let full = spans
                .into_iter()
                .map(|span| Span {
                    fill_milli: 1000,
                    ..span
                })
                .collect::<Vec<_>>();
            let note = format!(
                "Проект до сохранения: {} сечений по 2,5 мм, объём {:.1} см³. Так форму проверит сервер",
                full.len(),
                volume_mm3 / 1000.
            );
            Ok((full, note))
        }
        Source::Blank => {
            let view = network
                .casting
                .as_ref()
                .filter(|view| view.result_item.is_some())
                .ok_or("Отливка: её ещё нет")?;
            let filled = view.spans.iter().filter(|span| span.fill_milli > 0).count();
            if filled == 0 {
                return Err("Отливка: в форме нет металла".into());
            }
            Ok((
                view.spans.clone(),
                format!(
                    "Отливка, как её записал сервер: металл в {filled} из {} сечений",
                    view.spans.len()
                ),
            ))
        }
    }
}

#[allow(clippy::type_complexity)]
pub fn update(
    mut board: ResMut<Board>,
    network: Res<Network>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut camera: Single<(&mut Camera, &mut Transform), (With<PreviewCamera>, Without<PreviewBody>)>,
    mut body: Single<
        (Entity, &mut Transform, &mut Visibility),
        (With<PreviewBody>, Without<PreviewCamera>),
    >,
    mut note: Single<&mut Text, With<PreviewNote>>,
) {
    camera.0.is_active = board.open;
    if !board.open {
        return;
    }
    let key = Key {
        source: board.orbit.source,
        points: board.points.clone(),
        closed: board.closed,
        thickness: board.thickness,
        revision: network.casting.as_ref().map(|view| view.revision),
    };
    if board.orbit.shown.as_ref() != Some(&key) {
        match chosen(&board, &network) {
            Ok((spans, text)) => match crate::objects::blade_mesh(&spans) {
                Some(mesh) => {
                    commands.entity(body.0).insert(Mesh3d(meshes.add(mesh)));
                    *body.2 = Visibility::Inherited;
                    let min = spans.iter().map(|span| span.x0_mm).min().unwrap_or(0);
                    let max = spans.iter().map(|span| span.x1_mm).max().unwrap_or(0);
                    let radius = (max - min).max(20) as f32 / 2000.;
                    // The view is four times wider than tall: fit the length across it, and keep
                    // the near end clear of the camera when the blade is turned toward it.
                    let distance = radius * 1.6 + 0.02;
                    *camera.1 =
                        Transform::from_xyz(0., 0., distance).looking_at(Vec3::ZERO, Vec3::Y);
                    note.0 = format!("{text}. Перетащите мышью, чтобы повернуть");
                }
                None => {
                    *body.2 = Visibility::Hidden;
                    note.0 = text;
                }
            },
            Err(text) => {
                *body.2 = Visibility::Hidden;
                note.0 = text;
            }
        }
        board.orbit.shown = Some(key);
    }
    body.1.rotation = Quat::from_euler(EulerRot::YXZ, board.orbit.yaw, board.orbit.pitch, 0.);
}
