//! Contour editor for one casting. Points are millimetres on the drawing.
//! The server rechecks the outline; this screen only sends it and draws the filled part back.

use crate::{
    actionbar::{self, Icons},
    network::{Mode, Network},
    ui::{BUTTON, EDGE, GOLD, PANEL},
};
use bevy::{prelude::*, ui::FocusPolicy, window::PrimaryWindow};
use mortown_protocol::casting::{self, Command, DefectKind, Phase};

const MARKS: usize = 48;
const W: f32 = 640.;
const H: f32 = 220.;
const STRIP: f32 = 36.;
const X0: f32 = -200.;
const X1: f32 = 700.;
const Y0: f32 = -80.;
const Y1: f32 = 80.;

#[derive(Resource)]
pub struct Board {
    pub open: bool,
    points: Vec<[i32; 2]>,
    closed: bool,
    thickness: [u16; 3],
    units: u16,
    revision: u64,
    drag: Option<usize>,
    shown: Option<u64>,
}
impl Default for Board {
    fn default() -> Self {
        Self {
            open: false,
            points: Vec::new(),
            closed: false,
            thickness: [6, 6, 6],
            units: 18,
            revision: 0,
            drag: None,
            shown: None,
        }
    }
}

#[derive(Component)]
pub(crate) struct Overlay;
#[derive(Component)]
pub(crate) struct Canvas;
#[derive(Component)]
pub(crate) struct Dot(usize);
#[derive(Component)]
pub(crate) struct Segment(usize);
#[derive(Component)]
pub(crate) struct Bars;
#[derive(Component)]
pub(crate) struct Strip;
#[derive(Component)]
pub(crate) struct Note;
#[derive(Component, Clone, Copy)]
pub(crate) enum Action {
    Back,
    Example,
    Close,
    Undo,
    Thickness(usize, i16),
    /// Grow or shrink the whole outline. The server still checks the sent points.
    Scale(bool),
    Save,
    ChargeStep(i16),
    Charge,
    Heat,
    Pour(u16, u16),
    Cool,
    Machine,
}

pub fn setup(mut commands: Commands, icons: Res<Icons>) {
    commands.init_resource::<Board>();
    commands
        .spawn((
            Overlay,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                padding: UiRect::bottom(px(70)),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.02, 0.03, 0.28)),
            GlobalZIndex(20),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(700),
                    max_width: percent(96),
                    padding: UiRect::all(px(18)),
                    border: UiRect::all(px(1)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(10),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(GOLD),
            ))
            .with_children(|p| {
                p.spawn(actionbar::label("Литьё", &icons.1, 22.));
                p.spawn(actionbar::label(
                    "Клик ставит точку контура, перетаскивание двигает её. Сохранение чертёж не тратит металл.",
                    &icons.1,
                    12.,
                ));
                p.spawn((
                    Canvas,
                    FocusPolicy::Block,
                    Node {
                        width: px(W),
                        height: px(H),
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.08, 0.07, 0.05)),
                    BorderColor::all(EDGE),
                ))
                .with_children(|c| {
                    for index in 0..MARKS {
                        c.spawn((
                            Segment(index),
                            Node {
                                display: Display::None,
                                position_type: PositionType::Absolute,
                                height: px(2),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.72, 0.62, 0.42)),
                            UiTransform::default(),
                        ));
                        c.spawn((
                            Dot(index),
                            Node {
                                display: Display::None,
                                position_type: PositionType::Absolute,
                                width: px(8),
                                height: px(8),
                                ..default()
                            },
                            BackgroundColor(Color::srgb(0.93, 0.84, 0.62)),
                        ));
                    }
                    c.spawn((
                        Bars,
                        Node {
                            width: percent(100),
                            height: percent(100),
                            ..default()
                        },
                    ));
                });
                p.spawn(actionbar::label("Толщина вдоль длины", &icons.1, 12.));
                p.spawn((
                    Strip,
                    Node {
                        width: px(W),
                        height: px(STRIP),
                        border: UiRect::all(px(1)),
                        ..default()
                    },
                    BackgroundColor(Color::srgb(0.06, 0.05, 0.04)),
                    BorderColor::all(EDGE),
                ));
                p.spawn((Note, actionbar::label("", &icons.1, 13.)));
                p.spawn(Node {
                    column_gap: px(8),
                    flex_wrap: FlexWrap::Wrap,
                    row_gap: px(8),
                    ..default()
                })
                .with_children(|p| {
                    button(p, &icons, Action::Example, "Пример меча");
                    button(p, &icons, Action::Close, "Замкнуть");
                    button(p, &icons, Action::Undo, "Убрать точку");
                    button(p, &icons, Action::Scale(false), "Мельче");
                    button(p, &icons, Action::Scale(true), "Крупнее");
                    for (station, name) in ["Основание", "Середина", "Остриё"].iter().enumerate() {
                        let down = format!("{name} −");
                        let up = format!("{name} +");
                        button(p, &icons, Action::Thickness(station, -1), &down);
                        button(p, &icons, Action::Thickness(station, 1), &up);
                    }
                    button(p, &icons, Action::Save, "Сохранить чертёж");
                    button(p, &icons, Action::ChargeStep(-1), "Тигель −100 г");
                    button(p, &icons, Action::ChargeStep(1), "Тигель +100 г");
                    button(p, &icons, Action::Charge, "Загрузить в тигель");
                    button(p, &icons, Action::Heat, "Греть до 1040 °C");
                    button(p, &icons, Action::Pour(25, 0), "Залить, широкие ворота");
                    button(p, &icons, Action::Pour(2, 0), "Залить, узкие ворота");
                    button(p, &icons, Action::Pour(25, 2), "Закрыть подачу через 2 с");
                    button(p, &icons, Action::Cool, "Остудить");
                    button(p, &icons, Action::Machine, "Снять скос 0,1 мм");
                    button(p, &icons, Action::Back, "← К ремеслу");
                });
            });
        });
}

