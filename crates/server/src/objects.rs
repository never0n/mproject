//! Containers placed in the world and stacks lying on the ground. Contents are reachable only in
//! person: within reach of the object, alive, and for a locked container with its key in the
//! bag. Every change runs in one transaction and bumps the container revision.
use crate::{
    inventory::{self, Change, database_error},
    store::ExchangeFailure,
};
use mortown_protocol::{
    Actor, Character,
    items::{self, Location, Stack},
    objects::{self, CastingBody, Command, Container, Loose, View},
    transfer::{self, Holder},
};
use rand_core::{OsRng, RngCore};
use sqlx::{PgConnection, Row};

pub(crate) fn lock_code() -> i64 {
    (OsRng.next_u64() >> 1).max(1) as i64
}
struct Object {
    id: i64,
    kind: u8,
    at: [f32; 2],
    yaw: f32,
    lock: Option<i64>,
    revision: i64,
    owner: Option<i64>,
}
impl Object {
    fn public(&self) -> Container {
        Container {
            id: self.id,
            kind: self.kind,
            position: self.at,
            yaw: self.yaw,
            locked: self.lock.is_some(),
            price: None,
            offer: None,
            owner: self.owner,
        }
    }
    fn holder(&self) -> Holder {
        Holder::Container {
            id: self.id,
            cells: objects::CHEST_CELLS,
        }
    }
}
fn object(row: &sqlx::postgres::PgRow) -> Object {
    Object {
        id: row.get("id"),
        kind: row.get::<i16, _>("kind") as u8,
        at: [row.get("x"), row.get("z")],
        yaw: row.get("yaw"),
        lock: row.get("lock_code"),
        revision: row.get("revision"),
        owner: row.get("placed_by"),
    }
}
async fn locked(conn: &mut PgConnection, id: i64) -> Result<Object, String> {
    sqlx::query("SELECT * FROM world_objects WHERE id=$1 FOR UPDATE")
        .bind(id)
        .fetch_optional(conn)
        .await
        .map_err(database_error)?
        .map(|row| object(&row))
        .ok_or_else(|| "Хранилища здесь больше нет".into())
}
/// Positions of every placed object, to keep new ones apart.
pub(crate) async fn placed(conn: &mut PgConnection) -> Result<Vec<[f32; 2]>, String> {
    Ok(sqlx::query("SELECT x,z FROM world_objects")
        .fetch_all(conn)
        .await
        .map_err(database_error)?
        .iter()
        .map(|r| [r.get("x"), r.get("z")])
        .collect())
}
/// First walkable point on rings around `center`, starting at `radius`, at least 2 m from
/// `taken` points.
pub(crate) fn spot(
    world: &mortown_world::World,
    center: [f32; 2],
    radius: f32,
    taken: &[[f32; 2]],
) -> [f32; 2] {
    for ring in 0..60 {
        let r = radius + ring as f32 * 2.;
        for step in 0..36 {
            let angle = step as f32 * std::f32::consts::TAU / 36.;
            let p = [center[0] + r * angle.cos(), center[1] + r * angle.sin()];
            if world.walkable(p[0], p[1])
                && taken.iter().all(|t| mortown_world::distance(*t, p) >= 2.)
            {
                return p;
            }
        }
    }
    mortown_world::SPAWN
}
/// A new locked chest at `at` and its only key (not yet stored).
pub(crate) async fn create_locked(
    conn: &mut PgConnection,
    at: [f32; 2],
) -> Result<(i64, Stack), String> {
    let lock = lock_code();
    let id = create(conn, items::CHEST, at, 0., None, lock).await?;
    let mut key = Stack::new(items::KEY, 1, Location::Purse);
    key.lock = Some(lock);
    Ok((id, key))
}
async fn create(
    conn: &mut PgConnection,
    kind: u8,
    at: [f32; 2],
    yaw: f32,
    by: Option<i64>,
    lock: i64,
) -> Result<i64, String> {
    sqlx::query_scalar(
        "INSERT INTO world_objects(kind,x,z,yaw,lock_code,placed_by) VALUES($1,$2,$3,$4,$5,$6) RETURNING id",
    )
    .bind(i16::from(kind))
    .bind(at[0])
    .bind(at[1])
    .bind(yaw)
    .bind(lock)
    .bind(by)
    .fetch_one(conn)
    .await
    .map_err(database_error)
}
/// Containers and ground stacks around a point, as everyone may see them.
pub(crate) async fn nearby(
    conn: &mut PgConnection,
    at: [f32; 2],
) -> Result<(Vec<Container>, Vec<Loose>), String> {
    let r = objects::VIEW_RANGE;
    let containers = sqlx::query(
        "SELECT * FROM world_objects WHERE x BETWEEN $1-$3 AND $1+$3 AND z BETWEEN $2-$3 AND $2+$3 ORDER BY id LIMIT 200",
    )
    .bind(at[0])
    .bind(at[1])
    .bind(r)
    .fetch_all(&mut *conn)
    .await
    .map_err(database_error)?
    .iter()
    .map(|row| object(row).public())
    .collect();
    let ground = inventory::ground(conn, at, r)
        .await?
        .into_iter()
        .filter_map(|s| match s.location {
            Location::Ground { x, z } => Some(Loose {
                id: s.id,
                kind: s.kind,
                quantity: s.quantity,
                position: [x, z],
            }),
            _ => None,
        })
        .collect();
    Ok((containers, ground))
}

