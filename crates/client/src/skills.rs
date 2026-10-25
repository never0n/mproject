use crate::{
    Movement,
    network::{Mode, Network},
    ui::{BUTTON, CharacterWindow, GOLD, INK, PANEL},
};
use bevy::prelude::*;
use mortown_protocol::{ClientMessage, skills};
#[derive(Component)]
pub struct Overlay;
#[derive(Component)]
pub struct Details;
#[derive(Resource, Default)]
pub struct Training(pub bool);
#[derive(Component, Clone, Copy)]
pub enum Action {
    Open,
    Training,
    SpendTalent(&'static str),
    /// 0 return, 1 guard, 2 work.
    Orders(u8),
}
pub fn setup(
    mut commands: Commands,
    mut fonts: ResMut<Assets<Font>>,
    icons: Res<crate::actionbar::Icons>,
) {
    commands.init_resource::<Training>();
    let font = fonts.add(
        Font::try_from_bytes(include_bytes!("../../../assets/fonts/DejaVuSans.ttf").to_vec())
            .unwrap(),
    );
    commands
        .spawn((
            Overlay,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.02, 0.03, 0.16)),
            GlobalZIndex(12),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(480),
                    max_width: percent(96),
                    padding: UiRect::all(px(24)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(12),
                    ..default()
                },
                BackgroundColor(PANEL),
            ))
            .with_children(|p| {
                p.spawn((
                    Text::new("Навыки"),
                    TextFont {
                        font: font.clone(),
                        font_size: 22.,
                        ..default()
                    },
                    TextColor(GOLD),
                ));
                p.spawn(Node {
                    display: Display::Grid,
                    grid_template_columns: RepeatedGridTrack::px(6, 66.),
                    column_gap: px(7),
                    row_gap: px(7),
                    ..default()
                })
                .with_children(|p| {
                    for n in 1..=skills::LAST_SKILL {
                        crate::actionbar::slot(p, crate::actionbar::Slot::Skill(n), &icons, 66.);
                    }
                });
                p.spawn((
                    Details,
                    Text::new(""),
                    TextFont {
                        font: font.clone(),
                        font_size: 16.,
                        ..default()
                    },
                    TextColor(INK),
                ));
                p.spawn((
                    Action::Training,
                    Button,
                    Node {
                        padding: UiRect::all(px(8)),
                        ..default()
                    },
                    BackgroundColor(BUTTON),
                ))
                .with_children(|b| {
                    b.spawn((
                        Text::new("Развитие"),
                        TextFont {
                            font: font.clone(),
                            font_size: 15.,
                            ..default()
                        },
                        TextColor(GOLD),
                    ));
                });
                for (kind, name) in [
                    (0, "Приказ: вернуться"),
                    (1, "Приказ: охранять"),
                    (2, "Приказ: работать"),
                ] {
                    p.spawn((
                        Action::Orders(kind),
                        Button,
                        Node {
                            padding: UiRect::all(px(8)),
                            ..default()
                        },
                        BackgroundColor(BUTTON),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(name),
                            TextFont {
                                font: font.clone(),
                                font_size: 15.,
                                ..default()
                            },
                            TextColor(GOLD),
                        ));
                    });
                }
                for (key, name) in mortown_protocol::progression::TALENTS {
                    p.spawn((
                        Action::SpendTalent(key),
                        Button,
                        Node {
                            padding: UiRect::all(px(8)),
                            ..default()
                        },
                        BackgroundColor(BUTTON),
                    ))
                    .with_children(|b| {
                        b.spawn((
                            Text::new(format!("Талант: {name}")),
                            TextFont {
                                font: font.clone(),
                                font_size: 15.,
                                ..default()
                            },
                            TextColor(GOLD),
                        ));
                    });
                }
            });
        });
}
pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    mut window: ResMut<CharacterWindow>,
    mut network: ResMut<Network>,
    mut movement: ResMut<Movement>,
    mut training: ResMut<Training>,
    buttons: Query<(&Action, &Interaction), Changed<Interaction>>,
) {
    if network.mode != Mode::Online {
        return;
    }
    let action = if keys.just_pressed(KeyCode::KeyK) {
        Some(Action::Open)
    } else {
        buttons
            .iter()
            .find(|(_, i)| **i == Interaction::Pressed)
            .map(|(a, _)| *a)
    };
    match action {
        Some(Action::Open) => {
            training.0 = false;
            window.open = !window.open || !window.skills;
            window.inventory = false;
            window.skills = true;
            *movement = Movement::default();
        }
        Some(Action::Training) if window.open && window.skills => training.0 = !training.0,
        Some(Action::Orders(kind)) if window.open && window.skills && !network.pending => {
            if let Some(actor) = network.authoritative.as_ref() {
                let mut orders = actor.orders;
                match kind {
                    0 => orders.returns = !orders.returns,
                    1 => orders.guards = !orders.guards,
                    _ => orders.works = !orders.works,
                }
                network.request(ClientMessage::OfflineOrders {
                    returns: orders.returns,
                    guards: orders.guards,
                    works: orders.works,
                });
            }
        }
        Some(Action::SpendTalent(talent)) if window.open && window.skills && !network.pending => {
            if let Some(c) = network.character.as_ref() {
                let expected = mortown_protocol::progression::talent_rank(&c.stats, talent);
                network.request(ClientMessage::SpendTalent {
                    talent: talent.into(),
                    expected,
                });
            }
        }
        _ => {}
    }
}
#[allow(clippy::type_complexity)]
pub fn display(
    window: Res<CharacterWindow>,
    network: Res<Network>,
    mut overlay: Single<&mut Node, (With<Overlay>, Without<Action>)>,
    mut actions: Query<(&Action, &mut Node), Without<Overlay>>,
    mut details: Single<(&mut Text, &mut Node), (With<Details>, Without<Action>, Without<Overlay>)>,
    training: Res<Training>,
) {
    overlay.display = if window.open && window.skills && network.mode == Mode::Online {
        Display::Flex
    } else {
        Display::None
    };
    let Some(a) = network.authoritative.as_ref() else {
        return;
    };
    for (action, mut node) in &mut actions {
        let show = match action {
            Action::SpendTalent(_) => training.0 && a.progress.talent_points >= 1.,
            _ => true,
        };
        node.display = if show { Display::Flex } else { Display::None };
    }
    details.1.display = if training.0 {
        Display::Flex
    } else {
        Display::None
    };
    let lines: Vec<String> = (1..=skills::LAST_SKILL)
        .map(|id| {
            let uses = a.progress.practice[usize::from(id - 1)];
            let state = if skills::learned(&a.progress, id) {
                "открыт"
            } else {
                "закрыт"
            };
            format!("Навык {id}: {state}, применений {uses}")
        })
        .collect();
    details.0.0 = format!(
        "Уровень {} · характеристики {:.0} · таланты {:.0} · жизни {}\n{}\nУмения растут от применения. Талант тратится только очками за уровень.\nОффлайн: вернуться {} · охранять {} · работать {}\n{}",
        a.progress.level,
        a.progress.stat_points,
        a.progress.talent_points,
        a.lives,
        lines.join("\n"),
        if a.orders.returns { "да" } else { "нет" },
        if a.orders.guards { "да" } else { "нет" },
        if a.orders.works { "да" } else { "нет" },
        network.status
    );
}
