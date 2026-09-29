use crate::{
    Movement, Terrain,
    actionbar::{self, Icons, ItemHint},
    network::{Mode, Network},
    ui::{BUTTON, CharacterWindow, EDGE, GOLD, PANEL},
};
use bevy::{prelude::*, tasks::AsyncComputeTaskPool, window::PrimaryWindow};
use mortown_protocol::{Character, ClientMessage, items, layout, trade::Sale};

const PAGE_SIZE: usize = 20;
#[derive(Component)]
pub struct TradeOverlay;
#[derive(Component, Clone, Copy)]
pub enum Action {
    Open,
    Select,
    Tab(bool),
    Page(i32),
    Commit,
    Close,
    Talk,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct Entry {
    kind: u8,
    sale: Option<Sale>,
}
impl Entry {
    fn valid(self, c: &Character) -> bool {
        match self.sale {
            Some(sale) => {
                items::sell_price(self.kind).is_some() && sale.validate(c, self.kind).is_ok()
            }
            None => {
                items::buy_price(self.kind).is_some()
                    && (mortown_protocol::crafting::definition(self.kind).is_none()
                        || layout::quantity(c, self.kind) == 0)
            }
        }
    }
    fn price(self, network: &Network) -> Option<u32> {
        if self.sale.is_some() {
            items::sell_price(self.kind)
        } else {
            network
                .authoritative
                .as_ref()
                .and_then(|a| items::purchase_price(self.kind, &a.progress))
        }
    }
}
#[derive(Resource, Default)]
pub struct Form {
    context: Option<(i64, u8)>,
    selling: bool,
    page: usize,
    selected: Option<Entry>,
    reset_scroll: bool,
}
#[derive(Component)]
pub struct Cell {
    index: usize,
    entry: Option<Entry>,
}
#[derive(Component)]
pub struct DescriptionScroll;
#[derive(Component)]
pub enum Caption {
    Heading,
    Wallet,
    Guide,
    Page,
    Empty,
    Cell(usize),
    Count(usize),
    Name,
    Details,
    Price,
    Reason,
    Commit,
    Receipt,
}
#[derive(Component)]
pub enum Picture {
    Cell(usize),
    Selected,
}

fn entries(c: &Character, selling: bool) -> Vec<Entry> {
    if selling {
        layout::bag(c)
            .into_iter()
            .filter_map(|(slot, token)| {
                let kind = layout::kind(token)?;
                items::sell_price(kind)?;
                Some(Entry {
                    kind,
                    sale: Some(Sale {
                        slot,
                        token,
                        owned: layout::quantity(c, kind),
                    }),
                })
            })
            .collect()
    } else {
        mortown_protocol::weapons::BASIC
            .iter()
            .chain(items::MERCHANT_STOCK)
            .map(|&kind| Entry { kind, sale: None })
            .filter(|e| e.valid(c))
            .collect()
    }
}
fn reason(entry: Option<Entry>, network: &Network) -> Option<String> {
    if network.pending {
        return Some("Сделка обрабатывается…".into());
    }
    let entry = entry?;
    let Some(c) = network.character.as_ref() else {
        return Some("Персонаж недоступен".into());
    };
    if !entry.valid(c) {
        return Some("Вещь изменилась. Выберите её заново".into());
    }
    let Some(actor) = network.authoritative.as_ref().filter(|a| a.hp > 0.) else {
        return Some("Сначала возродитесь".into());
    };
    let Some(price) = entry.price(network) else {
        return Some("Сделка недоступна".into());
    };
    if entry.sale.is_none() && actor.progress.coins < f64::from(price) {
        return Some(format!(
            "Не хватает {:.0} монет",
            (f64::from(price) - actor.progress.coins).ceil()
        ));
    }
    None
}
fn button(p: &mut ChildSpawnerCommands, icons: &Icons, action: Action, title: &str) {
    p.spawn((
        Button,
        action,
        crate::skin::OwnStyle,
        Node {
            padding: UiRect::axes(px(14), px(9)),
            border: UiRect::all(px(1)),
            align_items: AlignItems::Center,
            justify_content: JustifyContent::Center,
            ..default()
        },
        BackgroundColor(BUTTON),
        BorderColor::all(EDGE),
    ))
    .with_children(|b| {
        b.spawn(actionbar::label(title, &icons.1, 14.));
    });
}
pub fn setup(mut commands: Commands, icons: Res<Icons>) {
    commands.init_resource::<Form>();
    commands
        .spawn((
            TradeOverlay,
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
                    width: px(860),
                    max_width: percent(96),
                    padding: UiRect::all(px(22)),
                    row_gap: px(12),
                    flex_direction: FlexDirection::Column,
                    ..default()
                },
                BackgroundColor(PANEL),
            ))
            .with_children(|p| {
                p.spawn((Caption::Heading, actionbar::label("Лавка", &icons.1, 22.)));
                p.spawn((Caption::Wallet, actionbar::label("", &icons.1, 14.)));
                p.spawn(Node {
                    column_gap: px(24),
                    ..default()
                })
                .with_children(|body| {
                    body.spawn(Node {
                        width: px(352),
                        flex_shrink: 0.,
                        flex_direction: FlexDirection::Column,
                        row_gap: px(12),
                        ..default()
                    })
                    .with_children(|left| {
                        left.spawn(Node {
                            column_gap: px(8),
                            ..default()
                        })
                        .with_children(|tabs| {
                            button(tabs, &icons, Action::Tab(false), "Купить");
                            button(tabs, &icons, Action::Tab(true), "Продать");
                        });
                        left.spawn((Caption::Guide, actionbar::label("", &icons.1, 12.)));
                        left.spawn(Node {
                            height: px(320),
                            flex_shrink: 0.,
                            flex_wrap: FlexWrap::Wrap,
                            align_content: AlignContent::Start,
                            column_gap: px(8),
                            row_gap: px(8),
                            ..default()
                        })
                        .with_children(|grid| {
                            for index in 0..PAGE_SIZE {
                                grid.spawn((
                                    Button,
                                    Action::Select,
                                    Cell { index, entry: None },
                                    ItemHint(0),
                                    crate::skin::OwnStyle,
                                    Node {
                                        display: Display::None,
                                        width: px(64),
                                        height: px(74),
                                        padding: UiRect::all(px(5)),
                                        border: UiRect::all(px(1)),
                                        flex_direction: FlexDirection::Column,
                                        align_items: AlignItems::Center,
                                        justify_content: JustifyContent::Center,
                                        row_gap: px(3),
                                        ..default()
                                    },
                                    BackgroundColor(BUTTON),
                                    BorderColor::all(EDGE),
                                ))
                                .with_children(|b| {
                                    b.spawn((
                                        Picture::Cell(index),
                                        ImageNode::new(icons.0[0].clone()),
                                        Node {
                                            width: px(44),
                                            height: px(44),
                                            ..default()
                                        },
                                    ));
                                    b.spawn((
                                        Caption::Cell(index),
                                        actionbar::label("", &icons.1, 11.),
                                    ));
                                    b.spawn((
                                        Caption::Count(index),
                                        actionbar::label("", &icons.1, 11.),
                                        Node {
                                            position_type: PositionType::Absolute,
                                            top: px(2),
                                            right: px(3),
                                            ..default()
                                        },
                                        BackgroundColor(PANEL),
                                    ));
                                });
                            }
                        });
                        left.spawn((Caption::Empty, actionbar::label("", &icons.1, 13.)));
                        left.spawn(Node {
                            align_items: AlignItems::Center,
                            column_gap: px(12),
                            ..default()
                        })
                        .with_children(|pager| {
                            button(pager, &icons, Action::Page(-1), "◀");
                            pager.spawn((Caption::Page, actionbar::label("", &icons.1, 13.)));
                            button(pager, &icons, Action::Page(1), "▶");
                        });
                    });
                    body.spawn((
                        Node {
                            flex_grow: 1.,
                            flex_basis: px(0),
                            min_width: px(240),
                            padding: UiRect::all(px(16)),
                            border: UiRect::all(px(1)),
                            flex_direction: FlexDirection::Column,
                            row_gap: px(10),
                            ..default()
                        },
                        BackgroundColor(Color::srgb(0.048, 0.063, 0.077)),
                        BorderColor::all(EDGE),
                    ))
                    .with_children(|detail| {
                        detail
                            .spawn(Node {
                                min_height: px(64),
                                align_items: AlignItems::Center,
                                column_gap: px(14),
                                ..default()
                            })
                            .with_children(|header| {
                                header.spawn((
                                    Picture::Selected,
                                    ImageNode::new(icons.0[0].clone()),
                                    Node {
                                        width: px(64),
                                        height: px(64),
                                        flex_shrink: 0.,
                                        ..default()
                                    },
                                ));
                                header.spawn((
                                    Caption::Name,
                                    actionbar::label("Выберите предмет", &icons.1, 18.),
                                    Node {
                                        flex_grow: 1.,
                                        flex_basis: px(0),
                                        ..default()
                                    },
                                ));
                            });
                        detail
                            .spawn((
                                DescriptionScroll,
                                ScrollPosition::default(),
                                Node {
                                    height: px(248),
                                    flex_shrink: 0.,
                                    overflow: Overflow::scroll_y(),
                                    ..default()
                                },
                            ))
                            .with_children(|content| {
                                content.spawn((
                                    Caption::Details,
                                    actionbar::label("", &icons.1, 13.),
                                    Node {
                                        flex_shrink: 0.,
                                        width: percent(100),
                                        ..default()
                                    },
                                ));
                            });
                        detail.spawn((
                            Caption::Price,
                            actionbar::label("", &icons.1, 16.),
                            Node {
                                min_height: px(40),
                                ..default()
                            },
                        ));
                        detail.spawn((
                            Caption::Reason,
                            actionbar::label("", &icons.1, 12.),
                            Node {
                                min_height: px(30),
                                ..default()
                            },
                        ));
                        detail
                            .spawn((
                                Button,
                                Action::Commit,
                                crate::skin::OwnStyle,
                                Node {
                                    min_height: px(44),
                                    padding: UiRect::all(px(10)),
                                    border: UiRect::all(px(1)),
                                    align_items: AlignItems::Center,
                                    justify_content: JustifyContent::Center,
                                    ..default()
                                },
                                BackgroundColor(BUTTON),
                                BorderColor::all(EDGE),
                            ))
                            .with_children(|b| {
                                b.spawn((
                                    Caption::Commit,
                                    actionbar::label("Купить", &icons.1, 15.),
                                ));
                            });
                    });
                });
                p.spawn((
                    Caption::Receipt,
                    actionbar::label("", &icons.1, 13.),
                    Node {
                        min_height: px(18),
                        ..default()
                    },
                ));
                p.spawn(Node {
                    column_gap: px(10),
                    ..default()
                })
                .with_children(|footer| {
                    button(footer, &icons, Action::Talk, "Вернуться к разговору");
                    button(footer, &icons, Action::Close, "Закрыть");
                });
            });
        });
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn input(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<CharacterWindow>,
    mut network: ResMut<Network>,
    mut movement: ResMut<Movement>,
    world: Res<Terrain>,
    buttons: Query<(&Action, &Interaction, Option<&Cell>), Changed<Interaction>>,
    mut services: MessageReader<crate::npcs::ServiceRequest>,
    mut form: ResMut<Form>,
) {
    let shop = services
        .read()
        .copied()
        .find(|r| r.service == crate::npcs::Service::Shop && r.valid(&network, &world.0));
    let context = if state.open && state.trading {
        network
            .character
            .as_ref()
            .and_then(|c| state.npc.map(|n| (c.id, n)))
    } else {
        None
    };
    if network.mode != Mode::Online || context != form.context {
        *form = Form {
            context,
            ..default()
        };
    }
    if network.mode != Mode::Online || state.mail {
        return;
    }
    if keys.just_pressed(KeyCode::KeyH) && !network.pending {
        network.request(ClientMessage::UsePotion);
    }
    if state.open
        && state.trading
        && !state
            .npc
            .is_some_and(|id| crate::npcs::available(&network, &world.0, id))
    {
        *state = default();
        *form = default();
        network.status = "Лавка закрыта: вы отошли от торговца".into();
        return;
    }
    let pressed = buttons.iter().find(|(_, i, _)| **i == Interaction::Pressed);
    let selected_cell = pressed.and_then(|(_, _, c)| c.and_then(|c| c.entry));
    let action = if shop.is_some() {
        Some(Action::Open)
    } else if keys.just_pressed(KeyCode::KeyT) {
        Some(if state.open && state.trading {
            Action::Close
        } else {
            Action::Open
        })
    } else {
        pressed.map(|(a, _, _)| *a)
    };
    match action {
        Some(Action::Open) => {
            let Some(actor) = network.authoritative.as_ref() else {
                return;
            };
            let from = [actor.position[0], actor.position[2]];
            if actor.hp <= 0. {
                network.status = "Сначала возродитесь".into();
                return;
            }
            let trader = shop.map(|r| r.npc).or_else(|| {
                mortown_world::npcs::RESIDENTS
                    .iter()
                    .filter(|n| {
                        n.role == mortown_world::npcs::Role::Merchant
                            && n.available(&world.0, actor.position)
                    })
                    .min_by(|a, b| {
                        mortown_world::distance(from, a.position())
                            .total_cmp(&mortown_world::distance(from, b.position()))
                    })
                    .map(|n| n.id)
            });
            if let Some(npc) = trader {
                *state = CharacterWindow {
                    open: true,
                    trading: true,
                    npc: Some(npc),
                    ..default()
                };
                *form = Form {
                    context: Some((actor.id, npc)),
                    ..default()
                };
                network.status.clear();
                *movement = Movement::default();
                network.action(ClientMessage::Attack { target: None });
            } else {
                state.open = false;
                let terrain = world.0.clone();
                *movement = Movement::default();
                movement.destination = Some(mortown_world::SPAWN);
                movement.status = "Идём к торговцу. Рядом нажмите T, чтобы открыть лавку".into();
                movement.pending = Some(AsyncComputeTaskPool::get().spawn(async move {
                    mortown_world::navigation::find_path(&terrain, from, mortown_world::SPAWN)
                }));
            }
        }
        Some(Action::Close) => {
            state.open = false;
            *form = default();
        }
        Some(Action::Talk) if state.open && state.trading => {
            if let Some(npc) = state.npc {
                crate::dialogue::open(npc, &mut state, &mut network, &mut movement, &world.0);
            }
            *form = default();
        }
        Some(Action::Tab(selling)) if state.open && state.trading => {
            form.selling = selling;
            form.page = 0;
            form.selected = None;
            form.reset_scroll = true;
        }
        Some(Action::Page(delta)) if state.open && state.trading => {
            if let Some(c) = &network.character {
                let max = entries(c, form.selling).len().saturating_sub(1) / PAGE_SIZE;
                let page = form.page.saturating_add_signed(delta as isize).min(max);
                if page != form.page {
                    form.page = page;
                    form.selected = None;
                    form.reset_scroll = true;
                }
            }
        }
        Some(Action::Select) if state.open && state.trading => {
            if let Some(entry) = selected_cell.filter(|e| e.sale.is_some() == form.selling)
                && network.character.as_ref().is_some_and(|c| entry.valid(c))
            {
                form.selected = Some(entry);
                form.reset_scroll = true;
            }
        }
        Some(Action::Commit) if state.open && state.trading && !network.pending => {
            if let (Some(entry), Some(npc)) = (form.selected, state.npc) {
                if let Some(why) = reason(Some(entry), &network) {
                    network.status = why;
                    return;
                }
                network.request(ClientMessage::Trade {
                    npc,
                    item: entry.kind,
                    buy: entry.sale.is_none(),
                    sale: entry.sale,
                });
                // Each submitted transaction consumes the selection, so a second click cannot trade again.
                if network.pending {
                    form.selected = None;
                    form.reset_scroll = true;
                }
            }
        }
        _ => {}
    }
}
#[allow(clippy::type_complexity)]
pub fn scroll(
    state: Res<CharacterWindow>,
    mut form: ResMut<Form>,
    mut wheel: ResMut<bevy::input::mouse::AccumulatedMouseScroll>,
    window: Single<&Window, With<PrimaryWindow>>,
    mut area: Single<
        (&ComputedNode, &UiGlobalTransform, &mut ScrollPosition),
        With<DescriptionScroll>,
    >,
) {
    if form.reset_scroll {
        area.2.y = 0.;
        form.reset_scroll = false;
    }
    if !state.open || !state.trading {
        return;
    }
    if wheel.delta.y != 0.
        && window
            .physical_cursor_position()
            .is_some_and(|p| actionbar::contains(area.0, area.1, p))
    {
        let max =
            ((area.0.content_size().y - area.0.size().y) * area.0.inverse_scale_factor()).max(0.);
        let step = if wheel.unit == bevy::input::mouse::MouseScrollUnit::Line {
            28.
        } else {
            1.
        };
        area.2.y = (area.2.y - wheel.delta.y * step).clamp(0., max);
        wheel.delta = Vec2::ZERO;
    }
}
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
pub fn display(
    state: Res<CharacterWindow>,
    network: Res<Network>,
    icons: Res<Icons>,
    mut form: ResMut<Form>,
    mut root: Single<&mut Node, (With<TradeOverlay>, Without<Action>)>,
    mut buttons: Query<
        (
            &Action,
            &Interaction,
            &mut Node,
            &mut BackgroundColor,
            &mut BorderColor,
            Option<&mut Cell>,
            Option<&mut ItemHint>,
        ),
        Without<TradeOverlay>,
    >,
    mut texts: Query<(&Caption, &mut Text)>,
    mut pictures: Query<(&Picture, &mut ImageNode)>,
) {
    root.display = if state.open && state.trading && network.mode == Mode::Online {
        Display::Flex
    } else {
        Display::None
    };
    let Some(c) = network.character.as_ref() else {
        return;
    };
    let all = entries(c, form.selling);
    let pages = all.len().div_ceil(PAGE_SIZE).max(1);
    form.page = form.page.min(pages - 1);
    if form.selected.is_some_and(|e| !e.valid(c)) {
        form.selected = None;
        form.reset_scroll = true;
    }
    let visible = |index| all.get(form.page * PAGE_SIZE + index).copied();
    let why = reason(form.selected, &network);
    let ready = form.selected.is_some() && why.is_none();
    for (action, interaction, mut node, mut bg, mut edge, cell, hint) in &mut buttons {
        let active = match action {
            Action::Tab(sell) => *sell == form.selling,
            Action::Select => cell
                .as_ref()
                .and_then(|c| visible(c.index))
                .is_some_and(|e| Some(e) == form.selected),
            _ => false,
        };
        let enabled = match action {
            Action::Commit => ready,
            Action::Page(d) => {
                if *d < 0 {
                    form.page > 0
                } else {
                    form.page + 1 < pages
                }
            }
            _ => true,
        };
        if let Some(mut cell) = cell {
            cell.entry = visible(cell.index);
            node.display = if cell.entry.is_some() {
                Display::Flex
            } else {
                Display::None
            };
            if let Some(mut hint) = hint {
                hint.0 = cell.entry.map_or(0, |e| e.kind);
            }
        }
        let hover = enabled && *interaction != Interaction::None;
        bg.0 = if !enabled {
            Color::srgb(0.075, 0.085, 0.095)
        } else if active {
            Color::srgb(0.21, 0.20, 0.15)
        } else if hover {
            Color::srgb(0.16, 0.21, 0.24)
        } else {
            BUTTON
        };
        *edge = BorderColor::all(if active || hover { GOLD } else { EDGE });
    }
    let selected = form.selected;
    for (caption, mut text) in &mut texts {
        text.0 = match caption {
            Caption::Heading => state
                .npc
                .and_then(|id| crate::npcs::resolve(&network, id))
                .map_or_else(
                    || "Лавка".into(),
                    |n| {
                        format!(
                            "{} · {}",
                            n.name,
                            if n.hall().is_some() {
                                "Торговые ряды"
                            } else {
                                "Лавка у площади"
                            }
                        )
                    },
                ),
            Caption::Wallet => format!(
                "Ваши монеты: {:.0}",
                network
                    .authoritative
                    .as_ref()
                    .map_or(0., |a| a.progress.coins)
            ),
            Caption::Guide => if form.selling {
                "Вещи из рюкзака · цена за одну вещь"
            } else {
                "Товары торговца · цена за одну вещь"
            }
            .into(),
            Caption::Page => format!("{} / {}", form.page + 1, pages),
            Caption::Empty => if all.is_empty() {
                "В рюкзаке нет вещей, которые принимает торговец."
            } else {
                ""
            }
            .into(),
            Caption::Cell(index) => visible(*index).map_or_else(String::new, |e| {
                format!("{} м.", e.price(&network).unwrap_or(0))
            }),
            Caption::Count(index) => visible(*index)
                .filter(|e| e.sale.is_some() && !items::equippable(e.kind))
                .map_or_else(String::new, |e| {
                    let amount = layout::quantity(c, e.kind);
                    if amount > 999 {
                        "999+".into()
                    } else {
                        format!("×{amount}")
                    }
                }),
            Caption::Name => selected
                .and_then(|e| items::definition(e.kind))
                .map_or_else(|| "Выберите предмет".into(), |d| d.name.into()),
            Caption::Details => selected.map_or_else(
                || {
                    "Наведите курсор на иконку или выберите вещь, чтобы посмотреть её свойства."
                        .into()
                },
                |e| {
                    let description = actionbar::item_description(c, e.kind, false);
                    description
                        .split_once('\n')
                        .map_or(description.clone(), |(_, rest)| rest.to_owned())
                },
            ),
            Caption::Price => selected.map_or_else(String::new, |e| {
                format!(
                    "{}: {} монет · 1 шт.\nУ вас: {}",
                    if e.sale.is_some() {
                        "Вы получите"
                    } else {
                        "Стоимость"
                    },
                    e.price(&network).unwrap_or(0),
                    layout::quantity(c, e.kind)
                )
            }),
            Caption::Reason => why.clone().unwrap_or_default(),
            Caption::Commit => selected.map_or_else(
                || {
                    if form.selling {
                        "Продать выбранное"
                    } else {
                        "Купить выбранное"
                    }
                    .into()
                },
                |e| {
                    format!(
                        "{} за {} монет",
                        if e.sale.is_some() {
                            "Продать 1 шт."
                        } else {
                            "Купить 1 шт."
                        },
                        e.price(&network).unwrap_or(0)
                    )
                },
            ),
            Caption::Receipt => network.status.clone(),
        };
    }
    for (picture, mut image) in &mut pictures {
        let entry = match picture {
            Picture::Cell(index) => visible(*index),
            Picture::Selected => selected,
        };
        image.image = icons.0[entry.map_or(0, |e| actionbar::item_icon(e.kind))].clone();
        image.color = if entry.is_some() {
            Color::WHITE
        } else {
            Color::NONE
        };
    }
}