/// Stored fill of castings in `ground`. An empty cell stays out of the list's mesh.
pub(crate) async fn casting_bodies(
    conn: &mut PgConnection,
    ground: &[Loose],
) -> Result<Vec<CastingBody>, String> {
    let ids: Vec<i64> = ground
        .iter()
        .filter(|item| item.kind == items::CASTING)
        .map(|item| item.id)
        .collect();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query("SELECT item_id, geometry FROM item_bodies WHERE item_id = ANY($1)")
        .bind(&ids)
        .fetch_all(conn)
        .await
        .map_err(database_error)?;
    let mut bodies = Vec::new();
    for row in rows {
        let spans =
            serde_json::from_value::<Vec<mortown_protocol::casting::Span>>(row.get("geometry"))
                .map_err(|_| "Не удалось прочитать форму отливки")?;
        if spans.is_empty() {
            continue;
        }
        bodies.push(CastingBody {
            item: row.get("item_id"),
            spans,
        });
    }
    Ok(bodies)
}
fn own_stall(o: &Object, actor: i64) -> Result<(), String> {
    if o.kind == items::STALL && o.owner != Some(actor) {
        Err("Чужой прилавок: товар покупают, а не забирают".into())
    } else {
        Ok(())
    }
}
fn opens(c: &Character, lock: Option<i64>) -> bool {
    lock.is_none_or(|code| {
        c.items
            .iter()
            .any(|s| s.kind == items::KEY && s.lock == Some(code) && s.carried())
    })
}
fn feet(actor: &Actor) -> [f32; 2] {
    [actor.position[0], actor.position[2]]
}
fn within_reach(actor: &Actor, at: [f32; 2]) -> Result<(), String> {
    if mortown_world::distance(feet(actor), at) > objects::REACH {
        Err("Подойдите ближе".into())
    } else {
        Ok(())
    }
}
fn container_weight_ok(before: &[Stack], after: &[Stack]) -> Result<(), String> {
    let weight = |s: &[Stack]| s.iter().map(Stack::weight).sum::<f32>();
    if weight(after) > objects::CHEST_WEIGHT && weight(after) > weight(before) + 1e-3 {
        return Err("Сундук не выдержит такой вес".into());
    }
    Ok(())
}
pub(crate) struct Outcome {
    pub view: Option<View>,
    pub message: String,
    pub update: Option<(Character, Actor)>,
    /// Something visible to others moved (ground stacks or containers).
    pub changed: bool,
}
/// Caller holds the simulation lock and replaces the actor only after this commits.
pub(crate) async fn apply(
    conn: &mut PgConnection,
    world: &mortown_world::World,
    actor: Actor,
    command: Command,
) -> Result<Outcome, ExchangeFailure> {
    if actor.hp <= 0. {
        return Err("Сначала возродитесь".into());
    }
    inventory::transaction(conn, async |tx| {
        let mut actor = actor;
        let mut c = inventory::load(tx, &actor).await?;
        let before = c.items.clone();
        let owner = Some(actor.id);
        let mut view = None;
        let mut changed = true;
        let message = match command {
            Command::Drop { item, quantity } => {
                let mut s = transfer::split(&mut c.items, Holder::Character, item, quantity)?;
                let at = feet(&actor);
                s.location = Location::Ground { x: at[0], z: at[1] };
                let ground = [s];
                inventory::sync(
                    &mut *tx,
                    &[
                        Change {
                            owner,
                            before: &before,
                            after: &c.items,
                        },
                        Change {
                            owner: None,
                            before: &[],
                            after: &ground,
                        },
                    ],
                )
                .await?;
                format!("Положено на землю: {} ×{quantity}", name(s.kind))
            }
            Command::PickUp { item } => {
                let s = inventory::loose(&mut *tx, item)
                    .await?
                    .ok_or("Эту вещь уже подобрали")?;
                if let Location::Ground { x, z } = s.location {
                    within_reach(&actor, [x, z])?;
                }
                transfer::put(&mut c.items, Holder::Character, s)?;
                transfer::check_carry(&before, &c.items)?;
                inventory::sync(
                    &mut *tx,
                    &[
                        Change {
                            owner,
                            before: &before,
                            after: &c.items,
                        },
                        Change {
                            owner: None,
                            before: &[s],
                            after: &[],
                        },
                    ],
                )
                .await?;
                format!("Подобрано: {} ×{}", name(s.kind), s.quantity)
            }
            Command::Place { item } => {
                let chest = transfer::split(&mut c.items, Holder::Character, item, 1)?;
                if chest.kind == items::STALL {
                    let at = feet(&actor);
                    if !world.walkable(at[0], at[1]) || world.hall_at(at[0], at[1]).is_some() {
                        return Err("Здесь нельзя поставить прилавок".into());
                    }
                    let near = placed(&mut *tx).await?;
                    if near.iter().any(|p| mortown_world::distance(*p, at) < 1.5) {
                        return Err("Здесь уже стоит хранилище".into());
                    }
                    sqlx::query(
                        "INSERT INTO world_objects(kind,x,z,yaw,placed_by) VALUES($1,$2,$3,$4,$5)",
                    )
                    .bind(i16::from(items::STALL))
                    .bind(at[0])
                    .bind(at[1])
                    .bind(actor.yaw)
                    .bind(owner)
                    .execute(&mut *tx)
                    .await
                    .map_err(database_error)?;
                    inventory::sync(
                        &mut *tx,
                        &[Change {
                            owner,
                            before: &before,
                            after: &c.items,
                        }],
                    )
                    .await?;
                    "Прилавок поставлен. Выложите товар и назначьте цену".into()
                } else if chest.kind == items::CART {
                    let at = feet(&actor);
                    if !world.walkable(at[0], at[1]) || world.hall_at(at[0], at[1]).is_some() {
                        return Err("Здесь нельзя поставить телегу".into());
                    }
                    let near = placed(&mut *tx).await?;
                    if near.iter().any(|p| mortown_world::distance(*p, at) < 1.5) {
                        return Err("Здесь уже стоит хранилище".into());
                    }
                    sqlx::query(
                        "INSERT INTO world_objects(kind,x,z,yaw,placed_by) VALUES($1,$2,$3,$4,$5)",
                    )
                    .bind(i16::from(items::CART))
                    .bind(at[0])
                    .bind(at[1])
                    .bind(actor.yaw)
                    .bind(owner)
                    .execute(&mut *tx)
                    .await
                    .map_err(database_error)?;
                    inventory::sync(
                        &mut *tx,
                        &[Change {
                            owner,
                            before: &before,
                            after: &c.items,
                        }],
                    )
                    .await?;
                    "Телега поставлена. Сундук рядом поедет вместе с ней".into()
                } else if chest.kind != items::CHEST {
                    return Err("Поставить можно только сундук, прилавок или телегу".into());
                } else {
                    let at = feet(&actor);
                    if !world.walkable(at[0], at[1]) || world.hall_at(at[0], at[1]).is_some() {
                        return Err("Здесь нельзя поставить сундук".into());
                    }
                    let near = placed(&mut *tx).await?;
                    if near.iter().any(|p| mortown_world::distance(*p, at) < 1.5) {
                        return Err("Здесь уже стоит хранилище".into());
                    }
                    // A chest keeps its lock when lifted and placed again; a new one gets a lock and key.
                    let lock = match chest.lock {
                        Some(lock) => lock,
                        None => {
                            let lock = lock_code();
                            let mut key = Stack::new(items::KEY, 1, Location::Purse);
                            key.lock = Some(lock);
                            transfer::put(&mut c.items, Holder::Character, key)?;
                            lock
                        }
                    };
                    create(&mut *tx, items::CHEST, at, actor.yaw, owner, lock).await?;
                    inventory::sync(
                        &mut *tx,
                        &[Change {
                            owner,
                            before: &before,
                            after: &c.items,
                        }],
                    )
                    .await?;
                    if chest.lock.is_none() {
                        "Сундук поставлен. Ключ — в рюкзаке".into()
                    } else {
                        "Сундук поставлен".into()
                    }
                }
            }
            Command::Open { container } => {
                let o = locked(&mut *tx, container).await?;
                within_reach(&actor, o.at)?;
                own_stall(&o, actor.id)?;
                if o.kind == items::CART {
                    return Err("Телега не открывается. Груз везёт сундук на ней".into());
                }
                if !opens(&c, o.lock) {
                    return Err("Заперто: нужен ключ от этого сундука".into());
                }
                let contents = inventory::contents(&mut *tx, o.id).await?;
                changed = false;
                view = Some(show(&o, contents));
                return Ok(Outcome {
                    view,
                    message: "Сундук открыт".into(),
                    update: None,
                    changed,
                });
            }
            Command::Put {
                container,
                revision,
                item,
                coins,
            }
            | Command::Take {
                container,
                revision,
                item,
                coins,
            } => {
                let put = matches!(command, Command::Put { .. });
                let mut o = locked(&mut *tx, container).await?;
                own_stall(&o, actor.id)?;
                if o.revision as u64 != revision {
                    return Err("Содержимое сундука изменилось. Посмотрите ещё раз".into());
                }
                within_reach(&actor, o.at)?;
                if !opens(&c, o.lock) {
                    return Err("Заперто: нужен ключ от этого сундука".into());
                }
                if item.is_none() && coins == 0 {
                    return Err("Выберите вещь или сумму".into());
                }
                let stored = inventory::contents(&mut *tx, o.id).await?;
                let mut contents = stored.clone();
                let holder = o.holder();
                let (from, from_holder, to, to_holder) = if put {
                    (&mut c.items, Holder::Character, &mut contents, holder)
                } else {
                    (&mut contents, holder, &mut c.items, Holder::Character)
                };
                if let Some(request) = item {
                    let s = transfer::split(from, from_holder, request.item, request.quantity)?;
                    transfer::put(to, to_holder, s)?;
                }
                if coins > 0 {
                    transfer::pay(from, from_holder, coins)?;
                    let total = transfer::coins(to, to_holder)
                        .checked_add(coins)
                        .ok_or("Слишком много монет")?;
                    transfer::set_coins(to, to_holder, total)?;
                }
                transfer::check_carry(&before, &c.items)?;
                container_weight_ok(&stored, &contents)?;
                inventory::sync(
                    &mut *tx,
                    &[
                        Change {
                            owner,
                            before: &before,
                            after: &c.items,
                        },
                        Change {
                            owner: None,
                            before: &stored,
                            after: &contents,
                        },
                    ],
                )
                .await?;
                o.revision = sqlx::query_scalar(
                    "UPDATE world_objects SET revision=revision+1 WHERE id=$1 RETURNING revision",
                )
                .bind(o.id)
                .fetch_one(&mut *tx)
                .await
                .map_err(database_error)?;
                changed = false;
                let contents = inventory::contents(&mut *tx, o.id).await?;
                view = Some(show(&o, contents));
                if put {
                    "Положено в сундук"
                } else {
                    "Взято из сундука"
                }
                .into()
            }
            Command::Lift {
                container,
                revision,
            } => {
                let o = locked(&mut *tx, container).await?;
                own_stall(&o, actor.id)?;
                if o.revision as u64 != revision {
                    return Err("Содержимое сундука изменилось. Посмотрите ещё раз".into());
                }
                within_reach(&actor, o.at)?;
                if !opens(&c, o.lock) {
                    return Err("Заперто: нужен ключ от этого сундука".into());
                }
                if !inventory::contents(&mut *tx, o.id).await?.is_empty() {
                    return Err("Сначала опустошите сундук".into());
                }
                sqlx::query("DELETE FROM world_objects WHERE id=$1")
                    .bind(o.id)
                    .execute(&mut *tx)
                    .await
                    .map_err(database_error)?;
                let mut chest = Stack::new(o.kind, 1, Location::Purse);
                chest.lock = o.lock;
                transfer::put(&mut c.items, Holder::Character, chest)?;
                transfer::check_carry(&before, &c.items)?;
                inventory::sync(
                    &mut *tx,
                    &[Change {
                        owner,
                        before: &before,
                        after: &c.items,
                    }],
                )
                .await?;
                "Сундук поднят: замок и ключ прежние".into()
            }
        };
        let sheet = inventory::finish(&mut *tx, &mut actor, &c, &message).await?;
        Ok(Outcome {
            view,
            message,
            update: Some((sheet, actor)),
            changed,
        })
    })
    .await
}
fn show(o: &Object, items: Vec<Stack>) -> View {
    View {
        container: o.public(),
        revision: o.revision as u64,
        cells: objects::CHEST_CELLS,
        items,
    }
}
fn name(kind: u8) -> &'static str {
    items::definition(kind).map_or("Предмет", |d| d.name)
}
/// Stores earned drops and coins: in the bag and purse while the limits allow, otherwise on the
/// ground at the character's feet. Returns the new sheet and whether the ground changed.
pub(crate) async fn grant(
    conn: &mut PgConnection,
    actor: &mut Actor,
    grants: &[(u8, u32)],
) -> Result<(Character, bool), ExchangeFailure> {
    inventory::transaction(conn, async |tx| {
        let mut c = inventory::load(tx, actor).await?;
        let before = c.items.clone();
        let at = feet(actor);
        let mut ground = Vec::new();
        for &(kind, quantity) in grants {
            if items::definition(kind).is_none() || quantity == 0 {
                continue;
            }
            let parts = if items::stackable(kind) {
                vec![quantity]
            } else {
                vec![1; quantity as usize]
            };
            for part in parts {
                let s = Stack::new(kind, part, Location::Purse);
                let mut next = c.items.clone();
                if transfer::put(&mut next, Holder::Character, s).is_ok()
                    && transfer::check_carry(&c.items, &next).is_ok()
                {
                    c.items = next;
                } else {
                    ground.push(Stack::new(
                        kind,
                        part,
                        Location::Ground { x: at[0], z: at[1] },
                    ));
                }
            }
        }
        if !ground.is_empty() {
            actor
                .reward_status
                .push_str(" · Не унести: добыча осталась на земле");
        }
        inventory::sync(
            &mut *tx,
            &[
                Change {
                    owner: Some(actor.id),
                    before: &before,
                    after: &c.items,
                },
                Change {
                    owner: None,
                    before: &[],
                    after: &ground,
                },
            ],
        )
        .await?;
        let sheet = inventory::finish(&mut *tx, actor, &c, "").await?;
        Ok((sheet, !ground.is_empty()))
    })
    .await
}
