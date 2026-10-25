//! Physical things in the world: placed chests and stacks lying on the ground. Shows them in 3D,
//! offers the nearest one within reach on [E] (pick up / open) and draws the open-chest window.
//! Every move is a server request; the window only mirrors the last `View` the server sent.
use crate::{
    Player, Terrain,
    actionbar::{Icons, ItemHint, item_icon, label},
    network::{Mode, Network},
    ui::{BUTTON, GOLD, PANEL},
};
use bevy::{asset::RenderAssetUsages, mesh::PrimitiveTopology, prelude::*};
use mortown_protocol::{
    items,
    objects::{self, Command},
    transfer::{self, Holder, ItemRequest},
};
use std::collections::HashMap;

#[derive(Component, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Shown {
    Container(i64),
    Loose(i64),
    Block(i64),
}
#[derive(Resource)]
pub struct Looks {
    wood: Handle<StandardMaterial>,
    iron: Handle<StandardMaterial>,
    cloth: Handle<StandardMaterial>,
    coin: Handle<StandardMaterial>,
    body: Handle<Mesh>,
    lid: Handle<Mesh>,
    band: Handle<Mesh>,
    clasp: Handle<Mesh>,
    sack: Handle<Mesh>,
    pile: Handle<Mesh>,
    post: Handle<Mesh>,
    bronze: Handle<StandardMaterial>,
}
#[derive(Component)]
pub struct ChestWindow;
#[derive(Component)]
pub struct Content;
#[derive(Component)]
pub struct Prompt;
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Cell(u16),
    TakeCoins,
    PutCoins,
    Lift,
    Close,
    Price(u64),
}
#[derive(Component)]
pub struct CellIcon(u16);
#[derive(Component)]
pub struct CellText(u16);

