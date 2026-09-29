use crate::{
    Movement,
    actionbar::{Icons, label},
    network::{Mode, Network},
    ui::{BUTTON, CharacterWindow, EDGE, GOLD, PANEL},
};
use bevy::{
    input::{ButtonState, keyboard::KeyboardInput},
    prelude::*,
};
use mortown_protocol::alliance::{self, Command, Role};
#[derive(Resource, Default)]
pub struct Form {
    pub warehouse: bool,
    owner: Option<i64>,
    alliance: Option<i64>,
    name: String,
    focused: bool,
    selected: Option<i64>,
    confirm: Option<Confirmation>,
    suppressed: std::collections::HashSet<KeyCode>,
}
struct Confirmation {
    command: Command,
    message: String,
}
#[derive(Component)]
pub struct Overlay;
#[derive(Component)]
pub struct Notice;
#[derive(Component)]
pub struct NoticeText;
#[derive(Component)]
pub struct Summary;
#[derive(Component)]
pub struct Status;
#[derive(Component)]
pub struct Selection;
#[derive(Component)]
pub struct ConfirmText;
#[derive(Component)]
pub struct FieldText;
#[derive(Component)]
pub struct MemberText(usize);
#[derive(Component)]
pub struct Roster;
#[derive(Component)]
pub struct Scroll;
#[derive(Component, Clone, Copy)]
pub enum Action {
    Open,
    Warehouse,
    Field,
    Create,
    Invite,
    Accept,
    Decline,
    Member(usize),
    Refresh,
    Leave,
    Kick,
    Promote,
    Demote,
    Transfer,
    Dissolve,
    Confirm,
    Cancel,
}
fn button(a: Action) -> impl Bundle {
    (
        Button,
        a,
        Node {
            padding: UiRect::axes(px(12), px(8)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(4)),
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(EDGE),
    )
}
pub fn setup(mut commands: Commands, icons: Res<Icons>) {
    commands.init_resource::<Form>();
    commands
        .spawn((
            Overlay,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            GlobalZIndex(19),
            BackgroundColor(Color::srgba(0.01, 0.02, 0.03, 0.16)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(760),
                    max_width: percent(96),
                    padding: UiRect::all(px(24)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(14),
                    ..default()
                },
                BackgroundColor(PANEL),
            ))
            .with_children(|p| {
                p.spawn(label("Альянс", &icons.1, 23.));
                p.spawn((Summary, label("", &icons.1, 15.)));
                p.spawn((button(Action::Field), crate::skin::OwnStyle))
                    .with_children(|p| {
                        p.spawn((FieldText, label("", &icons.1, 14.)));
                    });
                p.spawn(Node {
                    column_gap: px(8),
                    flex_wrap: FlexWrap::Wrap,
                    row_gap: px(8),
                    ..default()
                })
                .with_children(|p| {
                    for (a, name) in [
                        (Action::Create, "Создать · 1000 монет"),
                        (Action::Invite, "Пригласить"),
                        (Action::Accept, "Принять приглашение"),
                        (Action::Decline, "Отклонить"),
                        (Action::Refresh, "Обновить"),
                        (Action::Warehouse, "Склад альянса"),
                    ] {
                        p.spawn(button(a)).with_children(|p| {
                            p.spawn(label(name, &icons.1, 13.));
                        });
                    }
                });
                p.spawn((
                    Roster,
                    Node {
                        column_gap: px(18),
                        ..default()
                    },
                ))
                .with_children(|row| {
                    row.spawn((
                        Button,
                        crate::skin::OwnStyle,
                        Scroll,
                        ScrollPosition::default(),
                        Node {
                            width: px(360),
                            height: px(280),
                            flex_shrink: 0.,
                            overflow: Overflow::scroll_y(),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(4),
                            ..default()
                        },
                        BackgroundColor(Color::NONE),
                    ))
                    .with_children(|list| {
                        for n in 0..alliance::LIMIT {
                            list.spawn((button(Action::Member(n)), crate::skin::OwnStyle))
                                .with_children(|p| {
                                    p.spawn((MemberText(n), label("", &icons.1, 13.)));
                                });
                        }
                    });
                    row.spawn(Node {
                        flex_grow: 1.,
                        flex_basis: px(0),
                        min_width: px(0),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        ..default()
                    })
                    .with_children(|p| {
                        p.spawn((Selection, label("", &icons.1, 14.)));
                        for (a, name) in [
                            (Action::Promote, "Назначить офицером"),
                            (Action::Demote, "Снять звание офицера"),
                            (Action::Kick, "Исключить"),
                            (Action::Transfer, "Передать лидерство"),
                            (Action::Leave, "Выйти из альянса"),
                            (Action::Dissolve, "Распустить альянс"),
                        ] {
                            p.spawn(button(a)).with_children(|p| {
                                p.spawn(label(name, &icons.1, 13.));
                            });
                        }
                    });
                });
                p.spawn((ConfirmText, label("", &icons.1, 14.)));
                p.spawn(Node {
                    column_gap: px(8),
                    ..default()
                })
                .with_children(|p| {
                    for (a, name) in [(Action::Confirm, "Подтвердить"), (Action::Cancel, "Отмена")]
                    {
                        p.spawn(button(a)).with_children(|p| {
                            p.spawn(label(name, &icons.1, 13.));
                        });
                    }
                });
                p.spawn((Status, label("", &icons.1, 12.)));
            });
        });
    commands
        .spawn((
            Notice,
            crate::ui::WorldInputBlocker,
            Button,
            Action::Open,
            BackgroundColor(PANEL),
            BorderColor::all(EDGE),
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                right: px(18),
                top: px(100),
                width: px(264),
                padding: UiRect::all(px(14)),
                border: UiRect::all(px(1)),
                ..default()
            },
            GlobalZIndex(7),
        ))
        .with_children(|p| {
            p.spawn((NoticeText, label("", &icons.1, 13.)));
        });
}
pub fn input(
    mut form: ResMut<Form>,
    mut network: ResMut<Network>,
    mut panels: ResMut<CharacterWindow>,
    mut movement: ResMut<Movement>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut events: MessageReader<KeyboardInput>,
    buttons: Query<(&Action, &Interaction), Changed<Interaction>>,
    mut wheel: ResMut<bevy::input::mouse::AccumulatedMouseScroll>,
    mut scroll: Single<(&ComputedNode, &UiGlobalTransform, &mut ScrollPosition), With<Scroll>>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    mut services: MessageReader<crate::npcs::ServiceRequest>,
    world: Res<crate::Terrain>,
) {
    let at_steward = services
        .read()
        .copied()
        .any(|r| r.service == crate::npcs::Service::Alliance && r.valid(&network, &world.0));
    let inputs: Vec<_> = events.read().cloned().collect();
    for e in &inputs {
        if e.state == ButtonState::Released {
            form.suppressed.remove(&e.key_code);
        }
    }
    for key in &form.suppressed {
        keys.reset(*key);
    }
    let owner = network.character.as_ref().map(|c| c.id);
    let id = network.alliance.alliance.as_ref().map(|a| a.id);
    if owner != form.owner || id != form.alliance {
        let suppressed = std::mem::take(&mut form.suppressed);
        *form = Form {
            owner,
            alliance: id,
            suppressed,
            ..default()
        };
    }
    if network.mode != Mode::Online {
        panels.alliance = false;
        return;
    }
    let pressed = buttons
        .iter()
        .find(|(_, i)| **i == Interaction::Pressed)
        .map(|(a, _)| *a);
    if at_steward
        || matches!(pressed, Some(Action::Open))
        || !form.focused && !panels.mail && keys.just_pressed(KeyCode::KeyL)
    {
        let open = at_steward || !panels.alliance || !panels.open;
        *panels = CharacterWindow {
            open,
            alliance: open,
            ..default()
        };
        form.focused = false;
        form.confirm = None;
        form.warehouse = false;
        *movement = default();
        keys.reset(KeyCode::KeyL);
        if open {
            network.alliance_request(Command::Refresh);
        }
        return;
    }
    if !panels.open || !panels.alliance {
        form.focused = false;
        form.confirm = None;
        return;
    }
    if let Some(a) = &network.alliance.alliance
        && form
            .selected
            .is_some_and(|id| !a.members.iter().any(|m| m.id == id))
    {
        form.selected = None;
        form.confirm = None;
    }
    let capturing = form.focused;
    if capturing {
        form.suppressed.extend(keys.get_pressed().copied());
    }
    if keys.just_pressed(KeyCode::Escape) {
        if form.confirm.take().is_none() {
            *panels = default();
        }
        form.focused = false;
        if capturing {
            keys.reset_all();
        } else {
            keys.reset(KeyCode::Escape);
        }
        return;
    }
    if !form.focused
        && [
            KeyCode::KeyC,
            KeyCode::KeyI,
            KeyCode::KeyK,
            KeyCode::KeyM,
            KeyCode::KeyJ,
            KeyCode::KeyT,
        ]
        .iter()
        .any(|k| keys.just_pressed(*k))
    {
        *panels = default();
        form.confirm = None;
        return;
    }
    if form.warehouse {
        return;
    }
    if wheel.delta.y != 0.
        && window
            .physical_cursor_position()
            .is_some_and(|p| crate::actionbar::contains(scroll.0, scroll.1, p))
    {
        let max = ((scroll.0.content_size().y - scroll.0.size().y)
            * scroll.0.inverse_scale_factor())
        .max(0.);
        let step = if wheel.unit == bevy::input::mouse::MouseScrollUnit::Line {
            28.
        } else {
            1.
        };
        scroll.2.y = (scroll.2.y - wheel.delta.y * step).clamp(0., max);
        wheel.delta = Vec2::ZERO;
    }
    if let Some(action) = pressed {
        if matches!(action, Action::Field) && form.confirm.is_none() {
            form.focused = true;
        } else if !network.alliance_pending {
            form.focused = false;
            if matches!(action, Action::Cancel) {
                form.confirm = None;
            } else if matches!(action, Action::Confirm) {
                if let Some(c) = form.confirm.take() {
                    network.alliance_request(c.command);
                }
            } else if form.confirm.is_none() {
                let a = network.alliance.alliance.clone();
                let incoming = network.alliance.invitation.clone();
                let selected = a
                    .as_ref()
                    .and_then(|a| a.members.iter().find(|m| Some(m.id) == form.selected))
                    .cloned();
                let command = match action {
                    Action::Warehouse => {
                        form.warehouse = a.is_some();
                        Some(Command::Refresh)
                    }
                    Action::Refresh => Some(Command::Refresh),
                    Action::Accept => incoming.map(|i| Command::Accept { invitation: i.id }),
                    Action::Decline => incoming.map(|i| Command::Decline { invitation: i.id }),
                    Action::Invite => a.as_ref().map(|a| Command::Invite {
                        alliance: a.id,
                        nickname: form.name.trim().into(),
                    }),
                    Action::Create => {
                        match alliance::validate_name(&form.name) {
                            Ok(name) => {
                                form.confirm = Some(Confirmation {
                                    command: Command::Create { name: name.into() },
                                    message: format!(
                                        "Создать «{name}» за 1000 монет? Плата не возвращается."
                                    ),
                                });
                            }
                            Err(e) => network.alliance.message = e.into(),
                        }
                        None
                    }
                    Action::Member(n) => {
                        form.selected = a.as_ref().and_then(|a| a.members.get(n)).map(|m| m.id);
                        None
                    }
                    Action::Promote | Action::Demote => {
                        a.as_ref()
                            .zip(selected.as_ref())
                            .map(|(a, m)| Command::SetOfficer {
                                alliance: a.id,
                                player: m.id,
                                officer: matches!(action, Action::Promote),
                            })
                    }
                    Action::Leave | Action::Dissolve | Action::Kick | Action::Transfer => {
                        if let Some(a) = a {
                            let pending = match action {
                                Action::Leave => Some((
                                    Command::Leave { alliance: a.id },
                                    format!("Выйти из альянса «{}»?", a.name),
                                )),
                                Action::Dissolve => Some((
                                    Command::Dissolve { alliance: a.id },
                                    format!(
                                        "Распустить «{}»? Весь состав будет удалён. 1000 монет не возвращаются.",
                                        a.name
                                    ),
                                )),
                                Action::Kick => selected.map(|m| {
                                    (
                                        Command::Kick {
                                            alliance: a.id,
                                            player: m.id,
                                        },
                                        format!("Исключить {} из альянса?", m.nickname),
                                    )
                                }),
                                _ => selected.map(|m| {
                                    (
                                        Command::Transfer {
                                            alliance: a.id,
                                            player: m.id,
                                        },
                                        format!(
                                            "Передать лидерство {}? Вы станете офицером.",
                                            m.nickname
                                        ),
                                    )
                                }),
                            };
                            form.confirm =
                                pending.map(|(command, message)| Confirmation { command, message });
                        }
                        None
                    }
                    _ => None,
                };
                if let Some(command) = command {
                    network.alliance_request(command);
                }
            }
        }
    }
    if form.focused && !network.alliance_pending {
        let limit = if id.is_some() { 16 } else { 24 };
        for e in &inputs {
            if e.state != ButtonState::Pressed {
                continue;
            }
            match e.key_code {
                KeyCode::Backspace => {
                    form.name.pop();
                }
                KeyCode::Enter | KeyCode::NumpadEnter => {
                    form.focused = false;
                }
                _ => {
                    if let Some(text) = &e.text {
                        for c in text.chars() {
                            if !c.is_control() && form.name.chars().count() < limit {
                                form.name.push(c);
                            }
                        }
                    }
                }
            }
        }
    }
    if capturing || form.focused {
        form.suppressed.extend(keys.get_pressed().copied());
        keys.reset_all();
        *movement = default();
    }
}
#[allow(clippy::type_complexity)]
pub fn display(
    network: Res<Network>,
    panels: Res<CharacterWindow>,
    form: Res<Form>,
    mut overlay: Single<&mut Node, (With<Overlay>, Without<Action>, Without<Roster>)>,
    mut roster: Single<&mut Node, (With<Roster>, Without<Overlay>, Without<Action>)>,
    mut buttons: Query<
        (
            &Action,
            &mut Node,
            &mut BackgroundColor,
            &mut BorderColor,
            Has<Notice>,
        ),
        (Without<Overlay>, Without<Roster>),
    >,
    mut texts: Query<(
        &mut Text,
        Option<&MemberText>,
        Has<Summary>,
        Has<Status>,
        Has<Selection>,
        Has<ConfirmText>,
        Has<FieldText>,
        Has<NoticeText>,
    )>,
) {
    let online = network.mode == Mode::Online;
    let shown = online && panels.open && panels.alliance;
    overlay.display = if shown && !form.warehouse {
        Display::Flex
    } else {
        Display::None
    };
    let own = network.character.as_ref().map(|c| c.id).unwrap_or(0);
    let a = network.alliance.alliance.as_ref();
    let role = a.and_then(|a| a.role(own));
    let selected = a.and_then(|a| a.members.iter().find(|m| Some(m.id) == form.selected));
    let incoming = network.alliance.invitation.as_ref();
    let confirmation = form.confirm.is_some();
    roster.display = if a.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    for (action, mut node, mut bg, mut edge, notice) in &mut buttons {
        let show = if notice {
            online && !shown && incoming.is_some()
        } else {
            match action {
                Action::Field => {
                    !confirmation && (a.is_none() || role.is_some_and(Role::can_invite))
                }
                Action::Create => a.is_none() && !confirmation,
                Action::Invite => {
                    a.is_some() && !confirmation && role.is_some_and(Role::can_invite)
                }
                Action::Accept | Action::Decline => incoming.is_some() && !confirmation,
                Action::Member(n) => a.is_some_and(|a| a.members.get(*n).is_some()),
                Action::Refresh => !confirmation,
                Action::Warehouse => a.is_some() && !confirmation,
                Action::Leave => role.is_some_and(|r| r != Role::Leader) && !confirmation,
                Action::Dissolve => role == Some(Role::Leader) && !confirmation,
                Action::Kick => {
                    !confirmation
                        && selected.is_some_and(|m| {
                            m.id != own && role.is_some_and(|r| r.can_kick(m.role))
                        })
                }
                Action::Promote => {
                    !confirmation
                        && role == Some(Role::Leader)
                        && selected.is_some_and(|m| m.role == Role::Member)
                }
                Action::Demote => {
                    !confirmation
                        && role == Some(Role::Leader)
                        && selected.is_some_and(|m| m.role == Role::Officer)
                }
                Action::Transfer => {
                    !confirmation
                        && role == Some(Role::Leader)
                        && selected.is_some_and(|m| m.id != own)
                }
                Action::Confirm | Action::Cancel => confirmation,
                Action::Open => true,
            }
        };
        node.display = if show { Display::Flex } else { Display::None };
        if matches!(action, Action::Field | Action::Member(_)) {
            let active = matches!(action, Action::Field) && form.focused
                || matches!(action,Action::Member(n) if a.and_then(|a|a.members.get(*n)).is_some_and(|m|Some(m.id)==form.selected));
            *edge = BorderColor::all(if active { GOLD } else { EDGE });
            bg.0 = if active {
                Color::srgb(0.16, 0.21, 0.24)
            } else {
                BUTTON
            };
            node.flex_shrink = 0.;
        }
    }
    for (mut text, member, summary, status, selection, confirm, field, notice) in &mut texts {
        let value = if let Some(MemberText(n)) = member {
            a.and_then(|a| a.members.get(*n))
                .map(|m| {
                    format!(
                        "{}  {} · ур. {}\n{}",
                        if m.online { "●" } else { "○" },
                        m.nickname,
                        m.level,
                        m.role.label()
                    )
                })
                .unwrap_or_default()
        } else if summary {
            if let Some(a) = a {
                format!(
                    "{} · {}/{} участников · в игре {}\nВаша роль: {}",
                    a.name,
                    a.members.len(),
                    alliance::LIMIT,
                    a.members.iter().filter(|m| m.online).count(),
                    role.map_or("Участник", Role::label)
                )
            } else if let Some(i) = incoming {
                format!(
                    "{} приглашает в «{}»\nОсталось около {} сек.",
                    i.inviter, i.name, i.seconds_left
                )
            } else {
                "Вы пока не состоите в альянсе.\nСоздание: 1000 монет · до 20 участников.".into()
            }
        } else if status {
            if network.alliance_pending {
                "Отправляем…".into()
            } else {
                network.alliance.message.clone()
            }
        } else if selection {
            selected
                .map(|m| {
                    format!(
                        "{}\n{} · {}",
                        m.nickname,
                        m.role.label(),
                        if m.online {
                            "В игре"
                        } else {
                            "Не в сети"
                        }
                    )
                })
                .unwrap_or("Выберите участника".into())
        } else if confirm {
            form.confirm
                .as_ref()
                .map(|c| c.message.clone())
                .unwrap_or_default()
        } else if field {
            format!(
                "{}: {}{}",
                if a.is_some() {
                    "Имя приглашаемого"
                } else {
                    "Название альянса"
                },
                form.name,
                if form.focused { "│" } else { "" }
            )
        } else if notice {
            incoming
                .map(|i| format!("Приглашение в «{}»\nОткрыть альянс", i.name))
                .unwrap_or_default()
        } else {
            continue;
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}
