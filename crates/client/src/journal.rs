use crate::{
    Movement,
    network::{Mode, Network},
    ui::{BUTTON, CharacterWindow, EDGE, GOLD, INK, PANEL},
};
use bevy::prelude::*;
use mortown_protocol::quests::{self, Status};
#[derive(Component)]
pub struct Toggle;
#[derive(Component)]
pub struct Overlay;
#[derive(Component)]
pub struct Details;
#[derive(Component)]
pub struct Tab(pub u8);
#[derive(Component)]
pub struct QuestRow(pub u8);
#[derive(Resource, Default)]
pub struct Filter(pub u8);
#[derive(Resource, Default)]
pub struct Selection(Option<u8>);

pub fn setup(mut commands: Commands, icons: Res<crate::actionbar::Icons>) {
    commands.init_resource::<Filter>();
    commands.init_resource::<Selection>();
    let text = |s: &str, size| crate::actionbar::label(s.to_owned(), &icons.1, size);
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
            GlobalZIndex(15),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(800),
                    max_width: percent(95),
                    padding: UiRect::all(px(24)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(18),
                    border: UiRect::all(px(1)),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(GOLD),
            ))
            .with_children(|p| {
                p.spawn(text("Журнал заданий", 23.));
                p.spawn(Node {
                    column_gap: px(10),
                    ..default()
                })
                .with_children(|tabs| {
                    for (id, title) in [(0, "Доступные"), (1, "Активные"), (2, "Выполненные")]
                    {
                        tabs.spawn((
                            Button,
                            Tab(id),
                            Node {
                                padding: UiRect::all(px(10)),
                                border: UiRect::all(px(1)),
                                ..default()
                            },
                            BackgroundColor(BUTTON),
                            BorderColor::all(EDGE),
                        ))
                        .with_children(|b| {
                            b.spawn(text(title, 14.));
                        });
                    }
                });
                p.spawn(Node {
                    column_gap: px(24),
                    min_height: px(310),
                    ..default()
                })
                .with_children(|cols| {
                    cols.spawn(Node {
                        width: px(210),
                        flex_shrink: 0.,
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        ..default()
                    })
                    .with_children(|list| {
                        for q in &quests::QUESTS {
                            list.spawn((
                                Button,
                                QuestRow(q.id),
                                crate::skin::OwnStyle,
                                Node {
                                    display: Display::None,
                                    width: percent(100),
                                    padding: UiRect::all(px(12)),
                                    border: UiRect::all(px(1)),
                                    ..default()
                                },
                                BackgroundColor(BUTTON),
                                BorderColor::all(EDGE),
                            ))
                            .with_children(|b| {
                                b.spawn(text(q.title, 14.));
                            });
                        }
                    });
                    cols.spawn((
                        Details,
                        Node {
                            flex_grow: 1.,
                            flex_basis: px(0),
                            min_width: px(0),
                            ..default()
                        },
                        text("", 14.),
                    ))
                    .insert(TextColor(INK));
                });
            });
        });
}
#[allow(clippy::too_many_arguments)]
pub fn input(
    mut state: ResMut<CharacterWindow>,
    mut filter: ResMut<Filter>,
    mut selected: ResMut<Selection>,
    network: Res<Network>,
    mut movement: ResMut<Movement>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    buttons: Query<&Interaction, (With<Toggle>, Changed<Interaction>)>,
    tabs: Query<(&Tab, &Interaction), Changed<Interaction>>,
    rows: Query<(&QuestRow, &Interaction), Changed<Interaction>>,
) {
    if network.mode != Mode::Online {
        if state.journal {
            state.open = false;
            state.journal = false;
        }
        return;
    }
    if state.mail {
        return;
    }
    if keys.just_pressed(KeyCode::KeyJ) || buttons.iter().any(|i| *i == Interaction::Pressed) {
        let open = !(state.open && state.journal);
        *state = CharacterWindow {
            open,
            journal: open,
            ..default()
        };
        *movement = Movement::default();
        if let Some(a) = network.authoritative.as_ref() {
            let current = quests::current(&a.progress);
            filter.0 = current
                .and_then(|q| q.status(&a.progress).tab())
                .unwrap_or(2);
            selected.0 = current.map(|q| q.id);
        }
    }
    if !state.open || !state.journal {
        return;
    }
    if keys.just_pressed(KeyCode::Escape) {
        state.open = false;
        state.journal = false;
        keys.clear_just_pressed(KeyCode::Escape);
        return;
    }
    if [
        KeyCode::KeyC,
        KeyCode::KeyI,
        KeyCode::KeyK,
        KeyCode::KeyM,
        KeyCode::KeyT,
    ]
    .iter()
    .any(|k| keys.just_pressed(*k))
    {
        state.open = false;
        state.journal = false;
        return;
    }
    for (tab, i) in &tabs {
        if *i == Interaction::Pressed {
            filter.0 = tab.0;
            selected.0 = None;
        }
    }
    for (row, i) in &rows {
        if *i == Interaction::Pressed {
            selected.0 = Some(row.0);
        }
    }
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn display(
    state: Res<CharacterWindow>,
    filter: Res<Filter>,
    mut selected: ResMut<Selection>,
    network: Res<Network>,
    mut overlay: Single<&mut Node, (With<Overlay>, Without<QuestRow>)>,
    mut rows: Query<
        (
            &QuestRow,
            &Interaction,
            &mut Node,
            &mut BorderColor,
            &mut BackgroundColor,
        ),
        (Without<Overlay>, Without<Tab>),
    >,
    mut details: Single<&mut Text, With<Details>>,
    mut tabs: Query<(&Tab, &mut BorderColor), Without<QuestRow>>,
) {
    overlay.display = if state.open && state.journal && network.mode == Mode::Online {
        Display::Flex
    } else {
        Display::None
    };
    let actor = network.authoritative.as_ref();
    let visible =
        |q: &quests::Quest| actor.is_some_and(|a| q.status(&a.progress).tab() == Some(filter.0));
    if !selected.0.and_then(quests::definition).is_some_and(visible) {
        selected.0 = quests::QUESTS.iter().find(|q| visible(q)).map(|q| q.id);
    }
    for (row, interaction, mut node, mut border, mut bg) in &mut rows {
        node.display = if quests::definition(row.0).is_some_and(visible) {
            Display::Flex
        } else {
            Display::None
        };
        let active = selected.0 == Some(row.0);
        *border = BorderColor::all(if active || *interaction != Interaction::None {
            GOLD
        } else {
            EDGE
        });
        bg.0 = if active {
            Color::srgb(0.16, 0.18, 0.18)
        } else {
            BUTTON
        };
    }
    for (tab, mut border) in &mut tabs {
        *border = BorderColor::all(if tab.0 == filter.0 { GOLD } else { EDGE });
    }
    details.0 = if let Some((q, a)) = selected.0.and_then(quests::definition).zip(actor) {
        let status = match q.status(&a.progress) {
            Status::Active if q.ready(&a.progress, &a.loot) => "Можно сдать торговцу",
            Status::Active => "Выполняется",
            Status::Completed => "Награда получена",
            _ => "Доступно у торговца",
        };
        format!(
            "{}\n{status}\n\n{}\n\n{}\n\n{}\n\nНаграда: {}\n\nВыдаёт и принимает: торговец у центральной площади.",
            q.title,
            q.story,
            q.hint,
            q.progress_text(&a.progress, &a.loot),
            q.reward_text()
        )
    } else {
        match filter.0 {
            0 if actor.is_some_and(|a| quests::current(&a.progress).is_none()) => {
                "Все поручения торговца завершены."
            }
            0 => {
                "Новых поручений пока нет.\nПродолжайте активное задание, чтобы открыть следующее."
            }
            1 => "Активных заданий нет.\nПоговорите с торговцем у центральной площади.",
            _ => "Здесь появятся завершённые поручения.",
        }
        .into()
    };
}