pub fn setup(
    mut commands: Commands,
    icons: Res<Icons>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let mut material = |rgb: [f32; 3], metallic: f32| {
        materials.add(StandardMaterial {
            base_color: Color::srgb(rgb[0], rgb[1], rgb[2]),
            metallic,
            perceptual_roughness: if metallic > 0. { 0.45 } else { 0.85 },
            ..default()
        })
    };
    commands.insert_resource(Looks {
        wood: material([0.36, 0.22, 0.12], 0.),
        iron: material([0.30, 0.32, 0.34], 0.7),
        cloth: material([0.55, 0.45, 0.30], 0.),
        coin: material([0.78, 0.58, 0.22], 0.8),
        body: meshes.add(Cuboid::new(0.9, 0.5, 0.55)),
        lid: meshes.add(Cuboid::new(0.94, 0.14, 0.59)),
        band: meshes.add(Cuboid::new(0.05, 0.66, 0.6)),
        clasp: meshes.add(Cuboid::new(0.1, 0.12, 0.04)),
        sack: meshes.add(Sphere::new(0.18).mesh().uv(12, 8)),
        pile: meshes.add(Cylinder::new(0.09, 0.04).mesh().resolution(12).build()),
        post: meshes.add(Cuboid::new(1.2, 1.6, 0.35)),
        bronze: materials.add(StandardMaterial {
            base_color: Color::srgb(0.72, 0.45, 0.18),
            metallic: 0.85,
            perceptual_roughness: 0.35,
            cull_mode: None,
            ..default()
        }),
    });
    commands.spawn((
        Prompt,
        label("", &icons.1, 18.),
        Node {
            display: Display::None,
            position_type: PositionType::Absolute,
            bottom: px(150),
            left: percent(50),
            margin: UiRect::left(px(-220)),
            width: px(440),
            padding: UiRect::axes(px(12), px(8)),
            justify_content: JustifyContent::Center,
            ..default()
        },
        TextLayout::new_with_justify(Justify::Center),
        BackgroundColor(Color::srgba(0.02, 0.03, 0.04, 0.82)),
        GlobalZIndex(45),
    ));
    commands
        .spawn((
            ChestWindow,
            crate::ui::WorldInputBlocker,
            Node {
                display: Display::None,
                position_type: PositionType::Absolute,
                right: px(18),
                top: px(90),
                width: px(420),
                padding: UiRect::all(px(18)),
                border: UiRect::all(px(1)),
                flex_direction: FlexDirection::Column,
                row_gap: px(10),
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(GOLD),
            // Above the inventory window (11): bag stacks are put in with a right click there.
            GlobalZIndex(12),
        ))
        .with_children(|p| {
            p.spawn(label("Сундук", &icons.1, 20.));
            p.spawn((Content, label("", &icons.1, 13.)));
            p.spawn(Node {
                display: Display::Grid,
                grid_template_columns: RepeatedGridTrack::px(6, 54.),
                grid_template_rows: RepeatedGridTrack::px(5, 54.),
                column_gap: px(6),
                row_gap: px(6),
                ..default()
            })
            .with_children(|p| {
                for n in 0..objects::CHEST_CELLS {
                    p.spawn((
                        Button,
                        Action::Cell(n),
                        ItemHint(0),
                        Node {
                            border: UiRect::all(px(1)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.02, 0.03, 0.04, 0.6)),
                        BorderColor::all(crate::ui::BUTTON),
                    ))
                    .with_children(|p| {
                        p.spawn((
                            CellIcon(n),
                            ImageNode::default(),
                            Node {
                                width: percent(100),
                                height: percent(100),
                                ..default()
                            },
                        ));
                        p.spawn((
                            CellText(n),
                            label("", &icons.1, 12.),
                            Node {
                                position_type: PositionType::Absolute,
                                right: px(3),
                                bottom: px(1),
                                ..default()
                            },
                        ));
                    });
                }
            });
            p.spawn(Node {
                flex_wrap: FlexWrap::Wrap,
                column_gap: px(5),
                row_gap: px(5),
                ..default()
            })
            .with_children(|row| {
                for (action, title) in [
                    (Action::TakeCoins, "Взять монеты"),
                    (Action::PutCoins, "Положить монеты"),
                    (Action::Lift, "Поднять сундук"),
                    (Action::Price(10), "Цена 10 м"),
                    (Action::Price(100), "Цена 1 с"),
                    (Action::Close, "Закрыть"),
                ] {
                    row.spawn((
                        Button,
                        action,
                        Node {
                            padding: UiRect::all(px(8)),
                            ..default()
                        },
                        BackgroundColor(BUTTON),
                    ))
                    .with_children(|b| {
                        b.spawn(label(title, &icons.1, 13.));
                    });
                }
            });
            p.spawn(label(
                "Клик по ячейке — забрать стопку. ПКМ по вещи в рюкзаке [I] — положить.",
                &icons.1,
                12.,
            ));
        });
}

/// Keeps one 3D entity per visible chest and ground stack.
pub fn sync(
    mut commands: Commands,
    network: Res<Network>,
    world: Res<Terrain>,
    looks: Res<Looks>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut shown: Local<HashMap<Shown, Entity>>,
) {
    if !network.is_changed() {
        return;
    }
    let online = network.mode == Mode::Online;
    let mut wanted: HashMap<Shown, ([f32; 2], f32, Option<u8>, f32)> = HashMap::new();
    if online {
        for c in &network.containers {
            let kind = (c.kind == items::CART).then_some(items::CART);
            wanted.insert(Shown::Container(c.id), (c.position, c.yaw, kind, 0.));
        }
        for l in &network.ground {
            wanted.insert(Shown::Loose(l.id), (l.position, 0., Some(l.kind), 0.));
        }
        for block in &network.blocks {
            wanted.insert(
                Shown::Block(block.id),
                (block.position, 0., None, f32::from(block.level) * 1.6),
            );
        }
    }
    shown.retain(|key, entity| {
        let keep = wanted.contains_key(key);
        if !keep {
            commands.entity(*entity).despawn();
        }
        keep
    });
    for (key, (p, yaw, kind, lift)) in wanted {
        if shown.contains_key(&key) {
            continue;
        }
        let mut at = Vec3::new(p[0], world.0.surface_height(p[0], p[1]) + lift, p[1]);
        if let Shown::Loose(id) = key {
            // Several stacks dropped on one spot stay distinguishable.
            let angle = id as f32 * 2.4;
            at += Vec3::new(angle.cos(), 0., angle.sin()) * 0.3;
        }
        let mut blade = match key {
            Shown::Loose(id) if kind == Some(items::CASTING) => network
                .castings
                .iter()
                .find(|body| body.item == id)
                .and_then(|body| blade_mesh(&body.spans)),
            _ => None,
        };
        let entity = commands
            .spawn((
                key,
                Transform::from_translation(at).with_rotation(Quat::from_rotation_y(yaw)),
                Visibility::default(),
            ))
            .with_children(|p| {
                let part = |mesh: &Handle<Mesh>, material: &Handle<StandardMaterial>, y: f32| {
                    (
                        Mesh3d(mesh.clone()),
                        MeshMaterial3d(material.clone()),
                        Transform::from_xyz(0., y, 0.),
                    )
                };
                if let Shown::Block(_) = key {
                    p.spawn(part(&looks.post, &looks.wood, 0.8));
                }
                match kind {
                    None if matches!(key, Shown::Block(_)) => {}
                    None => {
                        p.spawn(part(&looks.body, &looks.wood, 0.25));
                        p.spawn(part(&looks.lid, &looks.wood, 0.57));
                        for x in [-0.3, 0.3] {
                            let mut band = part(&looks.band, &looks.iron, 0.33);
                            band.2.translation.x = x;
                            p.spawn(band);
                        }
                        let mut clasp = part(&looks.clasp, &looks.iron, 0.47);
                        clasp.2.translation.z = 0.3;
                        p.spawn(clasp);
                    }
                    Some(k) if k == items::CART => {
                        p.spawn(part(&looks.post, &looks.wood, 0.35));
                    }
                    Some(k) if items::coin_value(k).is_some() => {
                        p.spawn(part(&looks.pile, &looks.coin, 0.02));
                    }
                    Some(k) if k == items::CASTING => match blade.take() {
                        Some(mesh) => {
                            p.spawn((
                                Mesh3d(meshes.add(mesh)),
                                MeshMaterial3d(looks.bronze.clone()),
                            ));
                        }
                        None => {
                            let mut sack = part(&looks.sack, &looks.cloth, 0.2);
                            sack.2.scale = Vec3::new(1.6, 1.1, 1.6);
                            p.spawn(sack);
                        }
                    },
                    Some(_) => {
                        let mut sack = part(&looks.sack, &looks.cloth, 0.2);
                        sack.2.scale = Vec3::new(1.6, 1.1, 1.6);
                        p.spawn(sack);
                    }
                }
            })
            .id();
        shown.insert(key, entity);
    }
}

fn blade_mesh(spans: &[mortown_protocol::casting::Span]) -> Option<Mesh> {
    let vertices = mortown_protocol::casting::blank_shell(spans);
    if vertices.is_empty() {
        return None;
    }
    Some(
        Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vertices)
        .with_computed_flat_normals(),
    )
}

