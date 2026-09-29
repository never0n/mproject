use crate::{
    actionbar::{Icons, contains, item_icon, label},
    network::{Mode, Network},
    skin::OwnStyle,
    ui::{BUTTON, CharacterWindow, EDGE, GOLD, PANEL},
};
use bevy::{prelude::*, ui::FocusPolicy, window::PrimaryWindow};
use mortown_protocol::{
    alliance::{self, Command, Role},
    items, layout,
    mail::AttachmentRequest,
    transfer,
};

const PAGE: usize = 25;
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Bag,
    Storage,
}
impl Side {
    fn index(self) -> usize {
        usize::from(self == Self::Storage)
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Source {
    Bag(u64),
    Storage(i64),
}
impl Source {
    fn side(self) -> Side {
        match self {
            Self::Bag(_) => Side::Bag,
            Self::Storage(_) => Side::Storage,
        }
    }
}
#[derive(Clone, Copy)]
struct Item {
    source: Source,
    kind: u8,
    quantity: u32,
}
#[derive(Clone, Copy)]
struct Held {
    item: Item,
    start: Vec2,
}
#[derive(Resource)]
pub struct Form {
    owner: Option<(i64, i64)>,
    revision: i64,
    bag: Vec<(u16, u64, u32)>,
    pages: [usize; 2],
    selected: Option<Source>,
    quantity: u32,
    held: Option<Held>,
    history: bool,
}
impl Default for Form {
    fn default() -> Self {
        Self {
            owner: None,
            revision: -1,
            bag: vec![],
            pages: [0, 0],
            selected: None,
            quantity: 1,
            held: None,
            history: false,
        }
    }
}
#[derive(Component)]
pub struct Overlay;
#[derive(Component)]
pub struct Grid(Side);
#[derive(Component, Clone, Copy)]
pub enum Action {
    Back,
    History,
    Refresh,
    Page(Side, i32),
    Cell(Side, usize),
    Less,
    More,
    All,
    Move,
}
#[derive(Component)]
pub enum Section {
    Root,
    Grids,
    History,
    Controls,
}
#[derive(Component)]
pub enum Caption {
    Summary,
    Page(Side),
    Count(Side, usize),
    Selection,
    Quantity,
    Status,
    History,
    HistoryButton,
    Move,
}
#[derive(Component)]
pub struct CellIcon(Side, usize);
#[derive(Component)]
pub struct Tip;
#[derive(Component)]
pub struct TipText;
#[derive(Component)]
pub struct Ghost;
fn button(action: Action) -> impl Bundle {
    (
        Button,
        action,
        Node {
            padding: UiRect::axes(px(12), px(7)),
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
            Section::Root,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: percent(100),
                height: percent(100),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(19),
            BackgroundColor(Color::srgba(0.01, 0.02, 0.03, 0.16)),
        ))
        .with_children(|root| {
            root.spawn((
                Node {
                    width: px(810),
                    max_width: percent(96),
                    padding: UiRect::all(px(24)),
                    row_gap: px(12),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                BackgroundColor(PANEL),
            ))
            .with_children(|p| {
                p.spawn(label("Склад альянса", &icons.1, 23.));
                p.spawn((Caption::Summary, label("", &icons.1, 13.)));
                p.spawn(Node {
                    column_gap: px(8),
                    ..default()
                })
                .with_children(|p| {
                    p.spawn(button(Action::Back)).with_children(|p| {
                        p.spawn(label("‹ Состав альянса", &icons.1, 13.));
                    });
                    p.spawn(button(Action::History)).with_children(|p| {
                        p.spawn((Caption::HistoryButton, label("Журнал", &icons.1, 13.)));
                    });
                    p.spawn(button(Action::Refresh)).with_children(|p| {
                        p.spawn(label("Обновить", &icons.1, 13.));
                    });
                });
                p.spawn((
                    Section::Grids,
                    Node {
                        column_gap: px(32),
                        justify_content: JustifyContent::SpaceBetween,
                        ..default()
                    },
                ))
                .with_children(|row| {
                    for side in [Side::Bag, Side::Storage] {
                        row.spawn(Node {
                            width: px(348),
                            flex_shrink: 0.,
                            flex_direction: FlexDirection::Column,
                            row_gap: px(10),
                            ..default()
                        })
                        .with_children(|col| {
                            col.spawn(label(
                                if side == Side::Bag {
                                    "РЮКЗАК"
                                } else {
                                    "ОБЩИЙ СКЛАД"
                                },
                                &icons.1,
                                13.,
                            ));
                            col.spawn((
                                Grid(side),
                                Node {
                                    width: px(348),
                                    height: px(318),
                                    flex_wrap: FlexWrap::Wrap,
                                    column_gap: px(10),
                                    row_gap: px(7),
                                    align_content: AlignContent::Start,
                                    padding: UiRect::axes(px(9), px(0)),
                                    ..default()
                                },
                            ))
                            .with_children(|grid| {
                                for n in 0..PAGE {
                                    grid.spawn((
                                        Button,
                                        Action::Cell(side, n),
                                        OwnStyle,
                                        Node {
                                            width: px(58),
                                            height: px(58),
                                            flex_shrink: 0.,
                                            border: UiRect::all(px(1)),
                                            border_radius: BorderRadius::all(px(5)),
                                            padding: UiRect::all(px(3)),
                                            ..default()
                                        },
                                        BackgroundColor(Color::srgb(0.035, 0.045, 0.055)),
                                        BorderColor::all(EDGE),
                                    ))
                                    .with_children(|cell| {
                                        cell.spawn((
                                            CellIcon(side, n),
                                            ImageNode::default(),
                                            Node {
                                                width: percent(100),
                                                height: percent(100),
                                                ..default()
                                            },
                                            FocusPolicy::Pass,
                                        ));
                                        cell.spawn((
                                            Caption::Count(side, n),
                                            label("", &icons.1, 12.),
                                            Node {
                                                position_type: PositionType::Absolute,
                                                bottom: px(2),
                                                right: px(4),
                                                ..default()
                                            },
                                            FocusPolicy::Pass,
                                        ));
                                    });
                                }
                            });
                            col.spawn(Node {
                                justify_content: JustifyContent::SpaceBetween,
                                align_items: AlignItems::Center,
                                ..default()
                            })
                            .with_children(|p| {
                                p.spawn(Node {
                                    width: px(44),
                                    height: px(34),
                                    ..default()
                                })
                                .with_children(|p| {
                                    p.spawn(button(Action::Page(side, -1))).with_children(|p| {
                                        p.spawn(label("‹", &icons.1, 15.));
                                    });
                                });
                                p.spawn((Caption::Page(side), label("", &icons.1, 12.)));
                                p.spawn(Node {
                                    width: px(44),
                                    height: px(34),
                                    justify_content: JustifyContent::FlexEnd,
                                    ..default()
                                })
                                .with_children(|p| {
                                    p.spawn(button(Action::Page(side, 1))).with_children(|p| {
                                        p.spawn(label("›", &icons.1, 15.));
                                    });
                                });
                            });
                        });
                    }
                });
                p.spawn((
                    Section::History,
                    Node {
                        display: Display::None,
                        height: px(391),
                        flex_direction: FlexDirection::Column,
                        row_gap: px(12),
                        padding: UiRect::all(px(14)),
                        ..default()
                    },
                    BackgroundColor(BUTTON),
                ))
                .with_children(|p| {
                    p.spawn(label("Последние 8 перемещений · время UTC", &icons.1, 13.));
                    p.spawn((Caption::History, label("", &icons.1, 12.)));
                });
                p.spawn((
                    Section::Controls,
                    Node {
                        flex_direction: FlexDirection::Column,
                        row_gap: px(8),
                        ..default()
                    },
                ))
                .with_children(|p| {
                    p.spawn((Caption::Selection, label("", &icons.1, 13.)));
                    p.spawn(Node {
                        align_items: AlignItems::Center,
                        column_gap: px(8),
                        height: px(36),
                        ..default()
                    })
                    .with_children(|p| {
                        for (a, text) in [
                            (Action::Less, "−"),
                            (Action::More, "+"),
                            (Action::All, "Всё"),
                        ] {
                            p.spawn(button(a)).with_children(|p| {
                                p.spawn(label(text, &icons.1, 14.));
                            });
                        }
                        p.spawn((Caption::Quantity, label("", &icons.1, 14.)));
                        p.spawn(button(Action::Move)).with_children(|p| {
                            p.spawn((Caption::Move, label("", &icons.1, 14.)));
                        });
                    });
                });
                p.spawn((Caption::Status, label("", &icons.1, 12.)));
            });
        });
    commands
        .spawn((
            Tip,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                width: px(348),
                padding: UiRect::all(px(14)),
                border: UiRect::all(px(1)),
                border_radius: BorderRadius::all(px(5)),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(GOLD),
            GlobalZIndex(32),
            FocusPolicy::Pass,
        ))
        .with_children(|p| {
            p.spawn((TipText, label("", &icons.1, 14.), FocusPolicy::Pass));
        });
    commands.spawn((
        Ghost,
        ImageNode::default(),
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            width: px(48),
            height: px(48),
            ..default()
        },
        GlobalZIndex(40),
        FocusPolicy::Pass,
    ));
}
fn visible(network: &Network, panels: &CharacterWindow, alliance: &crate::alliance::Form) -> bool {
    network.mode == Mode::Online
        && panels.open
        && panels.alliance
        && alliance.warehouse
        && network.alliance.alliance.is_some()
}
fn bag(network: &Network) -> Vec<(u16, u64, u32)> {
    let Some(c) = &network.character else {
        return vec![];
    };
    layout::bag(c)
        .into_iter()
        .filter_map(|(slot, t)| {
            let k = layout::kind(t)?;
            items::currency(k).is_none().then(|| {
                (
                    slot,
                    t,
                    if items::equippable(k) {
                        1
                    } else {
                        layout::quantity(c, k)
                    },
                )
            })
        })
        .collect()
}
fn pages(form: &Form, side: Side) -> usize {
    if side == Side::Storage {
        alliance::STORAGE_SLOTS.div_ceil(PAGE)
    } else {
        form.bag
            .last()
            .map_or(1, |(s, _, _)| (*s as usize + 1).div_ceil(PAGE))
    }
}
fn at(network: &Network, form: &Form, side: Side, n: usize) -> Option<Item> {
    let slot = form.pages[side.index()] * PAGE + n;
    match side {
        Side::Bag => form
            .bag
            .iter()
            .find(|(s, _, _)| *s as usize == slot)
            .and_then(|(_, t, q)| {
                Some(Item {
                    source: Source::Bag(*t),
                    kind: layout::kind(*t)?,
                    quantity: *q,
                })
            }),
        Side::Storage => network
            .alliance
            .alliance
            .as_ref()?
            .storage
            .entries
            .iter()
            .find(|e| e.slot == slot)
            .map(|e| Item {
                source: Source::Storage(e.id),
                kind: e.kind,
                quantity: e.quantity,
            }),
    }
}
fn selected(network: &Network, form: &Form) -> Option<Item> {
    match form.selected? {
        Source::Bag(token) => {
            form.bag
                .iter()
                .find(|(_, t, _)| *t == token)
                .and_then(|(_, t, q)| {
                    Some(Item {
                        source: Source::Bag(*t),
                        kind: layout::kind(*t)?,
                        quantity: *q,
                    })
                })
        }
        Source::Storage(id) => network
            .alliance
            .alliance
            .as_ref()?
            .storage
            .entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| Item {
                source: Source::Storage(e.id),
                kind: e.kind,
                quantity: e.quantity,
            }),
    }
}
fn can_take(network: &Network) -> bool {
    network
        .character
        .as_ref()
        .and_then(|c| network.alliance.alliance.as_ref()?.role(c.id))
        .is_some_and(|r| r != Role::Member)
}
fn permitted(network: &Network, item: Item) -> bool {
    item.source.side() == Side::Bag || can_take(network)
}
fn submit(network: &mut Network, form: &mut Form) {
    let Some(item) = selected(network, form) else {
        return;
    };
    if network.alliance_pending || network.pending || !permitted(network, item) {
        return;
    }
    let Some(a) = &network.alliance.alliance else {
        return;
    };
    // Keep the revision captured with the selection; never silently retry a stale transfer.
    let command = match item.source {
        Source::Bag(token) => Command::Deposit {
            alliance: a.id,
            revision: form.revision,
            item: AttachmentRequest {
                token,
                quantity: form.quantity,
                price: 0,
            },
        },
        Source::Storage(entry) => Command::Withdraw {
            alliance: a.id,
            revision: form.revision,
            entry,
            quantity: form.quantity,
        },
    };
    network.alliance_request(command);
    if network.alliance_pending {
        form.selected = None;
        form.held = None;
    }
}
pub fn input(
    mut form: ResMut<Form>,
    mut network: ResMut<Network>,
    panels: Res<CharacterWindow>,
    mut alliance: ResMut<crate::alliance::Form>,
    mouse: Res<ButtonInput<MouseButton>>,
    window: Single<&Window, With<PrimaryWindow>>,
    scale: Res<UiScale>,
    buttons: Query<(&Action, &Interaction), Changed<Interaction>>,
    grids: Query<(&Grid, &ComputedNode, &UiGlobalTransform)>,
) {
    if !visible(&network, &panels, &alliance) {
        *form = default();
        return;
    }
    let a = network.alliance.alliance.as_ref().unwrap();
    let owner = network.character.as_ref().map(|c| (c.id, a.id));
    if form.owner != owner {
        *form = Form { owner, ..default() };
    }
    let current = bag(&network);
    if form.revision != a.storage.revision || form.bag != current {
        form.selected = None;
        form.held = None;
        form.quantity = 1;
        form.revision = a.storage.revision;
        form.bag = current;
    }
    for side in [Side::Bag, Side::Storage] {
        form.pages[side.index()] = form.pages[side.index()].min(pages(&form, side) - 1);
    }
    let cursor = window.physical_cursor_position();
    let action = buttons
        .iter()
        .find(|(_, i)| **i == Interaction::Pressed)
        .map(|(a, _)| *a);
    if let Some(action) = action {
        match action {
            Action::Back => {
                alliance.warehouse = false;
                form.held = None;
                return;
            }
            Action::History => {
                form.history = !form.history;
                form.held = None;
            }
            Action::Refresh => {
                network.alliance_request(Command::Refresh);
                form.held = None;
            }
            Action::Page(side, delta) if !form.history => {
                let page = &mut form.pages[side.index()];
                *page = page.saturating_add_signed(delta as isize);
                form.pages[side.index()] = form.pages[side.index()].min(pages(&form, side) - 1);
                form.held = None;
            }
            Action::Cell(side, n)
                if !form.history && !network.alliance_pending && !network.pending =>
            {
                let item = at(&network, &form, side, n);
                let source = item.map(|i| i.source);
                if form.selected != source {
                    form.quantity = 1;
                }
                form.selected = source;
                form.held = item
                    .filter(|i| permitted(&network, *i))
                    .zip(cursor)
                    .map(|(item, start)| Held { item, start });
            }
            Action::Less => {
                form.quantity = form.quantity.saturating_sub(1).max(1);
            }
            Action::More | Action::All => {
                if let Some(item) = selected(&network, &form) {
                    let max = item.quantity.min(transfer::MAX_AMOUNT);
                    form.quantity = if matches!(action, Action::All) {
                        max
                    } else {
                        (form.quantity + 1).min(max)
                    };
                }
            }
            Action::Move if !form.history => submit(&mut network, &mut form),
            _ => {}
        }
    }
    if mouse.just_released(MouseButton::Left) {
        if let Some(held) = form.held.take()
            && let Some(cursor) = cursor
            && cursor.distance(held.start) > 7. * window.scale_factor() * scale.0
            && grids
                .iter()
                .any(|(g, n, t)| g.0 != held.item.source.side() && contains(n, t, cursor))
        {
            submit(&mut network, &mut form);
        }
    } else if !mouse.pressed(MouseButton::Left) {
        form.held = None;
    }
}
#[allow(clippy::type_complexity)]
pub fn display(
    network: Res<Network>,
    panels: Res<CharacterWindow>,
    alliance: Res<crate::alliance::Form>,
    form: Res<Form>,
    icons: Res<Icons>,
    mut nodes: Query<
        (&mut Node, Option<&Action>, Option<&Section>),
        Or<(With<Action>, With<Section>)>,
    >,
    mut images: Query<(&CellIcon, &mut ImageNode), (Without<Action>, Without<Section>)>,
    mut captions: Query<(&Caption, &mut Text)>,
    mut borders: Query<(&Action, &Interaction, &mut BorderColor)>,
) {
    let shown = visible(&network, &panels, &alliance);
    let selection = selected(&network, &form);
    let pending = network.alliance_pending || network.pending;
    let can_move = selection.is_some_and(|i| permitted(&network, i)) && !pending;
    for (mut node, action, section) in &mut nodes {
        let show = if let Some(section) = section {
            match section {
                Section::Root => shown,
                Section::Grids | Section::Controls => !form.history,
                Section::History => form.history,
            }
        } else if let Some(action) = action {
            match action {
                Action::Move => can_move,
                Action::Less => can_move && form.quantity > 1,
                Action::More => {
                    can_move
                        && selection
                            .is_some_and(|i| form.quantity < i.quantity.min(transfer::MAX_AMOUNT))
                }
                Action::All => {
                    can_move
                        && selection
                            .is_some_and(|i| form.quantity < i.quantity.min(transfer::MAX_AMOUNT))
                }
                Action::Page(side, delta) => {
                    if *delta < 0 {
                        form.pages[side.index()] > 0
                    } else {
                        form.pages[side.index()] + 1 < pages(&form, *side)
                    }
                }
                _ => true,
            }
        } else {
            continue;
        };
        node.display = if show { Display::Flex } else { Display::None };
    }
    for (cell, mut image) in &mut images {
        let item = at(&network, &form, cell.0, cell.1);
        image.image = icons.0[item.map_or(0, |i| item_icon(i.kind))].clone();
        image.color = if item.is_some() {
            Color::WHITE
        } else {
            Color::NONE
        };
    }
    for (action, interaction, mut border) in &mut borders {
        if let Action::Cell(side, n) = action {
            let active =
                at(&network, &form, *side, *n).is_some_and(|i| Some(i.source) == form.selected);
            *border = BorderColor::all(if active || *interaction == Interaction::Hovered {
                GOLD
            } else {
                EDGE
            });
        }
    }
    for (caption, mut text) in &mut captions {
        let value = match caption {
            Caption::Summary => network
                .alliance
                .alliance
                .as_ref()
                .map(|a| {
                    format!(
                        "{} · {}/{} ячеек занято · {}",
                        a.name,
                        a.storage.entries.len(),
                        alliance::STORAGE_SLOTS,
                        if can_take(&network) {
                            "Вклад и получение"
                        } else {
                            "Вклад · получение доступно лидеру и офицерам"
                        }
                    )
                })
                .unwrap_or_default(),
            Caption::Page(side) => {
                format!("{} / {}", form.pages[side.index()] + 1, pages(&form, *side))
            }
            Caption::Count(side, n) => at(&network, &form, *side, *n)
                .filter(|i| !items::equippable(i.kind))
                .map(|i| i.quantity.to_string())
                .unwrap_or_default(),
            Caption::Selection => selection
                .and_then(|i| items::definition(i.kind))
                .map(|d| d.name.to_owned())
                .unwrap_or_else(|| "Выберите предмет или перетащите его в соседнюю сетку".into()),
            Caption::Quantity => selection
                .map(|_| format!("× {}", form.quantity))
                .unwrap_or_default(),
            Caption::Status => {
                if pending {
                    "Сохраняем…".into()
                } else {
                    network.alliance.message.clone()
                }
            }
            Caption::Move => {
                if selection.is_some_and(|i| i.source.side() == Side::Bag) {
                    "На склад →".into()
                } else {
                    "← В рюкзак".into()
                }
            }
            Caption::HistoryButton => {
                if form.history {
                    "Предметы".into()
                } else {
                    "Журнал".into()
                }
            }
            Caption::History => network
                .alliance
                .alliance
                .as_ref()
                .map(|a| {
                    if a.storage.history.is_empty() {
                        "На складе ещё не было перемещений".into()
                    } else {
                        a.storage
                            .history
                            .iter()
                            .map(|e| {
                                format!(
                                    "{} · {} {}\n{} ×{}",
                                    e.time,
                                    e.nickname,
                                    if e.deposit {
                                        "положил(а)"
                                    } else {
                                        "забрал(а)"
                                    },
                                    items::definition(e.kind).map_or("Предмет", |d| d.name),
                                    e.quantity
                                )
                            })
                            .collect::<Vec<_>>()
                            .join("\n\n")
                    }
                })
                .unwrap_or_default(),
        };
        if text.0 != value {
            text.0 = value;
        }
    }
}
fn description(item: Item, can_take: bool) -> String {
    let Some(d) = items::definition(item.kind) else {
        return String::new();
    };
    let mut text = format!(
        "{}\n{} · Обычный\nКласс: {}",
        d.name,
        d.slot,
        match d.class {
            Some(1) => "Воин",
            Some(2) => "Маг",
            _ => "Любой",
        }
    );
    if d.bonus > 0. {
        text.push_str(&format!(
            "\n{} +{:.0}\nОтдельный экземпляр",
            match d.stat {
                "f_attack" => "Физическая атака",
                "m_attack" => "Магическая атака",
                _ => "Физическая защита",
            },
            d.bonus
        ));
    } else {
        text.push_str(&format!("\nКоличество: {}", item.quantity));
    }
    if item.kind == 8 {
        text.push_str("\nВосстанавливает до 50 HP.");
    }
    if let Some(recipe) = mortown_protocol::crafting::definition(item.kind) {
        text.push_str(&format!(
            "\nСоздаёт: {} ×1",
            items::definition(recipe.result).unwrap().name
        ));
        for (kind, count) in recipe.ingredients {
            text.push_str(&format!(
                "\n{} ×{}",
                items::definition(*kind).unwrap().name,
                count
            ));
        }
    }
    text.push_str(match item.source {
        Source::Bag(_) => "\nВыберите количество и перенесите на склад.",
        Source::Storage(_) if can_take => "\nВыберите количество и перенесите в рюкзак.",
        _ => "\nПолучение доступно лидеру и офицерам.",
    });
    text
}
#[allow(clippy::type_complexity)]
pub fn hints(
    network: Res<Network>,
    panels: Res<CharacterWindow>,
    alliance: Res<crate::alliance::Form>,
    form: Res<Form>,
    icons: Res<Icons>,
    window: Single<&Window, With<PrimaryWindow>>,
    scale: Res<UiScale>,
    cells: Query<(&Action, &ComputedNode, &UiGlobalTransform)>,
    mut tip: Single<&mut Node, (With<Tip>, Without<Ghost>)>,
    mut text: Single<&mut Text, With<TipText>>,
    mut ghost: Single<(&mut Node, &mut ImageNode), (With<Ghost>, Without<Tip>)>,
) {
    tip.display = Display::None;
    ghost.0.display = Display::None;
    if !visible(&network, &panels, &alliance) || form.history {
        return;
    }
    let Some(cursor) = window.physical_cursor_position() else {
        return;
    };
    let factor = window.scale_factor() * scale.0;
    let p = cursor / factor;
    if let Some(held) = form.held
        && cursor.distance(held.start) > 7. * factor
    {
        ghost.0.display = Display::Flex;
        ghost.0.left = px(p.x + 12.);
        ghost.0.top = px(p.y + 12.);
        ghost.1.image = icons.0[item_icon(held.item.kind)].clone();
        return;
    }
    if let Some(item) = cells.iter().find_map(|(action, n, t)| {
        if let Action::Cell(side, i) = action
            && contains(n, t, cursor)
        {
            at(&network, &form, *side, *i)
        } else {
            None
        }
    }) {
        let value = description(item, can_take(&network));
        let rows: usize = value
            .lines()
            .map(|l| l.chars().count().div_ceil(36).max(1))
            .sum();
        let height = rows as f32 * 19. + 28.;
        text.0 = value;
        tip.display = Display::Flex;
        tip.left = px((p.x + 16.).min(window.width() / scale.0 - 364.).max(8.));
        tip.top = px(if p.y + height + 24. < window.height() / scale.0 {
            p.y + 20.
        } else {
            (p.y - height - 12.).max(8.)
        });
    }
}
