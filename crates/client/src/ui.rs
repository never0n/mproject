use crate::{Movement, Player, Terrain, network::Network};
use bevy::prelude::*;
use mortown_protocol::{CLASSES, RACES};

// Mortown: charcoal steel, warm parchment and restrained brass.
pub const GOLD: Color = Color::srgb(0.78, 0.66, 0.43);
pub const INK: Color = Color::srgb(0.86, 0.89, 0.90);
pub const PANEL: Color = Color::srgb(0.065, 0.083, 0.10);
pub const EDGE: Color = Color::srgb(0.25, 0.29, 0.32);
pub const BUTTON: Color = Color::srgb(0.10, 0.13, 0.15);
#[derive(Component)]
pub struct Portrait;
#[derive(Component)]
pub struct Hud;
#[derive(Component)]
pub struct WorldInputBlocker;
#[derive(Resource, Default)]
pub struct CharacterWindow {
    pub open: bool,
    pub inventory: bool,
    pub trading: bool,
    pub skills: bool,
    pub mail: bool,
    pub menu: bool,
    pub journal: bool,
    pub dialogue: bool,
    pub npc: Option<u8>,
    pub alliance: bool,
}
#[derive(Component)]
pub struct CharacterOverlay;
#[derive(Component)]
pub struct CharacterDetails;
#[derive(Component)]
pub enum DetailKind {
    Battle(&'static str),
    Primary(&'static str, &'static str),
    Footer,
}
#[derive(Component)]
pub struct SpendAttribute(&'static str);
#[derive(Component)]
pub struct ToggleCharacter;

#[derive(Component)]
pub enum Readout {
    Hero,
    Level,
    Location,
    Players,
    Journal,
    Meter(usize),
}
#[derive(Component)]
pub struct Fill(usize);
const METERS: [(&str, &str, &str, Color); 4] = [
    ("Опыт", "current_exp", "exp", Color::srgb(0.53, 0.53, 0.53)),
    (
        "Здоровье",
        "current_hp",
        "hp",
        Color::srgb(0.76, 0.19, 0.19),
    ),
    (
        "Выносливость",
        "current_stamina",
        "stamina",
        Color::srgb(0.19, 0.63, 0.19),
    ),
    (
        "Мана",
        "current_mana",
        "mana",
        Color::srgb(0.19, 0.38, 0.75),
    ),
];
fn text(value: &str, font: &Handle<Font>, size: f32, color: Color) -> impl Bundle {
    (
        Text::new(value),
        TextFont {
            font: font.clone(),
            font_size: size,
            ..default()
        },
        TextColor(color),
    )
}
fn panel(left: Val, right: Val, top: Val, bottom: Val, width: Val) -> impl Bundle {
    (
        Hud,
        Node {
            position_type: PositionType::Absolute,
            left,
            right,
            top,
            bottom,
            width,
            padding: UiRect::all(px(12)),
            border: UiRect::all(px(1)),
            border_radius: BorderRadius::all(px(6)),
            flex_direction: FlexDirection::Column,
            row_gap: px(6),
            ..default()
        },
        BackgroundColor(PANEL),
        BorderColor::all(EDGE),
    )
}
pub fn setup(mut commands: Commands, mut fonts: ResMut<Assets<Font>>) {
    commands.init_resource::<CharacterWindow>();
    let font = fonts.add(
        Font::try_from_bytes(include_bytes!("../../../assets/fonts/DejaVuSans.ttf").to_vec())
            .unwrap(),
    );
    commands
        .spawn((
            panel(px(18), Val::Auto, px(18), Val::Auto, px(286)),
            crate::target::SelfPanel,
            Button,
        ))
        .with_children(|p| {
            p.spawn(Node {
                column_gap: px(12),
                align_items: AlignItems::Center,
                ..default()
            })
            .with_children(|row| {
                row.spawn(Node {
                    width: px(58),
                    height: px(74),
                    flex_shrink: 0.,
                    ..default()
                })
                .with_children(|avatar| {
                    avatar.spawn((
                        Portrait,
                        ImageNode::default(),
                        Node {
                            width: px(58),
                            height: px(68),
                            border: UiRect::all(px(1)),
                            border_radius: BorderRadius::all(px(6)),
                            ..default()
                        },
                        BorderColor::all(GOLD),
                    ));
                    avatar.spawn((
                        Readout::Level,
                        text("", &font, 12., INK),
                        Node {
                            position_type: PositionType::Absolute,
                            right: px(-3),
                            bottom: px(0),
                            padding: UiRect::axes(px(5), px(1)),
                            border_radius: BorderRadius::all(px(3)),
                            ..default()
                        },
                        BackgroundColor(BUTTON),
                    ));
                });
                row.spawn(Node {
                    flex_grow: 1.,
                    flex_direction: FlexDirection::Column,
                    row_gap: px(5),
                    ..default()
                })
                .with_children(|stats| {
                    stats.spawn((Readout::Hero, text("Путешественник", &font, 15., INK)));
                    for (i, color) in [
                        (1, Color::srgb(0.65, 0.23, 0.27)),
                        (3, Color::srgb(0.22, 0.43, 0.66)),
                        (2, Color::srgb(0.24, 0.52, 0.43)),
                    ] {
                        stats
                            .spawn((
                                Node {
                                    height: px(14),
                                    width: percent(100),
                                    border_radius: BorderRadius::all(px(2)),
                                    ..default()
                                },
                                BackgroundColor(Color::srgb(0.018, 0.025, 0.032)),
                            ))
                            .with_children(|bar| {
                                bar.spawn((
                                    Fill(i),
                                    Node {
                                        width: percent(0),
                                        height: percent(100),
                                        border_radius: BorderRadius::all(px(2)),
                                        ..default()
                                    },
                                    BackgroundColor(color),
                                ));
                                bar.spawn((
                                    Readout::Meter(i),
                                    text("", &font, 10., INK),
                                    Node {
                                        position_type: PositionType::Absolute,
                                        left: px(5),
                                        top: px(-1),
                                        ..default()
                                    },
                                ));
                            });
                    }
                });
            });
            p.spawn((Readout::Meter(0), text("", &font, 10., GOLD)));
            p.spawn((
                Node {
                    width: percent(100),
                    height: px(3),
                    ..default()
                },
                BackgroundColor(BUTTON),
            ))
            .with_children(|b| {
                b.spawn((
                    Fill(0),
                    Node {
                        height: percent(100),
                        width: percent(0),
                        ..default()
                    },
                    BackgroundColor(GOLD),
                ));
            });
        });
    commands
        .spawn(panel(Val::Auto, px(18), px(18), Val::Auto, px(244)))
        .with_children(|p| {
            p.spawn(text("М О Р Т А У Н", &font, 11., GOLD));
            p.spawn((Readout::Location, text("", &font, 14., INK)));
        });
    commands
        .spawn((
            CharacterOverlay,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            BackgroundColor(Color::srgba(0.01, 0.02, 0.03, 0.18)),
            GlobalZIndex(10),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(650),
                    max_width: percent(96),
                    padding: UiRect::all(px(24)),
                    border: UiRect::all(px(2)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(18),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(GOLD),
            ))
            .with_children(|p| {
                p.spawn(text("Характеристики", &font, 24., GOLD));
                p.spawn((CharacterDetails, text("", &font, 15., INK)));
                p.spawn(Node {
                    flex_wrap: FlexWrap::Wrap,
                    column_gap: px(6),
                    row_gap: px(6),
                    ..default()
                })
                .with_children(|row| {
                    for (key, name) in mortown_protocol::attributes::ATTRIBUTES {
                        row.spawn((
                            Button,
                            SpendAttribute(key),
                            Node {
                                width: px(185),
                                padding: UiRect::all(px(8)),
                                ..default()
                            },
                            BackgroundColor(BUTTON),
                        ))
                        .with_children(|b| {
                            b.spawn((DetailKind::Primary(key, name), text("", &font, 13., INK)));
                        });
                    }
                });

                p.spawn(Node {
                    column_gap: px(12),
                    ..default()
                })
                .with_children(|row| {
                    for (prefix, title) in [("f", "Физические"), ("m", "Магические")]
                    {
                        row.spawn((
                            Node {
                                width: percent(50),
                                padding: UiRect::all(px(14)),
                                border: UiRect::all(px(1)),
                                border_radius: BorderRadius::all(px(4)),
                                flex_direction: FlexDirection::Column,
                                row_gap: px(14),
                                ..default()
                            },
                            BorderColor::all(EDGE),
                            BackgroundColor(Color::srgb(0.025, 0.035, 0.045)),
                        ))
                        .with_children(|p| {
                            p.spawn(text(title, &font, 15., GOLD));
                            p.spawn(Node { justify_content: JustifyContent::SpaceBetween, ..default() })
                                .with_children(|row| {
                                    row.spawn(text("Атака\nЗащита\nТочность\nУклонение\nСкорость атаки\nШанс крита\nСила крита", &font, 14., INK));
                                    row.spawn((DetailKind::Battle(prefix), text("", &font, 14., INK), TextLayout::new_with_justify(Justify::Right)));
                                });
                        });
                    }
                });
                p.spawn((DetailKind::Footer, text("", &font, 12., GOLD)));
            });
        });
}
pub fn update(
    world: Res<Terrain>,
    player: Single<&Transform, With<Player>>,
    movement: Res<Movement>,
    network: Res<Network>,
    mut panels: Query<&mut Node, (With<Hud>, Without<Fill>)>,
    mut labels: Query<(&Readout, &mut Text)>,
    mut fills: Query<(&Fill, &mut Node), Without<Hud>>,
) {
    for mut node in &mut panels {
        node.display = if network.playing() {
            Display::Flex
        } else {
            Display::None
        };
    }
    let c = network.character.as_ref();
    let stat = |key: &str| c.and_then(|c| c.stats.get(key)).copied().unwrap_or(0.);
    let p = player.translation;
    let location = if let Some(id) = world.0.hall_at(p.x, p.z) {
        world.0.halls[id].name
    } else if (p.x - mortown_world::PLAZA[0]).hypot(p.z - mortown_world::PLAZA[1]) < 85. {
        "Центральная площадь"
    } else if mortown_world::inside_city([p.x, p.z]) {
        "Кварталы Мортауна"
    } else {
        "Мортаунские холмы"
    };
    for (kind, mut label) in &mut labels {
        label.0 = match kind {
            Readout::Hero => c
                .map(|c| c.nickname.clone())
                .unwrap_or("Путешественник".into()),
            Readout::Level => format!("{:.0}", stat("level").max(1.)),
            Readout::Location => location.into(),
            Readout::Players => {
                if c.is_some() {
                    format!("Игроков в мире: {}", network.roster.len())
                } else {
                    "Осмотр мира".into()
                }
            }
            Readout::Journal => format!(
                "{}\n{}\n{}",
                network
                    .authoritative
                    .as_ref()
                    .map(|a| a.reward_status.as_str())
                    .unwrap_or(""),
                if movement.status.is_empty() {
                    "Исследуйте Мортаун и окрестности"
                } else {
                    &movement.status
                },
                if c.is_some() {
                    &network.status
                } else {
                    "Прогресс осмотра не сохраняется"
                }
            ),
            Readout::Meter(i) => {
                let (name, current, max, _) = METERS[*i];
                if c.is_some() {
                    format!(
                        "{}  {:.0} / {:.0}",
                        if *i == 0 {
                            "Опыт"
                        } else {
                            match *i {
                                1 => "HP",
                                2 => "SP",
                                _ => "MP",
                            }
                        },
                        stat(current),
                        stat(max)
                    )
                } else {
                    format!("{name}  —")
                }
            }
        };
    }
    for (Fill(i), mut node) in &mut fills {
        let (_, current, max, _) = METERS[*i];
        node.width = percent(if stat(max) > 0. {
            (stat(current) / stat(max) * 100.).clamp(0., 100.) as f32
        } else {
            0.
        });
    }
}
#[allow(clippy::type_complexity)]
pub fn button_feedback(
    mut buttons: Query<
        (&Interaction, &mut BackgroundColor, &mut BorderColor),
        (
            With<Button>,
            Changed<Interaction>,
            Without<crate::skin::OwnStyle>,
        ),
    >,
) {
    for (state, mut bg, mut edge) in &mut buttons {
        bg.0 = match state {
            Interaction::None => BUTTON,
            Interaction::Hovered => Color::srgb(0.16, 0.21, 0.24),
            Interaction::Pressed => Color::srgb(0.25, 0.29, 0.29),
        };
        *edge = BorderColor::all(if *state == Interaction::None {
            EDGE
        } else {
            GOLD
        });
    }
}

pub fn spend_attribute(
    state: Res<CharacterWindow>,
    mut network: ResMut<Network>,
    buttons: Query<(&SpendAttribute, &Interaction), Changed<Interaction>>,
) {
    if !state.open
        || state.inventory
        || state.trading
        || state.skills
        || state.mail
        || state.menu
        || state.journal
        || state.dialogue
        || state.alliance
        || network.pending
        || network.mode != crate::network::Mode::Online
        || network
            .character
            .as_ref()
            .is_none_or(|c| c.stats.get("stat_point").copied().unwrap_or(0.) < 1.)
    {
        return;
    }
    for (SpendAttribute(key), interaction) in &buttons {
        if *interaction == Interaction::Pressed {
            if let Some(expected) = network
                .character
                .as_ref()
                .and_then(|c| c.stats.get(*key))
                .copied()
            {
                network.request(mortown_protocol::ClientMessage::SpendAttribute {
                    stat: (*key).into(),
                    expected: expected as u32,
                });
            }
            break;
        }
    }
}

pub fn character_window(
    mut state: ResMut<CharacterWindow>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    buttons: Query<&Interaction, (With<ToggleCharacter>, Changed<Interaction>)>,
    network: Res<Network>,
    mut movement: ResMut<Movement>,
    mut overlay: Single<&mut Node, With<CharacterOverlay>>,
    mut details: Single<&mut Text, With<CharacterDetails>>,
) {
    if (state.mail || state.menu || state.journal || state.dialogue || state.alliance)
        && network.playing()
    {
        overlay.display = Display::None;
        return;
    }
    if !network.playing() {
        state.open = false;
    } else if state.open && keys.just_pressed(KeyCode::Escape) {
        state.open = false;
        keys.clear_just_pressed(KeyCode::Escape);
    } else if keys.just_pressed(KeyCode::KeyC) || buttons.iter().any(|i| *i == Interaction::Pressed)
    {
        state.open = !state.open || state.inventory || state.trading || state.skills;
        state.inventory = false;
        state.trading = false;
        state.skills = false;
        if state.open {
            movement.path.clear();
            movement.pending = None;
            movement.destination = None;
            movement.status = "Маршрут остановлен".into();
        }
    }
    overlay.display = if state.open && !state.inventory && !state.trading && !state.skills {
        Display::Flex
    } else {
        Display::None
    };
    if !state.open || state.inventory || state.trading || state.skills {
        return;
    }
    let Some(c) = network.character.as_ref() else {
        details.0="Сейчас открыт осмотр мира без персонажа.\n\nВойдите в аккаунт и выберите героя, чтобы увидеть\nего характеристики, ресурсы и боевые показатели.".into();
        return;
    };
    let v = |k: &str| c.stats.get(k).copied().unwrap_or(0.);
    details.0 = format!(
        "{} · Уровень {:.0}\n{} · {}",
        c.nickname,
        v("level"),
        RACES[(c.race - 1) as usize],
        CLASSES[(c.class - 1) as usize]
    );
}
pub fn battle_details(c: &mortown_protocol::Character, prefix: &str) -> String {
    let n = |key: &str| {
        c.stats
            .get(&format!("{prefix}_{key}"))
            .copied()
            .unwrap_or(0.)
    };
    format!(
        "{:.0}\n{:.0}\n{:.0}\n{:.0}\n{:.0}\n{:.1}%\n×{:.2}",
        n("attack"),
        n("defence"),
        n("accuracy"),
        n("evasion"),
        n("attack_speed"),
        n("crit_chance") * 100.,
        n("crit_power")
    )
}
pub fn character_cards(network: Res<Network>, mut labels: Query<(&DetailKind, &mut Text)>) {
    let Some(c) = &network.character else { return };
    let v = |k: &str| c.stats.get(k).copied().unwrap_or(0.);
    for (kind, mut text) in &mut labels {
        let value = match kind {
            DetailKind::Battle(prefix) => battle_details(c, prefix),
            DetailKind::Primary(key, name) => format!(
                "{}   {:.0}{}",
                name,
                v(key),
                if v("stat_point") >= 1. { "   +" } else { "" }
            ),
            DetailKind::Footer => format!(
                "Очки характеристик  {:.0}     ·     Очки навыков  {:.0}",
                v("stat_point"),
                v("sp")
            ),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn character_window_consumes_escape_and_closes_when_leaving_world() {
        let mut app = App::new();
        let mut network = Network::default();
        network.mode = crate::network::Mode::Offline;
        app.insert_resource(network)
            .init_resource::<Movement>()
            .init_resource::<CharacterWindow>()
            .init_resource::<ButtonInput<KeyCode>>()
            .add_systems(Update, character_window);
        app.world_mut().spawn((CharacterOverlay, Node::default()));
        app.world_mut().spawn((CharacterDetails, Text::new("")));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::KeyC);
        app.update();
        assert!(app.world().resource::<CharacterWindow>().open);
        app.world_mut().resource_mut::<Network>().character = Some(mortown_protocol::Character {
            id: 1,
            slot: 1,
            nickname: "Проверка".into(),
            race: 1,
            class: 1,
            gender: 1,
            stats: mortown_protocol::starting_stats(1, 1).unwrap(),
            inventory: mortown_protocol::items::starter(1),
            loot: Default::default(),
        });
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.update();
        let details = app
            .world_mut()
            .query_filtered::<&Text, With<CharacterDetails>>()
            .single(app.world())
            .unwrap();
        assert!(details.0.contains("Проверка"));
        let battle = battle_details(
            app.world()
                .resource::<Network>()
                .character
                .as_ref()
                .unwrap(),
            "f",
        );
        assert!(battle.ends_with("12.0%\n×1.58"));
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Escape);
        app.update();
        assert!(!app.world().resource::<CharacterWindow>().open);
        assert!(
            !app.world()
                .resource::<ButtonInput<KeyCode>>()
                .just_pressed(KeyCode::Escape)
        );
        app.world_mut().resource_mut::<CharacterWindow>().open = true;
        app.world_mut().resource_mut::<Network>().mode = crate::network::Mode::Selection;
        app.update();
        assert!(!app.world().resource::<CharacterWindow>().open);
    }
}