enum Near {
    Loose(objects::Loose),
    Container(objects::Container),
    Block(mortown_protocol::work::Block),
    Node(mortown_protocol::work::Node),
    Plot(mortown_protocol::work::Plot),
}
fn nearest(network: &Network, at: Vec3) -> Option<Near> {
    let d = |p: [f32; 2]| Vec2::new(p[0] - at.x, p[1] - at.z).length();
    let closest = |items: Vec<(f32, Near)>| {
        items
            .into_iter()
            .filter(|(distance, _)| *distance <= objects::REACH)
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, near)| near)
    };
    // A stack on the ground is what [E] picks up, even when a source or a stall is closer.
    if let Some(item) = closest(
        network
            .ground
            .iter()
            .map(|l| (d(l.position), Near::Loose(*l)))
            .collect(),
    ) {
        return Some(item);
    }
    if let Some(container) = closest(
        network
            .containers
            .iter()
            .map(|c| (d(c.position), Near::Container(*c)))
            .collect(),
    ) {
        return Some(container);
    }
    if let Some(block) = closest(
        network
            .blocks
            .iter()
            .map(|block| (d(block.position), Near::Block(*block)))
            .collect(),
    ) {
        return Some(block);
    }
    closest(
        network
            .nodes
            .iter()
            .map(|n| (d(n.position), Near::Node(*n)))
            .chain(
                network
                    .plots
                    .iter()
                    .map(|n| (d(n.position), Near::Plot(*n))),
            )
            .collect(),
    )
}
fn name(kind: u8) -> &'static str {
    items::definition(kind).map_or("Предмет", |d| d.name)
}
fn pickup_block(network: &Network, kind: u8, quantity: u32) -> Option<String> {
    let hero = network.character.as_ref()?;
    let added = items::definition(kind).map(|d| d.weight).unwrap_or(0.) * quantity as f32;
    let weight = items::carried_weight(&hero.items);
    (weight + added > items::CARRY_LIMIT + 1e-3 && added > 1e-3)
        .then(|| "Слишком тяжело: освободите рюкзак".into())
}

