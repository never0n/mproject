//! Native chat command dialogs share the game's movable window chrome.
//! The server opens them (`Reply::Dialog` → `Log::dialog`): help, ignore list, confirm,
//! game/chat settings, admin panel. `input` rebuilds the whole window whenever the dialog
//! (or the admin board) changes; settings buttons act locally on the window and lights.
use super::*;
use bevy::window::{MonitorSelection, PresentMode, PrimaryWindow, WindowMode};
use mortown_protocol::chat::{Dialog, DialogKind};
/// Chat look; client-only and not saved (reset on restart).
#[derive(Resource)]
pub struct Settings {
    pub font: f32,
    pub width: f32,
    pub opacity: f32,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            font: 12.,
            width: 300.,
            opacity: 0.94,
        }
    }
}
/// What is currently spawned: overlay root, inner panel, and the dialog it was built from.
#[derive(Resource, Default)]
pub struct State {
    entity: Option<Entity>,
    panel: Option<Entity>,
    rendered: Option<Dialog>,
}
/// Dialog buttons; `Font`/`Width` carry the step to add.
#[derive(Component, Clone, Copy)]
pub enum DialogAction {
    Close,
    Confirm,
    Unignore(i64),
    Fullscreen,
    Vsync,
    Shadows,
    Font(f32),
    Width(f32),
    Opacity,
}
#[derive(Component)]
pub struct DialogScroll;
#[derive(Component)]
pub struct SettingsText;
#[derive(Component)]
pub struct FeedbackText;
pub fn setup(mut commands: Commands) {
    commands.init_resource::<Settings>();
    commands.init_resource::<State>();
}
fn control(action: DialogAction) -> impl Bundle {
    (
        Button,
        action,
        crate::skin::OwnStyle,
        Node {
            padding: UiRect::axes(px(12), px(8)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(crate::ui::BUTTON),
        BorderColor::all(crate::ui::EDGE),
    )
}
/// Runs before `chat::input`; sets `Form::modal_capture` so the chat yields the keyboard.
pub fn input(
    mut commands: Commands,
    mut network: ResMut<Network>,
    mut form: ResMut<Form>,
    mut state: ResMut<State>,
    mut board: ResMut<super::admin::Board>,
    mut settings: ResMut<Settings>,
    icons: Res<Icons>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut wheel: ResMut<AccumulatedMouseScroll>,
    buttons: Query<(&DialogAction, &Interaction), Changed<Interaction>>,
    mut scrolls: Query<
        (&ComputedNode, &UiGlobalTransform, &mut ScrollPosition),
        With<DialogScroll>,
    >,
    mut labels: Query<&mut Text, (With<SettingsText>, Without<FeedbackText>)>,
    mut feedback: Query<&mut Text, (With<FeedbackText>, Without<SettingsText>)>,
    mut window: Single<&mut Window, With<PrimaryWindow>>,
    mut lights: Query<&mut DirectionalLight>,
    geometry: Query<&Node, With<crate::movable::Movable>>,
) {
    let open = network.chat.dialog.is_some() && network.mode == Mode::Online;
    if open {
        form.modal_capture = true;
        if let Some(p) = window.physical_cursor_position() {
            for (node, transform, mut scroll) in &mut scrolls {
                if crate::actionbar::contains(node, transform, p) {
                    scroll.y = (scroll.y
                        - wheel.delta.y
                            * if wheel.unit == MouseScrollUnit::Line {
                                28.
                            } else {
                                1.
                            })
                    .max(0.);
                }
            }
        }
        wheel.delta = Vec2::ZERO;
        let action = if keys.just_pressed(KeyCode::Escape) {
            Some(DialogAction::Close)
        } else {
            buttons
                .iter()
                .find(|(_, i)| **i == Interaction::Pressed)
                .map(|(a, _)| *a)
        };
        match action {
            Some(DialogAction::Close) => network.chat.dialog = None,
            // Dangerous commands come back as Confirm with a one-time token; sending it executes.
            Some(DialogAction::Confirm) if !network.chat.pending => {
                if let Some(dialog) = network.chat.dialog.take()
                    && let Some(token) = dialog.token
                {
                    network.chat_request(Command::Confirm { token });
                }
            }
            Some(DialogAction::Unignore(id)) if !network.chat.pending => {
                network.chat_request(Command::Execute {
                    text: format!("/unignore {id}"),
                });
            }
            Some(DialogAction::Fullscreen) => {
                window.mode = if window.mode == WindowMode::Windowed {
                    WindowMode::BorderlessFullscreen(MonitorSelection::Current)
                } else {
                    WindowMode::Windowed
                }
            }
            Some(DialogAction::Vsync) => {
                window.present_mode = if window.present_mode == PresentMode::AutoNoVsync {
                    PresentMode::AutoVsync
                } else {
                    PresentMode::AutoNoVsync
                }
            }
            Some(DialogAction::Shadows) => {
                let enabled = lights.iter().any(|l| l.shadows_enabled);
                for mut light in &mut lights {
                    light.shadows_enabled = !enabled;
                }
            }
            Some(DialogAction::Font(delta)) => {
                settings.font = (settings.font + delta).clamp(11., 18.)
            }
            Some(DialogAction::Width(delta)) => {
                settings.width = (settings.width + delta).clamp(260., 460.)
            }
            Some(DialogAction::Opacity) => {
                settings.opacity = if settings.opacity > 0.8 { 0.65 } else { 0.94 }
            }
            _ => {}
        }
        // Keys held while closing (e.g. Escape) must not reach the game.
        if matches!(action, Some(DialogAction::Close)) {
            form.suppressed.extend(keys.get_pressed().copied());
            keys.reset_all();
        }
    }
    for mut text in &mut feedback {
        if text.0 != network.chat.status {
            text.0 = network.chat.status.clone();
        }
    }
    if network.mode != Mode::Online {
        network.chat.dialog = None;
    }
    let admin_changed = network
        .chat
        .dialog
        .as_ref()
        .is_some_and(|d| d.kind == DialogKind::Admin)
        && board.revision != board.rendered;
    // Rebuild on change. Same dialog kind → keep the dragged position and scroll offset.
    if state.rendered != network.chat.dialog || admin_changed {
        let keep = state
            .rendered
            .as_ref()
            .zip(network.chat.dialog.as_ref())
            .is_some_and(|(a, b)| a.kind == b.kind);
        let placement = if keep {
            state
                .panel
                .and_then(|p| geometry.get(p).ok())
                .map(|n| (n.left, n.right, n.top, n.bottom))
        } else {
            None
        };
        let scroll_y = if keep {
            scrolls.iter().next().map_or(0., |(_, _, s)| s.y)
        } else {
            0.
        };
        let mut next_panel = None;
        board.rendered = board.revision;
        if let Some(entity) = state.entity.take() {
            commands.entity(entity).despawn();
        }
        state.rendered = network.chat.dialog.clone();
        if let Some(dialog) = &state.rendered {
            let entity = commands
                .spawn((
                    Button,
                    crate::ui::WorldInputBlocker,
                    crate::skin::OwnStyle,
                    Node {
                        position_type: PositionType::Absolute,
                        width: percent(100),
                        height: percent(100),
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        ..default()
                    },
                    BackgroundColor(Color::srgba(0., 0., 0., 0.35)),
                    // Topmost layer: above the ⚙ menu (40).
                    GlobalZIndex(80),
                ))
                .with_children(|overlay| {
                    // Spawned at runtime, so it adds its own Movable + Handle
                    // (`movable::setup` only sees windows that exist at startup).
                    let mut panel = overlay.spawn((
                        crate::movable::Movable,
                        crate::ui::WorldInputBlocker,
                        Node {
                            width: px(if dialog.kind == DialogKind::Admin {
                                930.
                            } else {
                                610.
                            }),
                            max_width: percent(94),
                            left: placement.map_or(Val::Auto, |p| p.0),
                            right: placement.map_or(Val::Auto, |p| p.1),
                            top: placement.map_or(Val::Auto, |p| p.2),
                            bottom: placement.map_or(Val::Auto, |p| p.3),
                            padding: UiRect::all(px(20)),
                            border: UiRect::all(px(1)),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(14),
                            border_radius: BorderRadius::all(px(8)),
                            ..default()
                        },
                        BackgroundColor(crate::ui::PANEL),
                        BorderColor::all(crate::ui::EDGE),
                    ));
                    let panel_id = panel.id();
                    next_panel = Some(panel_id);
                    panel.with_children(|p| {
                        p.spawn((
                            Button,
                            crate::movable::Handle(panel_id),
                            crate::skin::OwnStyle,
                            Node {
                                width: percent(100),
                                padding: UiRect::bottom(px(10)),
                                border: UiRect::bottom(px(1)),
                                ..default()
                            },
                            BackgroundColor(Color::NONE),
                            BorderColor::all(crate::ui::EDGE),
                        ))
                        .with_children(|h| {
                            h.spawn(label(&dialog.title, &icons.1, 22.))
                                .insert(TextColor(crate::ui::GOLD));
                        });
                        p.spawn((
                            DialogScroll,
                            ScrollPosition(Vec2::new(0., scroll_y)),
                            Node {
                                max_height: px(if dialog.kind == DialogKind::Admin {
                                    510.
                                } else {
                                    360.
                                }),
                                min_height: px(40),
                                overflow: Overflow::scroll_y(),
                                flex_direction: FlexDirection::Column,
                                row_gap: px(8),
                                ..default()
                            },
                        ))
                        .with_children(|body| {
                            if let Some(view) = &dialog.admin {
                                super::admin::content(body, &icons, view, &board);
                            }
                            if !dialog.text.is_empty() {
                                body.spawn((
                                    label(&dialog.text, &icons.1, 14.),
                                    Node {
                                        flex_shrink: 0.,
                                        ..default()
                                    },
                                ));
                            }
                            if dialog.kind == DialogKind::IgnoreList {
                                if dialog.entries.is_empty() {
                                    body.spawn(label("Список пуст", &icons.1, 15.));
                                }
                                for (id, name) in &dialog.entries {
                                    body.spawn(Node {
                                        width: percent(100),
                                        flex_shrink: 0.,
                                        align_items: AlignItems::Center,
                                        justify_content: JustifyContent::SpaceBetween,
                                        ..default()
                                    })
                                    .with_children(|row| {
                                        row.spawn(label(
                                            &format!("{name} · ID {id}"),
                                            &icons.1,
                                            15.,
                                        ));
                                        row.spawn(control(DialogAction::Unignore(*id)))
                                            .with_children(|b| {
                                                b.spawn(label("×", &icons.1, 20.)).insert(
                                                    TextColor(Color::srgb(0.95, 0.35, 0.3)),
                                                );
                                            });
                                    });
                                }
                            }
                            if matches!(
                                dialog.kind,
                                DialogKind::GameSettings | DialogKind::ChatSettings
                            ) {
                                body.spawn((SettingsText, label("", &icons.1, 15.)));
                                let controls = if dialog.kind == DialogKind::GameSettings {
                                    vec![
                                        (DialogAction::Fullscreen, "Окно / полный экран"),
                                        (DialogAction::Vsync, "Вертикальная синхронизация"),
                                        (DialogAction::Shadows, "Тени"),
                                    ]
                                } else {
                                    vec![
                                        (DialogAction::Font(-1.), "Шрифт −"),
                                        (DialogAction::Font(1.), "Шрифт +"),
                                        (DialogAction::Width(-20.), "Уже"),
                                        (DialogAction::Width(20.), "Шире"),
                                        (DialogAction::Opacity, "Прозрачность фона"),
                                    ]
                                };
                                body.spawn(Node {
                                    flex_wrap: FlexWrap::Wrap,
                                    column_gap: px(8),
                                    row_gap: px(8),
                                    flex_shrink: 0.,
                                    ..default()
                                })
                                .with_children(|row| {
                                    for (action, name) in controls {
                                        row.spawn(control(action)).with_children(|b| {
                                            b.spawn(label(name, &icons.1, 14.));
                                        });
                                    }
                                });
                                body.spawn(label(
                                    "Настройки действуют до закрытия клиента.",
                                    &icons.1,
                                    12.,
                                ));
                            }
                        });
                        p.spawn((FeedbackText, label("", &icons.1, 13.)))
                            .insert(TextColor(Color::srgb(0.95, 0.64, 0.36)));
                        p.spawn(Node {
                            justify_content: JustifyContent::End,
                            column_gap: px(10),
                            ..default()
                        })
                        .with_children(|row| {
                            if dialog.kind == DialogKind::Confirm {
                                row.spawn(control(DialogAction::Confirm))
                                    .with_children(|b| {
                                        b.spawn(label("Подтвердить", &icons.1, 15.))
                                            .insert(TextColor(crate::ui::GOLD));
                                    });
                            }
                            row.spawn(control(DialogAction::Close)).with_children(|b| {
                                b.spawn(label(
                                    if dialog.kind == DialogKind::Confirm {
                                        "Отмена"
                                    } else {
                                        "Закрыть"
                                    },
                                    &icons.1,
                                    15.,
                                ));
                            });
                        });
                    });
                })
                .id();
            state.entity = Some(entity);
            state.panel = next_panel;
        }
    }
    // Settings summary is refreshed every frame from the live window/lights state.
    if let Some(dialog) = &network.chat.dialog {
        for mut text in &mut labels {
            text.0 = if dialog.kind == DialogKind::GameSettings {
                format!(
                    "Режим: {}\nVSync: {} · Тени: {}",
                    if window.mode == WindowMode::Windowed {
                        "оконный"
                    } else {
                        "полный экран"
                    },
                    if window.present_mode == PresentMode::AutoNoVsync {
                        "выкл."
                    } else {
                        "вкл."
                    },
                    if lights.iter().any(|l| l.shadows_enabled) {
                        "вкл."
                    } else {
                        "выкл."
                    }
                )
            } else {
                format!(
                    "Шрифт: {:.0} · Ширина: {:.0}\nНепрозрачность фона: {:.0}%",
                    settings.font,
                    settings.width,
                    settings.opacity * 100.
                )
            };
        }
    }
}
