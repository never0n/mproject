use crate::{
    actionbar::{Icons, label},
    network::{Mode, Network},
    ui::{BUTTON, GOLD, PANEL},
};
use bevy::prelude::*;
use mortown_protocol::{
    exchange::{Command, Offer},
    mail::AttachmentRequest,
};
#[derive(Component)]
pub struct Overlay;
#[derive(Component)]
pub struct Invite;
#[derive(Component, Clone, Copy)]
pub enum Action {
    Accept,
    Cancel,
    Previous,
    Next,
    Remove,
    Less,
    More,
    All,
    Coins(i64),
    Clear,
    Offer,
    Confirm,
    Unlock,
}
#[derive(Component)]
pub struct Content;
#[derive(Component)]
pub struct Picture(bool);
#[derive(Resource, Default)]
pub struct Form {
    session: Option<u64>,
    item: Option<AttachmentRequest>,
    coins: u32,
    published: Option<(Option<AttachmentRequest>, u32)>,
}
fn button(action: Action) -> impl Bundle {
    (
        Button,
        action,
        Node {
            padding: UiRect::all(px(8)),
            ..default()
        },
        BackgroundColor(BUTTON),
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
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            GlobalZIndex(18),
        ))
        .with_children(|root| {
            root.spawn((
                crate::ui::WorldInputBlocker,
                Node {
                    width: px(680),
                    max_width: percent(96),
                    padding: UiRect::all(px(22)),
                    border: UiRect::all(px(1)),
                    flex_direction: FlexDirection::Column,
                    row_gap: px(10),
                    ..default()
                },
                BackgroundColor(PANEL),
                BorderColor::all(GOLD),
            ))
            .with_children(|p| {
                p.spawn(label("Обмен между игроками", &icons.1, 22.));
                p.spawn((Content, label("", &icons.1, 15.)));
                p.spawn(Node {
                    column_gap: px(230),
                    ..default()
                })
                .with_children(|row| {
                    for own in [true, false] {
                        row.spawn((
                            Picture(own),
                            crate::actionbar::ItemHint(0),
                            ImageNode::default(),
                            Node {
                                width: px(48),
                                height: px(48),
                                ..default()
                            },
                        ));
                    }
                });
                for entries in [
                    vec![
                        (Action::Previous, "Предыдущая вещь"),
                        (Action::Next, "Следующая вещь"),
                        (Action::Remove, "Убрать вещь"),
                    ],
                    vec![
                        (Action::Less, "Количество −"),
                        (Action::More, "Количество +"),
                        (Action::All, "Вся стопка"),
                    ],
                    vec![
                        (Action::Coins(-1000), "−1000"),
                        (Action::Coins(-100), "−100"),
                        (Action::Coins(-10), "−10"),
                        (Action::Coins(-1), "−1"),
                        (Action::Coins(1), "+1"),
                        (Action::Coins(10), "+10"),
                        (Action::Coins(100), "+100"),
                        (Action::Coins(1000), "+1000"),
                        (Action::Clear, "0 монет"),
                    ],
                    vec![
                        (Action::Accept, "Принять приглашение"),
                        (Action::Offer, "Предложить"),
                        (Action::Confirm, "Подтвердить обмен"),
                        (Action::Unlock, "Изменить предложение"),
                        (Action::Cancel, "Отменить обмен"),
                    ],
                ] {
                    p.spawn(Node {
                        flex_wrap: FlexWrap::Wrap,
                        column_gap: px(5),
                        row_gap: px(5),
                        ..default()
                    })
                    .with_children(|row| {
                        for (action, title) in entries {
                            row.spawn(button(action)).with_children(|b| {
                                b.spawn(label(title, &icons.1, 13.));
                            });
                        }
                    });
                }
                p.spawn(label(
                    "Изменение предложения сбрасывает оба подтверждения. Время обмена — 2 минуты.",
                    &icons.1,
                    12.,
                ));
            });
        });
}
pub fn input(
    mut form: ResMut<Form>,
    mut network: ResMut<Network>,
    mut panels: ResMut<crate::ui::CharacterWindow>,
    mut crafting: ResMut<crate::crafting::Open>,
    target: Res<crate::combat::CombatUi>,
    buttons: Query<(&Interaction, &Action), Changed<Interaction>>,
    invites: Query<&Interaction, (With<Invite>, Changed<Interaction>)>,
) {
    if network.mode != Mode::Online {
        *form = default();
        network.exchange = None;
        network.exchange_pending = false;
        return;
    }
    if invites.iter().any(|i| *i == Interaction::Pressed)
        && let Some(player) = target.player
    {
        network.exchange_request(Command::Invite { player });
    }
    let Some(view) = network.exchange.clone() else {
        *form = default();
        return;
    };
    if form.session != Some(view.session) {
        *panels = default();
        crafting.0 = false;
    }
    let published = (view.own.item, view.own.coins);
    if form.session != Some(view.session) || form.published != Some(published) {
        form.session = Some(view.session);
        form.item = published.0;
        form.coins = published.1;
        form.published = Some(published);
    }
    if network.exchange_pending {
        return;
    }
    let Some(action) = buttons
        .iter()
        .find(|(i, _)| **i == Interaction::Pressed)
        .map(|(_, a)| *a)
    else {
        return;
    };
    let session = view.session;
    let revision = view.revision;
    match action {
        Action::Cancel => network.exchange_request(Command::Cancel { session }),
        Action::Accept => network.exchange_request(Command::Accept { session, revision }),
        Action::Offer if view.accepted => network.exchange_request(Command::Offer {
            session,
            revision,
            item: form.item,
            coins: form.coins,
        }),
        Action::Unlock if view.accepted && view.own.confirmed => {
            network.exchange_request(Command::Offer {
                session,
                revision,
                item: view.own.item,
                coins: view.own.coins,
            })
        }
        Action::Confirm if view.accepted && (form.item, form.coins) == published => {
            network.exchange_request(Command::Confirm { session, revision })
        }
        _ if !view.accepted || view.own.confirmed => {}
        Action::Remove => form.item = None,
        Action::Clear => form.coins = 0,
        Action::Coins(delta) => {
            let balance = network
                .authoritative
                .as_ref()
                .map_or(0, |a| a.progress.coins as u32);
            form.coins = (i64::from(form.coins) + delta).clamp(0, i64::from(balance)) as u32;
        }
        Action::Previous | Action::Next => {
            let items = network
                .character
                .as_ref()
                .map_or(Vec::new(), mortown_protocol::transfer::eligible);
            if !items.is_empty() {
                let index = form
                    .item
                    .and_then(|a| items.iter().position(|t| *t == a.token));
                let index = match (action, index) {
                    (Action::Previous, Some(i)) => (i + items.len() - 1) % items.len(),
                    (_, Some(i)) => (i + 1) % items.len(),
                    _ => 0,
                };
                form.item = Some(AttachmentRequest {
                    token: items[index],
                    quantity: 1,
                    price: 0,
                });
            }
        }
        Action::Less | Action::More | Action::All => {
            if let Some(mut a) = form.item
                && let Some(c) = &network.character
                && let Some(k) = mortown_protocol::layout::kind(a.token)
            {
                let max = if mortown_protocol::items::equippable(k) {
                    1
                } else {
                    mortown_protocol::layout::quantity(c, k)
                        .min(mortown_protocol::transfer::MAX_AMOUNT)
                };
                a.quantity = match action {
                    Action::Less => a.quantity.saturating_sub(1).max(1),
                    Action::More => a.quantity.saturating_add(1).min(max),
                    _ => max,
                };
                form.item = Some(a);
            }
        }
        _ => {}
    }
}
fn describe(offer: &Offer) -> String {
    let item = offer
        .item
        .and_then(|a| {
            mortown_protocol::layout::kind(a.token).and_then(|k| {
                mortown_protocol::items::definition(k)
                    .map(|d| format!("{} ×{}", d.name, a.quantity))
            })
        })
        .unwrap_or("Без предмета".into());
    format!(
        "{item} · {} монет · {}",
        offer.coins,
        if offer.confirmed {
            "ПОДТВЕРЖДЕНО"
        } else {
            "не подтверждено"
        }
    )
}
pub fn display(
    network: Res<Network>,
    form: Res<Form>,
    target: Res<crate::combat::CombatUi>,
    icons: Res<Icons>,
    mut root: Single<
        &mut Node,
        (
            With<Overlay>,
            Without<Action>,
            Without<Invite>,
            Without<Picture>,
        ),
    >,
    mut invite: Single<
        &mut Node,
        (
            With<Invite>,
            Without<Overlay>,
            Without<Action>,
            Without<Picture>,
        ),
    >,
    mut text: Single<&mut Text, With<Content>>,
    mut buttons: Query<(&Action, &mut Node), (Without<Overlay>, Without<Invite>, Without<Picture>)>,
    mut pictures: Query<
        (
            &Picture,
            &mut ImageNode,
            &mut Node,
            &mut crate::actionbar::ItemHint,
        ),
        (Without<Overlay>, Without<Invite>, Without<Action>),
    >,
) {
    let online = network.mode == Mode::Online;
    let view = network.exchange.as_ref().filter(|_| online);
    root.display = if view.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    invite.display = if online
        && view.is_none()
        && target
            .player
            .is_some_and(|id| network.character.as_ref().is_some_and(|c| c.id != id))
    {
        Display::Flex
    } else {
        Display::None
    };
    let Some(v) = view else { return };
    let dirty = (form.item, form.coins) != (v.own.item, v.own.coins);
    let own = Offer {
        item: form.item,
        coins: form.coins,
        confirmed: v.own.confirmed && !dirty,
    };
    text.0 = format!(
        "{}\nВы: {}\n{}: {}\n{}\n{}",
        if v.accepted {
            "Предложение обмена"
        } else if v.invited {
            "Вас приглашают обменяться"
        } else {
            "Ожидаем принятия приглашения"
        },
        describe(&own),
        v.peer,
        describe(&v.other),
        if dirty {
            "Черновик изменён — нажмите «Предложить»"
        } else {
            "Предложение отправлено"
        },
        network.exchange_status
    );
    for (action, mut node) in &mut buttons {
        let show = match action {
            Action::Accept => !v.accepted && v.invited,
            Action::Cancel => true,
            Action::Confirm => {
                v.accepted && !dirty && !v.own.confirmed && !network.exchange_pending
            }
            Action::Unlock => v.accepted && v.own.confirmed && !network.exchange_pending,
            Action::Offer => v.accepted && dirty && !network.exchange_pending,
            _ => v.accepted && !v.own.confirmed && !network.exchange_pending,
        };
        node.display = if show { Display::Flex } else { Display::None };
    }
    for (picture, mut image, mut node, mut hint) in &mut pictures {
        let item = if picture.0 { form.item } else { v.other.item };
        let kind = item.and_then(|a| mortown_protocol::layout::kind(a.token));
        node.display = if kind.is_some() {
            Display::Flex
        } else {
            Display::None
        };
        hint.0 = kind.unwrap_or(0);
        image.image = icons.0[crate::actionbar::item_icon(kind.unwrap_or(0))].clone();
    }
}