fn nearest_hint(network: &Network, at: Vec3) -> String {
    let d = |p: [f32; 2]| Vec2::new(p[0] - at.x, p[1] - at.z).length();
    network
        .ground
        .iter()
        .map(|item| d(item.position))
        .min_by(|a, b| a.total_cmp(b))
        .map_or_else(
            || "Рядом нет предмета, который можно поднять".into(),
            |distance| format!("Предмет на земле в {distance:.0} м. Подойдите ближе"),
        )
}
/// [E] picks up or opens the nearest thing in reach (or closes the open chest); window buttons
/// send chest requests. The chest closes when the hero walks away from it.
/// One press keeps picking up every stack still in reach, with a pause so the server
/// action budget is not exhausted.
pub fn input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mut network: ResMut<Network>,
    player: Single<&Transform, With<Player>>,
    buttons: Query<(&Action, &Interaction), Changed<Interaction>>,
    mut prompt: Single<(&mut Text, &mut Node), With<Prompt>>,
    mut chain: Local<bool>,
    mut awaiting: Local<bool>,
    mut sent_at: Local<f64>,
    mut refusal: Local<String>,
    mut speaking: Local<bool>,
) {
    let at = player.translation;
    if keys.just_pressed(KeyCode::KeyV) && network.playing() {
        *speaking = !*speaking;
        network.voice(*speaking);
        network.status = if *speaking {
            "Говорите. Слышно только рядом и без стены".into()
        } else {
            "Вы замолчали".into()
        };
    }
    if let Some(open) = network.container.as_ref() {
        let p = open.container.position;
        if Vec2::new(p[0] - at.x, p[1] - at.z).length() > objects::REACH + 1. {
            network.container = None;
        }
    }
    let near = (network.mode == Mode::Online)
        .then(|| nearest(&network, at))
        .flatten();
    prompt.1.display = Display::None;
    if network.container.is_none()
        && let Some(near) = &near
    {
        prompt.0.0 = match near {
            Near::Loose(l) => {
                if let Some(reason) = pickup_block(&network, l.kind, l.quantity) {
                    reason
                } else if refusal.is_empty() {
                    format!("[E] Подобрать: {} ×{}", name(l.kind), l.quantity)
                } else {
                    refusal.clone()
                }
            }
            Near::Container(c) if c.kind == items::STALL => match c.offer {
                Some(kind) => format!(
                    "[E] Лавка: {} · {}",
                    name(kind),
                    c.price
                        .map(items::format_coins)
                        .unwrap_or_else(|| "без цены".into())
                ),
                None => "[E] Свой прилавок".into(),
            },
            Near::Container(c) if c.kind == items::CART => "[E] Тянуть телегу".into(),
            Near::Container(c) if c.locked => "[E] Открыть сундук (заперт)".into(),
            Near::Container(_) => "[E] Открыть сундук".into(),
            Near::Block(block) => format!(
                "[E] Бить брус · ярус {} · прочность {}",
                block.level + 1,
                block.integrity
            ),
            Near::Node(n) => format!("[E] Добыть: {} (осталось {})", name(n.product), n.remaining),
            Near::Plot(p) => format!(
                "[E] {} · {}",
                match p.stage {
                    0 => "Посеять",
                    1 => "Полить",
                    _ if p.ready => "Собрать",
                    _ => "Ждать всходы",
                },
                match p.fertility {
                    3 => "плодородная",
                    2 => "обычная почва",
                    _ => "бедная почва",
                }
            ),
        };
        prompt.1.display = Display::Flex;
    }
    let now = time.elapsed_secs_f64();
    if network.object_pending && now - *sent_at > 1.5 {
        network.object_pending = false;
        *awaiting = false;
    }
    let tapped = keys.just_pressed(KeyCode::KeyE);
    if tapped && network.playing() && network.container.take().is_none() {
        *chain = true;
    }
    if *awaiting && !network.object_pending {
        *awaiting = false;
        if network.status.starts_with("Подобрано") {
            refusal.clear();
        } else {
            *chain = false;
            *refusal = network.status.clone();
        }
    }
    if *chain && network.playing() && !network.object_pending && now - *sent_at >= 0.25 {
        match &near {
            Some(Near::Block(block)) if tapped => {
                *chain = false;
                network.work_request(mortown_protocol::work::Command::Damage { block: block.id });
                *sent_at = now;
            }
            Some(Near::Loose(l)) => {
                network.object_request(Command::PickUp { item: l.id });
                *sent_at = now;
                *awaiting = network.object_pending;
                if !network.object_pending {
                    *chain = false;
                }
            }
            Some(other) if tapped => {
                *chain = false;
                match other {
                    Near::Container(c)
                        if c.kind == items::STALL
                            && c.owner != network.character.as_ref().map(|hero| hero.id)
                            && c.offer.is_some()
                            && c.price.is_some() =>
                    {
                        network.work_request(mortown_protocol::work::Command::Buy {
                            container: c.id,
                            price: c.price.unwrap_or(0),
                        });
                    }
                    Near::Container(c) if c.kind == items::CART => {
                        network.work_request(mortown_protocol::work::Command::Haul);
                    }
                    Near::Container(c) => network.object_request(Command::Open { container: c.id }),
                    Near::Node(n) => {
                        network.work_request(mortown_protocol::work::Command::Gather { node: n.id })
                    }
                    Near::Plot(p) => {
                        network.work_request(mortown_protocol::work::Command::Tend { plot: p.id })
                    }
                    Near::Loose(_) | Near::Block(_) => {}
                }
                *sent_at = now;
            }
            _ => {
                *chain = false;
                if tapped {
                    network.status = nearest_hint(&network, at);
                }
            }
        }
    }
    let Some(view) = network.container.clone() else {
        return;
    };
    let holder = Holder::Container {
        id: view.container.id,
        cells: view.cells,
    };
    let (id, revision) = (view.container.id, view.revision);
    for (action, interaction) in &buttons {
        if *interaction != Interaction::Pressed {
            continue;
        }
        let command = match action {
            Action::Close => {
                network.container = None;
                return;
            }
            Action::Price(price) => {
                network.work_request(mortown_protocol::work::Command::Price {
                    container: id,
                    price: *price,
                });
                return;
            }
            Action::Cell(n) => {
                let Some(s) = view.items.iter().find(|s| {
                    s.location
                        == (items::Location::Container {
                            container: id,
                            cell: *n,
                        })
                }) else {
                    continue;
                };
                let (item, coins) = match items::coin_value(s.kind) {
                    Some(value) => (None, value * u64::from(s.quantity)),
                    None => (
                        Some(ItemRequest {
                            item: s.id,
                            quantity: s.quantity,
                        }),
                        0,
                    ),
                };
                Command::Take {
                    container: id,
                    revision,
                    item,
                    coins,
                }
            }
            Action::TakeCoins => Command::Take {
                container: id,
                revision,
                item: None,
                coins: transfer::coins(&view.items, holder),
            },
            Action::PutCoins => Command::Put {
                container: id,
                revision,
                item: None,
                coins: network
                    .character
                    .as_ref()
                    .map_or(0, |c| transfer::coins(&c.items, Holder::Character)),
            },
            Action::Lift => Command::Lift {
                container: id,
                revision,
            },
        };
        network.object_request(command);
    }
}

