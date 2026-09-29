use crate::{
    Movement, Terrain,
    network::{Mode, Network},
    ui::{BUTTON, CharacterWindow, GOLD, PANEL},
};
use bevy::prelude::*;
use mortown_protocol::{
    ClientMessage,
    quests::{self, Status},
};
#[derive(Component)]
pub struct Overlay;
#[derive(Component)]
pub struct ServiceButton(crate::npcs::Service);
#[derive(Component)]
pub struct Heading;
#[derive(Component)]
pub struct QuestJournal;
#[derive(Component)]
pub struct Close;
#[derive(Component)]
pub struct Speech;
#[derive(Component)]
pub struct Feedback;
#[derive(Component)]
pub struct QuestAction {
    quest: u8,
    complete: bool,
}
pub fn open(
    npc: u8,
    state: &mut CharacterWindow,
    network: &mut Network,
    movement: &mut Movement,
    world: &mortown_world::World,
) {
    if !crate::npcs::available(network, world, npc) {
        network.status = crate::npcs::resolve(network, npc).map_or_else(
            || "Собеседник недоступен".into(),
            |n| format!("Подойдите ближе: {} · {}", n.name, n.role.title()),
        );
        return;
    }
    network.status.clear();
    *state = CharacterWindow {
        open: true,
        dialogue: true,
        npc: Some(npc),
        ..default()
    };
    *movement = Movement::default();
    network.action(mortown_protocol::ClientMessage::Attack { target: None });
}
pub fn setup(mut commands: Commands, icons: Res<crate::actionbar::Icons>) {
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
            GlobalZIndex(14),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(620),
                    max_width: percent(94),
                    padding: UiRect::all(px(24)),
                    border: UiRect::all(px(1)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(12),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(GOLD),
            ))
            .with_children(|p| {
                p.spawn((Heading, crate::actionbar::label("", &icons.1, 23.)));
                p.spawn((Speech, crate::actionbar::label("", &icons.1, 15.)));
                for q in &quests::QUESTS {
                    for complete in [false, true] {
                        let caption = if !complete {
                            "Принять поручение"
                        } else if matches!(q.objective, quests::Objective::Supplies(_)) {
                            "Передать припасы и получить награду"
                        } else {
                            "Получить награду"
                        };
                        p.spawn((
                            Button,
                            QuestAction {
                                quest: q.id,
                                complete,
                            },
                            Node {
                                display: Display::None,
                                padding: UiRect::all(px(12)),
                                border: UiRect::all(px(1)),
                                ..default()
                            },
                            BackgroundColor(BUTTON),
                            BorderColor::all(GOLD),
                        ))
                        .with_children(|b| {
                            b.spawn(crate::actionbar::label(caption, &icons.1, 15.));
                        });
                    }
                }
                p.spawn((Feedback, crate::actionbar::label("", &icons.1, 13.)));
                p.spawn((
                    Button,
                    crate::journal::Toggle,
                    QuestJournal,
                    Node {
                        padding: UiRect::all(px(10)),
                        ..default()
                    },
                    BackgroundColor(BUTTON),
                ))
                .with_children(|b| {
                    b.spawn(crate::actionbar::label("Посмотреть журнал", &icons.1, 14.));
                });
                use crate::npcs::Service;
                for (service, title) in [
                    (Service::Shop, "Открыть магазин"),
                    (Service::Recipes, "Изготовление"),
                    (Service::Professions, "Мастерство и специализация"),
                    (Service::Mail, "Открыть почту"),
                    (Service::Alliance, "Дела альянса"),
                ] {
                    p.spawn((
                        Button,
                        ServiceButton(service),
                        Node {
                            display: Display::None,
                            padding: UiRect::all(px(12)),
                            ..default()
                        },
                        BackgroundColor(BUTTON),
                    ))
                    .with_children(|b| {
                        b.spawn(crate::actionbar::label(title, &icons.1, 16.));
                    });
                }
                p.spawn((
                    Button,
                    Close,
                    Node {
                        padding: UiRect::all(px(10)),
                        ..default()
                    },
                    BackgroundColor(BUTTON),
                ))
                .with_children(|b| {
                    b.spawn(crate::actionbar::label("До встречи", &icons.1, 14.));
                });
            });
        });
}
pub fn input(
    mut state: ResMut<CharacterWindow>,
    mut network: ResMut<Network>,
    world: Res<Terrain>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    close: Query<&Interaction, (With<Close>, Changed<Interaction>)>,
    services: Query<(&ServiceButton, &Interaction), Changed<Interaction>>,
    quests: Query<(&QuestAction, &Interaction), Changed<Interaction>>,
    mut requests: MessageWriter<crate::npcs::ServiceRequest>,
) {
    if !state.open || !state.dialogue {
        return;
    }
    let escape = keys.just_pressed(KeyCode::Escape);
    let switch = [
        KeyCode::KeyC,
        KeyCode::KeyI,
        KeyCode::KeyK,
        KeyCode::KeyM,
        KeyCode::KeyJ,
        KeyCode::KeyT,
    ]
    .iter()
    .any(|k| keys.just_pressed(*k));
    if !state
        .npc
        .is_some_and(|id| crate::npcs::available(&network, &world.0, id))
        || escape
        || switch
        || close.iter().any(|i| *i == Interaction::Pressed)
    {
        state.dialogue = false;
        state.open = false;
        if escape {
            keys.clear_just_pressed(KeyCode::Escape);
        }
        return;
    }
    for (button, interaction) in &services {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let request = crate::npcs::ServiceRequest {
            npc: state.npc.unwrap(),
            service: button.0,
        };
        if request.valid(&network, &world.0) {
            requests.write(request);
            *state = default();
            return;
        }
    }
    if network.pending {
        return;
    }
    if state.npc != Some(mortown_world::npcs::PLAZA_TRADER) {
        return;
    }
    for (action, interaction) in &quests {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let valid = network.authoritative.as_ref().is_some_and(|a| {
            quests::definition(action.quest).is_some_and(|q| {
                if action.complete {
                    q.ready(&a.progress, &a.loot)
                } else {
                    q.status(&a.progress) == Status::Available
                }
            })
        });
        if valid {
            network.request(ClientMessage::Quest {
                quest: action.quest,
                complete: action.complete,
            });
            break;
        }
    }
}
#[allow(clippy::type_complexity)]
pub fn display(
    state: Res<CharacterWindow>,
    network: Res<Network>,
    mut nodes: Query<
        (
            &mut Node,
            Option<&Overlay>,
            Option<&QuestAction>,
            Option<&ServiceButton>,
            Option<&QuestJournal>,
        ),
        Or<(
            With<Overlay>,
            With<QuestAction>,
            With<ServiceButton>,
            With<QuestJournal>,
        )>,
    >,
    mut speech: Single<&mut Text, (With<Speech>, Without<Feedback>, Without<Heading>)>,
    mut feedback: Single<&mut Text, (With<Feedback>, Without<Speech>, Without<Heading>)>,
    mut heading: Single<&mut Text, (With<Heading>, Without<Speech>, Without<Feedback>)>,
) {
    let npc = state.npc.and_then(|id| crate::npcs::resolve(&network, id));
    let quest_giver = npc.is_some_and(|n| n.id == mortown_world::npcs::PLAZA_TRADER);
    heading.0 = npc.map_or_else(String::new, |n| format!("{} · {}", n.name, n.role.title()));
    let actor = network.authoritative.as_ref();
    for (mut node, overlay, quest, service, journal) in &mut nodes {
        let show = if overlay.is_some() {
            state.open && state.dialogue && network.mode == Mode::Online && npc.is_some()
        } else if let Some(service) = service {
            npc.is_some_and(|n| crate::npcs::offers(&n, service.0))
        } else if journal.is_some() {
            quest_giver
        } else if let Some(action) = quest {
            quest_giver
                && !network.pending
                && actor.is_some_and(|a| {
                    quests::definition(action.quest).is_some_and(|q| {
                        if action.complete {
                            q.ready(&a.progress, &a.loot)
                        } else {
                            q.status(&a.progress) == Status::Available
                        }
                    })
                })
        } else {
            false
        };
        node.display = if show { Display::Flex } else { Display::None };
    }
    feedback.0.clone_from(&network.status);
    if !quest_giver {
        speech.0 = npc.map_or_else(String::new, |n| n.greeting.into());
        return;
    }
    speech.0 = if let Some((q, a)) =
        actor.and_then(|a| quests::current(&a.progress).map(|q| (q, a)))
    {
        let ready = q.ready(&a.progress, &a.loot);
        let words = if ready { q.thanks } else { q.story };
        let detail = if q.status(&a.progress) == Status::Available {
            q.hint.to_string()
        } else {
            format!(
                "{}\n{}",
                q.progress_text(&a.progress, &a.loot),
                if ready {
                    "Всё готово к сдаче."
                } else {
                    q.hint
                }
            )
        };
        format!(
            "{}\n\n{words}\n\n{detail}\n\nНаграда: {}",
            q.title,
            q.reward_text()
        )
    } else {
        "Караван отправился в путь, а у вас за плечами первое ремесленное дело. Спасибо за помощь, путник! Заглядывайте за припасами — моя лавка всегда открыта.".into()
    };
}
