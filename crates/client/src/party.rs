use crate::{
    actionbar::{Icons, label},
    network::{Mode, Network},
    ui::{BUTTON, GOLD, INK, PANEL},
};
use bevy::prelude::*;
use mortown_protocol::party::Command;
#[derive(Resource, Default)]
pub struct Selection {
    group: Option<u64>,
    member: Option<i64>,
    world_target: Option<i64>,
}
#[derive(Component)]
pub struct Root;
#[derive(Component)]
pub struct Invite;
#[derive(Component)]
pub struct Content;
#[derive(Component)]
pub struct Details;
#[derive(Component)]
pub struct DetailsText;
#[derive(Component)]
pub struct MemberText(pub usize);
#[derive(Component, Clone, Copy)]
pub enum Action {
    Info,
    Accept,
    Decline,
    Leave,
    Kick,
    Promote,
    Member(usize),
}
pub fn setup(mut commands: Commands, icons: Res<Icons>) {
    commands.init_resource::<Selection>();
    commands
        .spawn((
            Root,
            crate::ui::WorldInputBlocker,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                left: px(12),
                top: px(270),
                width: px(260),
                padding: UiRect::all(px(15)),
                flex_direction: FlexDirection::Column,
                row_gap: px(6),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(GOLD),
            GlobalZIndex(6),
        ))
        .with_children(|p| {
            p.spawn(Node {
                column_gap: px(8),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|p| {
                p.spawn((
                    Content,
                    label("", &icons.1, 13.),
                    Node {
                        flex_grow: 1.,
                        flex_basis: px(0),
                        ..default()
                    },
                ));
                p.spawn((
                    Button,
                    Action::Info,
                    Node {
                        width: px(24),
                        height: px(24),
                        flex_shrink: 0.,
                        align_items: AlignItems::Center,
                        justify_content: JustifyContent::Center,
                        border_radius: BorderRadius::all(px(12)),
                        ..default()
                    },
                    BackgroundColor(BUTTON),
                ))
                .with_children(|p| {
                    p.spawn(label("i", &icons.1, 14.));
                });
            });
            p.spawn((
                Details,
                Node {
                    display: Display::None,
                    position_type: PositionType::Absolute,
                    left: percent(100),
                    top: px(0),
                    width: px(350),
                    padding: UiRect::all(px(14)),
                    border: UiRect::all(px(1)),
                    border_radius: BorderRadius::all(px(5)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(GOLD),
                bevy::ui::FocusPolicy::Pass,
                GlobalZIndex(32),
            ))
            .with_children(|p| {
                p.spawn((
                    DetailsText,
                    label("", &icons.1, 13.),
                    bevy::ui::FocusPolicy::Pass,
                ));
            });
            for i in 0..mortown_protocol::party::LIMIT {
                p.spawn((
                    Button,
                    Action::Member(i),
                    crate::skin::OwnStyle,
                    Node {
                        padding: UiRect::all(px(5)),
                        ..default()
                    },
                    BackgroundColor(BUTTON),
                ))
                .with_children(|b| {
                    b.spawn((MemberText(i), label("", &icons.1, 13.)));
                });
            }
            for (action, name) in [
                (Action::Accept, "Принять приглашение"),
                (Action::Decline, "Отклонить"),
                (Action::Leave, "Выйти из группы"),
                (Action::Kick, "Исключить выбранного"),
                (Action::Promote, "Передать лидерство выбранному"),
            ] {
                p.spawn((
                    Button,
                    action,
                    Node {
                        padding: UiRect::all(px(5)),
                        ..default()
                    },
                    BackgroundColor(BUTTON),
                ))
                .with_children(|b| {
                    b.spawn(label(name, &icons.1, 12.));
                });
            }
        });
}
pub fn input(
    mut network: ResMut<Network>,
    mut target: ResMut<crate::combat::CombatUi>,
    mut selection: ResMut<Selection>,
    invites: Query<&Interaction, (With<Invite>, Changed<Interaction>)>,
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
) {
    let group_id = network.party.group.as_ref().map(|g| g.id);
    if group_id != selection.group {
        *selection = Selection {
            group: group_id,
            ..default()
        };
    }
    if selection.member.is_some_and(|id| {
        !network
            .party
            .group
            .as_ref()
            .is_some_and(|g| g.members.iter().any(|m| m.id == id))
    }) {
        selection.member = None;
    }
    if selection.world_target != target.player {
        selection.world_target = target.player;
        if let Some(id) = target.player {
            selection.member = network
                .party
                .group
                .as_ref()
                .and_then(|g| g.members.iter().find(|m| m.id == id))
                .map(|m| m.id);
        }
    }
    if network.mode != Mode::Online || network.party_pending {
        return;
    }
    if invites.iter().any(|i| *i == Interaction::Pressed)
        && let Some(player) = target.player
    {
        network.party_request(Command::Invite { player });
        return;
    }
    let Some((_, action)) = buttons.iter().find(|(i, _)| **i == Interaction::Pressed) else {
        return;
    };
    let group = network.party.group.clone();
    let invitation = network.party.invitation.clone();
    match action {
        Action::Info => {}
        Action::Accept => {
            if let Some(i) = invitation {
                network.party_request(Command::Accept { invitation: i.id });
            }
        }
        Action::Decline => {
            if let Some(i) = invitation {
                network.party_request(Command::Decline { invitation: i.id });
            }
        }
        Action::Leave => {
            if let Some(g) = group {
                network.party_request(Command::Leave { party: g.id });
            }
        }
        Action::Kick => {
            if let Some(g) = group
                && let Some(player) = selection.member
            {
                network.party_request(Command::Kick {
                    party: g.id,
                    player,
                });
            }
        }
        Action::Promote => {
            if let Some(g) = group
                && let Some(player) = selection.member
            {
                network.party_request(Command::Promote {
                    party: g.id,
                    player,
                });
            }
        }
        Action::Member(index) => {
            if let Some(m) = group.as_ref().and_then(|g| g.members.get(*index)) {
                selection.member = Some(m.id);
                // Offline rows are selectable for group management, never as world targets.
                if m.online {
                    target.selected = None;
                    target.player = Some(m.id);
                    selection.world_target = Some(m.id);
                    network.action(mortown_protocol::ClientMessage::Attack { target: None });
                }
            }
        }
    }
}
pub fn display(
    network: Res<Network>,
    target: Res<crate::combat::CombatUi>,
    selection: Res<Selection>,
    time: Res<Time>,
    mut root: Single<
        &mut Node,
        (
            With<Root>,
            Without<Action>,
            Without<Invite>,
            Without<Details>,
        ),
    >,
    mut invite: Single<
        &mut Node,
        (
            With<Invite>,
            Without<Root>,
            Without<Action>,
            Without<Details>,
        ),
    >,
    mut buttons: Query<
        (&Action, &Interaction, &mut Node, &mut BackgroundColor),
        (Without<Root>, Without<Invite>, Without<Details>),
    >,
    mut content: Single<&mut Text, (With<Content>, Without<MemberText>, Without<DetailsText>)>,
    mut members: Query<
        (&MemberText, &mut Text, &mut TextColor),
        (Without<Content>, Without<DetailsText>),
    >,
    mut details: Single<
        &mut Node,
        (
            With<Details>,
            Without<Root>,
            Without<Invite>,
            Without<Action>,
        ),
    >,
    mut rules: Single<&mut Text, (With<DetailsText>, Without<Content>, Without<MemberText>)>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    scale: Res<UiScale>,
    frame: Single<(&ComputedNode, &UiGlobalTransform), With<Root>>,
) {
    let online = network.mode == Mode::Online;
    let group = network.party.group.as_ref();
    let incoming = network.party.invitation.as_ref();
    let owner = network.character.as_ref().map(|c| c.id);
    let leader = group.is_some_and(|g| Some(g.leader) == owner);
    root.display = if online && (group.is_some() || incoming.is_some()) {
        Display::Flex
    } else {
        Display::None
    };
    invite.display = if online
        && !network.party_pending
        && incoming.is_none()
        && (group.is_none() || leader)
        && target.player.is_some_and(|id| {
            Some(id) != owner && !group.is_some_and(|g| g.members.iter().any(|m| m.id == id))
        }) {
        Display::Flex
    } else {
        Display::None
    };
    content.0 = if let Some(i) = incoming {
        format!(
            "{} приглашает вас в группу\nПриглашение действует 60 секунд",
            i.inviter
        )
    } else {
        format!(
            "Группа · {}/5 · в игре {}\n{}",
            group.map_or(0, |g| g.members.len()),
            group.map_or(0, |g| g.members.iter().filter(|m| m.online).count()),
            network.party.message
        )
    };
    let mut show_details = false;
    for (action, interaction, mut node, mut bg) in &mut buttons {
        let visible = match action {
            Action::Info => {
                show_details = group.is_some() && *interaction != Interaction::None;
                group.is_some()
            }
            Action::Accept | Action::Decline => incoming.is_some(),
            Action::Leave => group.is_some(),
            Action::Member(i) => group.is_some_and(|g| g.members.get(*i).is_some()),
            Action::Kick | Action::Promote => {
                leader
                    && selection.member.is_some_and(|id| {
                        Some(id) != owner
                            && group.is_some_and(|g| {
                                g.members.iter().any(|m| {
                                    m.id == id && (matches!(action, Action::Kick) || m.online)
                                })
                            })
                    })
            }
        };
        if let Action::Member(n) = action {
            let selected = group
                .and_then(|g| g.members.get(*n))
                .is_some_and(|m| Some(m.id) == selection.member);
            bg.0 = if selected {
                Color::srgb(0.18, 0.23, 0.27)
            } else if *interaction == Interaction::Hovered {
                Color::srgb(0.13, 0.17, 0.20)
            } else {
                BUTTON
            };
        }
        node.display = if visible && (!network.party_pending || matches!(action, Action::Info)) {
            Display::Flex
        } else {
            Display::None
        };
    }
    details.display = if online && show_details {
        Display::Flex
    } else {
        Display::None
    };
    rules.0 = format!(
        "НАГРАДЫ В ГРУППЕ\n\nГруппа добившего врага получает награду. Учитываются живые участники в {} м от врага на момент его гибели.\n\nОпыт и монеты — поровну, остаток переходит по очереди. Предметы — по очереди.\n\nПосле выхода из мира место сохраняется {} мин. Вернитесь тем же персонажем. Лидерство переходит участнику в игре.\n\nПоследняя добыча:\n{}",
        mortown_protocol::party::REWARD_RANGE as u32,
        mortown_protocol::party::RECONNECT_SECONDS / 60,
        group
            .map(|g| g.last_loot.as_str())
            .filter(|s| !s.is_empty())
            .unwrap_or("Пока нет")
    );
    if show_details {
        let center = frame.1.transform_point2(Vec2::ZERO) * frame.0.inverse_scale_factor();
        let size = frame.0.size() * frame.0.inverse_scale_factor();
        let available = window.width() / scale.0;
        let left = if center.x + size.x / 2. + 358. <= available {
            size.x + 8.
        } else {
            -358.
        };
        details.left = px(left.max(8. - (center.x - size.x / 2.)));
        let rows: usize = rules
            .0
            .lines()
            .map(|l| l.chars().count().div_ceil(38).max(1))
            .sum();
        let height = rows as f32 * 17. + 28.;
        let top = center.y - size.y / 2.;
        details.top = px((window.height() / scale.0 - top - height - 8.)
            .min(0.)
            .max(8. - top));
    }
    let elapsed = (time.elapsed_secs_f64() - network.party_received_at).max(0.) as u32;
    for (index, mut text, mut color) in &mut members {
        let member = group.and_then(|g| g.members.get(index.0));
        color.0 = if member.is_some_and(|m| !m.online) {
            Color::srgb(0.53, 0.58, 0.63)
        } else {
            INK
        };
        let value = group
            .and_then(|g| {
                g.members.get(index.0).map(|m| {
                    let level = network
                        .roster
                        .iter()
                        .find(|a| a.id == m.id)
                        .map_or(m.level, |a| a.progress.level);
                    let mut text = format!(
                        "{}{} · ур. {}",
                        if g.leader == m.id { "★ " } else { "" },
                        m.nickname,
                        level
                    );
                    if !m.online {
                        let seconds = m.reconnect_seconds.saturating_sub(elapsed);
                        text.push_str(&format!(
                            "\nНе в мире · ожидание {}:{:02}",
                            seconds / 60,
                            seconds % 60
                        ));
                    }
                    text
                })
            })
            .unwrap_or_default();
        if text.0 != value {
            text.0 = value;
        }
    }
}