#[allow(clippy::type_complexity)]
pub fn display(
    network: Res<Network>,
    icons: Res<Icons>,
    mut window: Single<&mut Node, With<ChestWindow>>,
    mut content: Single<&mut Text, (With<Content>, Without<CellText>)>,
    mut cells: Query<(&Action, &mut ItemHint)>,
    mut pictures: Query<(&CellIcon, &mut ImageNode)>,
    mut counts: Query<(&CellText, &mut Text), Without<Content>>,
) {
    let view = network
        .container
        .as_ref()
        .filter(|_| network.mode == Mode::Online);
    window.display = if view.is_some() {
        Display::Flex
    } else {
        Display::None
    };
    let Some(view) = view else {
        return;
    };
    let id = view.container.id;
    let at = |n: u16| {
        view.items.iter().find(|s| {
            s.location
                == (items::Location::Container {
                    container: id,
                    cell: n,
                })
        })
    };
    let weight: f32 = view.items.iter().map(items::Stack::weight).sum();
    let coins = transfer::coins(
        &view.items,
        Holder::Container {
            id,
            cells: view.cells,
        },
    );
    content.0 = format!(
        "Занято {} / {} ячеек · Вес {:.1} / {:.0} кг · Монеты: {}{}",
        view.items.len(),
        view.cells,
        weight,
        objects::CHEST_WEIGHT,
        items::format_coins(coins),
        if network.object_pending {
            " · Сохраняем…"
        } else {
            ""
        }
    );
    for (action, mut hint) in &mut cells {
        if let Action::Cell(n) = action {
            hint.0 = at(*n).map_or(0, |s| s.kind);
        }
    }
    for (CellIcon(n), mut image) in &mut pictures {
        match at(*n) {
            Some(s) => {
                image.image = icons.0[item_icon(s.kind)].clone();
                image.color = Color::WHITE;
            }
            None => image.color = Color::NONE,
        }
    }
    for (CellText(n), mut text) in &mut counts {
        text.0 = at(*n)
            .filter(|s| s.quantity > 1)
            .map_or(String::new(), |s| s.quantity.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ground_item_wins_over_a_closer_source() {
        let mut network = Network::default();
        network.ground.push(objects::Loose {
            id: 7,
            kind: 30,
            quantity: 1,
            position: [2., 0.],
        });
        network.nodes.push(mortown_protocol::work::Node {
            id: 1,
            product: 30,
            position: [0.4, 0.],
            remaining: 20,
        });
        network.containers.push(objects::Container {
            id: 3,
            kind: items::STALL,
            position: [0.2, 0.],
            yaw: 0.,
            locked: false,
            price: Some(10),
            offer: Some(30),
            owner: Some(1),
        });
        let near = nearest(&network, Vec3::ZERO);
        assert!(matches!(near, Some(Near::Loose(l)) if l.id == 7));
    }
}