fn button(p: &mut ChildSpawnerCommands, icons: &Icons, action: Action, title: &str) {
    p.spawn((
        action,
        Button,
        Node {
            padding: UiRect::all(px(8)),
            border: UiRect::all(px(1)),
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(EDGE),
    ))
    .with_children(|p| {
        p.spawn(actionbar::label(title, &icons.1, 13.));
    });
}

pub fn input(
    mut board: ResMut<Board>,
    mut network: ResMut<Network>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    buttons: Query<(&Action, &Interaction), Changed<Interaction>>,
    canvas: Query<(&ComputedNode, &UiGlobalTransform), With<Canvas>>,
) {
    if network.mode != Mode::Online {
        board.open = false;
        return;
    }
    for (action, interaction) in &buttons {
        if *interaction != Interaction::Pressed || !board.open {
            continue;
        }
        match action {
            Action::Back => board.open = false,
            Action::Example => {
                let (points, thickness) = casting::example_sword();
                board.points = points;
                board.thickness = [thickness[0], thickness[1], thickness[2]];
                board.closed = true;
            }
            Action::Close if board.points.len() >= 4 => board.closed = true,
            Action::Undo => {
                board.points.pop();
                board.closed = false;
            }
            Action::Scale(grow) => {
                let (num, den) = if *grow { (11, 10) } else { (10, 11) };
                if let Some(points) = scaled_outline(&board.points, num, den) {
                    board.points = points;
                }
            }
            Action::Thickness(station, step) => {
                let value = board.thickness[*station] as i16 + step;
                board.thickness[*station] = value.clamp(1, 40) as u16;
            }
            Action::ChargeStep(step) => {
                let value = board.units as i16 + step;
                board.units = value.clamp(1, 40) as u16;
            }
            Action::Save if !network.object_pending && board.closed => {
                let revision = board.revision;
                network.cast_request(Command::Save {
                    revision,
                    points: board.points.clone(),
                    thickness_mm: board.thickness.to_vec(),
                });
            }
            Action::Charge if !network.object_pending => {
                network.cast_request(Command::Charge {
                    revision: board.revision,
                    units: board.units,
                });
            }
            Action::Heat if !network.object_pending => {
                network.cast_request(Command::Heat {
                    revision: board.revision,
                    target_c: 1040,
                });
            }
            Action::Pour(gate, seconds) if !network.object_pending => {
                network.cast_request(Command::Pour {
                    revision: board.revision,
                    gate_mm2: *gate,
                    ideal_feed: true,
                    seconds: *seconds,
                });
            }
            Action::Cool if !network.object_pending => {
                if let Some(item) = network.casting.as_ref().and_then(|view| view.result_item) {
                    network.cast_request(Command::Cool { item });
                }
            }
            Action::Machine if !network.object_pending => {
                if let Some(item) = network.casting.as_ref().and_then(|view| view.result_item) {
                    network.cast_request(Command::Machine {
                        item,
                        edge_tenth_mm: 1,
                    });
                }
            }
            _ => {}
        }
    }
    let Some(cursor) = window.physical_cursor_position() else {
        return;
    };
    let Some((node, transform)) = canvas.iter().next() else {
        return;
    };
    if !board.open || !actionbar::contains(node, transform, cursor) {
        if mouse.just_released(MouseButton::Left) {
            board.drag = None;
        }
        return;
    }
    let Some(inverse) = transform.try_inverse() else {
        return;
    };
    let local = inverse.transform_point2(cursor) + node.size() / 2.;
    let uv = local / node.size();
    let mm = [
        (X0 + uv.x.clamp(0., 1.) * (X1 - X0)).round() as i32,
        (Y1 - uv.y.clamp(0., 1.) * (Y1 - Y0)).round() as i32,
    ];
    if mouse.just_pressed(MouseButton::Left) {
        let near = board.points.iter().position(|p| {
            let dx = p[0] - mm[0];
            let dy = p[1] - mm[1];
            dx * dx + dy * dy <= 12 * 12
        });
        if let Some(index) = near {
            board.drag = Some(index);
        } else if !board.closed && board.points.len() < MARKS {
            board.points.push(mm);
        }
    } else if mouse.pressed(MouseButton::Left)
        && let Some(index) = board.drag
        && let Some(point) = board.points.get_mut(index)
    {
        *point = mm;
    }
    if mouse.just_released(MouseButton::Left) {
        board.drag = None;
    }
}

pub fn display(
    mut board: ResMut<Board>,
    network: Res<Network>,
    mut commands: Commands,
    mut overlay: Query<&mut Node, (With<Overlay>, Without<Dot>, Without<Segment>)>,
    mut note: Query<&mut Text, With<Note>>,
    mut dots: Query<(&Dot, &mut Node), (Without<Segment>, Without<Overlay>)>,
    mut segments: Query<(&Segment, &mut Node, &mut UiTransform), (Without<Dot>, Without<Overlay>)>,
    bars: Query<Entity, With<Bars>>,
    strips: Query<Entity, With<Strip>>,
) {
    let mut overlay = overlay.single_mut().unwrap();
    overlay.display = if board.open {
        Display::Flex
    } else {
        Display::None
    };
    if let Some(view) = &network.casting {
        board.revision = view.revision;
    }
    if let Ok(mut text) = note.single_mut() {
        let phase = network
            .casting
            .as_ref()
            .map(|view| phase_label(view.phase))
            .unwrap_or("чертёж ещё не отправлен");
        let defects = network
            .casting
            .as_ref()
            .map(defect_note)
            .unwrap_or_default();
        let balance = network
            .casting
            .as_ref()
            .map(balance_note)
            .unwrap_or_default();
        let metal = network
            .casting
            .as_ref()
            .and_then(|view| view.temperature_c)
            .map(|temperature| format!("Металл {temperature} °C. "))
            .unwrap_or_default();
        let (length, width) = extent(&board.points);
        let edge = network
            .casting
            .as_ref()
            .and_then(edge_note)
            .unwrap_or_default();
        text.0 = format!(
            "Длина {length} мм, ширина {width} мм. Толщина {} / {} / {} мм. {edge}В тигель {} г. {phase}. {metal}{balance}{defects}{}",
            board.thickness[0],
            board.thickness[1],
            board.thickness[2],
            u32::from(board.units) * 100,
            network.status
        );
    }
    for (dot, mut node) in &mut dots {
        match board.points.get(dot.0) {
            Some(point) => {
                let (x, y) = pixel(*point);
                node.display = Display::Flex;
                node.left = px(x - 4.);
                node.top = px(y - 4.);
            }
            None => node.display = Display::None,
        }
    }
    let count = board.points.len();
    let links = if board.closed && count >= 2 {
        count
    } else {
        count.saturating_sub(1)
    };
    for (segment, mut node, mut transform) in &mut segments {
        if segment.0 >= links || count < 2 {
            node.display = Display::None;
            continue;
        }
        let a = pixel(board.points[segment.0]);
        let b = pixel(board.points[(segment.0 + 1) % count]);
        let delta = Vec2::new(b.0 - a.0, b.1 - a.1);
        let len = delta.length().max(1.);
        node.display = Display::Flex;
        node.width = px(len);
        node.left = px((a.0 + b.0) / 2. - len / 2.);
        node.top = px((a.1 + b.1) / 2. - 1.);
        transform.rotation = Rot2::radians(delta.y.atan2(delta.x));
    }
    let Some(view) = network.casting.clone() else {
        return;
    };
    if board.shown == Some(view.revision) {
        return;
    }
    let Ok(host) = bars.single() else {
        return;
    };
    let Ok(strip) = strips.single() else {
        return;
    };
    commands.entity(host).despawn_related::<Children>();
    commands.entity(strip).despawn_related::<Children>();
    for span in &view.spans {
        let height = STRIP * f32::from(span.thickness_mm).clamp(1., 40.) / 40.;
        let left = x_px(span.x0_mm);
        let right = x_px(span.x1_mm);
        commands.entity(strip).with_children(|parent| {
            parent.spawn((
                Node {
                    position_type: PositionType::Absolute,
                    left: px(left),
                    bottom: px(0.),
                    width: px((right - left).max(1.)),
                    height: px(height.max(1.)),
                    ..default()
                },
                BackgroundColor(Color::srgba(0.72, 0.45, 0.18, 0.75)),
            ));
        });
    }
    for span in &view.spans {
        let length = span.x1_mm - span.x0_mm;
        let filled = (i64::from(length) * i64::from(span.fill_milli.min(1000)) / 1000) as i32;
        if filled > 0 {
            paint(
                &mut commands,
                host,
                span.x0_mm,
                span.x0_mm + filled,
                span.width_mm,
                Color::srgba(0.72, 0.45, 0.18, 0.85),
                false,
            );
        }
        if span.fill_milli < 1000 {
            let kind = view.defects.iter().find(|defect| {
                defect.kind != DefectKind::SolidifiedFeed
                    && defect.x0_mm < span.x1_mm
                    && defect.x1_mm > span.x0_mm + filled
            });
            paint(
                &mut commands,
                host,
                span.x0_mm + filled,
                span.x1_mm,
                span.width_mm,
                empty_color(kind.map(|defect| defect.kind)),
                true,
            );
        }
    }
    for defect in &view.defects {
        if defect.kind != DefectKind::SolidifiedFeed {
            continue;
        }
        let width = view
            .spans
            .iter()
            .find(|span| span.x0_mm < defect.x1_mm && span.x1_mm > defect.x0_mm)
            .map(|span| span.width_mm)
            .unwrap_or(8);
        paint(
            &mut commands,
            host,
            defect.x0_mm,
            defect.x1_mm,
            width,
            Color::srgba(0.82, 0.9, 1., 0.95),
            false,
        );
    }
    board.shown = Some(view.revision);
}

fn paint(
    commands: &mut Commands,
    host: Entity,
    x0: i32,
    x1: i32,
    width: u16,
    color: Color,
    hollow: bool,
) {
    if x1 <= x0 {
        return;
    }
    let (left, top) = pixel([x0, i32::from(width) / 2]);
    let (right, bottom) = pixel([x1, -(i32::from(width) / 2)]);
    commands.entity(host).with_children(|parent| {
        parent.spawn((
            Node {
                position_type: PositionType::Absolute,
                left: px(left),
                top: px(top),
                width: px((right - left).max(1.)),
                height: px((bottom - top).max(1.)),
                border: if hollow {
                    UiRect::all(px(1))
                } else {
                    UiRect::all(px(0))
                },
                ..default()
            },
            BackgroundColor(if hollow {
                color.with_alpha(0.16)
            } else {
                color
            }),
            BorderColor::all(color),
        ));
    });
}

fn empty_color(kind: Option<DefectKind>) -> Color {
    match kind {
        Some(DefectKind::Halted) => Color::srgba(0.45, 0.62, 0.86, 0.9),
        Some(DefectKind::ShortCharge) => Color::srgba(0.86, 0.34, 0.24, 0.9),
        Some(DefectKind::Misrun) | Some(DefectKind::SolidifiedFeed) | None => {
            Color::srgba(0.62, 0.58, 0.48, 0.8)
        }
    }
}

fn balance_note(view: &casting::View) -> String {
    match (view.center_tenth_mm, view.inertia_mg_m2) {
        (Some(center), Some(inertia)) => format!(
            "Центр масс {:.1} мм по чертежу, инерция вокруг него {:.4} кг·м². ",
            f64::from(center) / 10.,
            inertia as f64 / 1_000_000.
        ),
        _ => String::new(),
    }
}

fn edge_note(view: &casting::View) -> Option<String> {
    let edge = view
        .spans
        .iter()
        .filter(|span| span.fill_milli > 0)
        .map(|span| span.edge_tenth_mm)
        .max()
        .filter(|edge| *edge > 0)?;
    Some(format!("Кромка {},{} мм. ", edge / 10, edge % 10))
}

fn defect_note(view: &casting::View) -> String {
    let mut names = Vec::new();
    for defect in &view.defects {
        let name = match defect.kind {
            DefectKind::Misrun => "недолив",
            DefectKind::SolidifiedFeed => "пробка",
            DefectKind::ShortCharge => "тигель пуст",
            DefectKind::Halted => "подача закрыта",
        };
        if !names.contains(&name) {
            names.push(name);
        }
    }
    if names.is_empty() {
        String::new()
    } else {
        format!("{}. ", names.join(", "))
    }
}

fn pixel(mm: [i32; 2]) -> (f32, f32) {
    let u = (mm[0] as f32 - X0) / (X1 - X0);
    let v = (Y1 - mm[1] as f32) / (Y1 - Y0);
    (u * W, v * H)
}

fn x_px(mm: i32) -> f32 {
    (mm as f32 - X0) / (X1 - X0) * W
}

fn extent(points: &[[i32; 2]]) -> (i32, i32) {
    let Some(first) = points.first() else {
        return (0, 0);
    };
    let mut min_x = first[0];
    let mut max_x = first[0];
    let mut min_y = first[1];
    let mut max_y = first[1];
    for point in points {
        min_x = min_x.min(point[0]);
        max_x = max_x.max(point[0]);
        min_y = min_y.min(point[1]);
        max_y = max_y.max(point[1]);
    }
    (max_x - min_x, max_y - min_y)
}

/// Scale the outline about its centre. `None` when the result leaves the 2 m sheet or shrinks under 20 mm.
fn scaled_outline(points: &[[i32; 2]], num: i32, den: i32) -> Option<Vec<[i32; 2]>> {
    if points.len() < 2 || den == 0 {
        return None;
    }
    let n = i32::try_from(points.len()).ok()?;
    let cx = points.iter().map(|point| point[0]).sum::<i32>() / n;
    let cy = points.iter().map(|point| point[1]).sum::<i32>() / n;
    let scaled: Vec<[i32; 2]> = points
        .iter()
        .map(|point| {
            [
                cx + (point[0] - cx) * num / den,
                cy + (point[1] - cy) * num / den,
            ]
        })
        .collect();
    if scaled
        .iter()
        .any(|point| point[0].abs() > 2000 || point[1].abs() > 2000)
    {
        return None;
    }
    let (length, _) = extent(&scaled);
    if length < 20 {
        return None;
    }
    Some(scaled)
}

fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::Draft => "чертёж",
        Phase::Charged => "тигель загружен",
        Phase::Hot => "металл горячий",
        Phase::Pouring => "заливка",
        Phase::Poured => "отливка готова",
    }
}

#[cfg(test)]
mod tests {
    use super::scaled_outline;

    #[test]
    fn overall_size_scales_about_the_centre_and_stays_on_the_sheet() {
        let bigger = scaled_outline(&[[0, 0], [100, 0], [100, 20], [0, 20]], 11, 10).unwrap();
        assert_eq!(bigger[0], [-5, -1]);
        assert_eq!(bigger[1], [105, -1]);
        assert!(
            scaled_outline(&[[-1900, 0], [1900, 0], [1900, 20], [-1900, 20]], 11, 10).is_none()
        );
        assert!(scaled_outline(&[[0, 0], [20, 0], [20, 10], [0, 10]], 10, 11).is_none());
    }
}
