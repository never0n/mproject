//! Gathering, a blade plan, a plot, food and a player stall. The caller holds the simulation
//! lock. Gold ore stays ore: minting it into coins is not an approved transition.
use crate::{
    inventory::{self, Change, database_error},
    objects::Outcome,
    store::ExchangeFailure,
};
use mortown_protocol::{
    Actor, Character,
    items::{self, Location, Stack},
    objects::Container,
    transfer::{self, Holder},
    work::{self, Command, Node, Plot},
};
use sqlx::{PgConnection, PgPool, Row};

pub async fn seed(pool: &PgPool, world: &mortown_world::World) -> Result<(), String> {
    let mut conn = pool.acquire().await.map_err(crate::store::database_error)?;
    let nodes: i64 = sqlx::query_scalar("SELECT count(*) FROM resource_nodes")
        .fetch_one(&mut *conn)
        .await
        .map_err(database_error)?;
    if nodes == 0 {
        let stock = work::node_stock();
        let home = mortown_world::SPAWN;
        for (product, offset) in [
            (work::WOOD, [6., 2.]),
            (items::IRON, [8., -1.]),
            (work::GRAIN, [6., 5.]),
            (work::WATER, [4., 7.]),
            (work::SALT, [9., 3.]),
            (work::GOLD_ORE, [11., 1.]),
        ] {
            let at = spot(world, [home[0] + offset[0], home[1] + offset[1]]);
            sqlx::query("INSERT INTO resource_nodes(product,x,z,remaining) VALUES($1,$2,$3,$4)")
                .bind(i16::from(product))
                .bind(at[0])
                .bind(at[1])
                .bind(stock)
                .execute(&mut *conn)
                .await
                .map_err(database_error)?;
        }
    }
    let plots: i64 = sqlx::query_scalar("SELECT count(*) FROM plots")
        .fetch_one(&mut *conn)
        .await
        .map_err(database_error)?;
    if plots == 0 {
        let home = mortown_world::SPAWN;
        for offset in [[6., -5.], [9., -5.]] {
            let at = spot(world, [home[0] + offset[0], home[1] + offset[1]]);
            sqlx::query("INSERT INTO plots(x,z) VALUES($1,$2)")
                .bind(at[0])
                .bind(at[1])
                .execute(&mut *conn)
                .await
                .map_err(database_error)?;
        }
    }
    Ok(())
}
fn spot(world: &mortown_world::World, mut at: [f32; 2]) -> [f32; 2] {
    if world.walkable(at[0], at[1]) {
        return at;
    }
    for ring in 1..16 {
        for step in 0..12 {
            let angle = step as f32 * std::f32::consts::TAU / 12.;
            let candidate = [
                at[0] + angle.cos() * ring as f32,
                at[1] + angle.sin() * ring as f32,
            ];
            if world.walkable(candidate[0], candidate[1]) {
                return candidate;
            }
        }
    }
    at = mortown_world::SPAWN;
    at
}

pub async fn nearby(
    conn: &mut PgConnection,
    at: [f32; 2],
) -> Result<(Vec<Node>, Vec<Plot>), String> {
    let radius = mortown_protocol::objects::VIEW_RANGE;
    let nodes = sqlx::query(
        "SELECT id, product, x, z, remaining FROM resource_nodes \
         WHERE x BETWEEN $1-$3 AND $1+$3 AND z BETWEEN $2-$3 AND $2+$3 AND remaining > 0 \
         ORDER BY id LIMIT 80",
    )
    .bind(at[0])
    .bind(at[1])
    .bind(radius)
    .fetch_all(&mut *conn)
    .await
    .map_err(database_error)?
    .iter()
    .map(|row| Node {
        id: row.get("id"),
        product: row.get::<i16, _>("product") as u8,
        position: [row.get("x"), row.get("z")],
        remaining: row.get("remaining"),
    })
    .collect();
    let plots = sqlx::query(
        "SELECT id, x, z, stage, owner_id, fertility, COALESCE(ready_at <= now(), false) AS ready FROM plots \
         WHERE x BETWEEN $1-$3 AND $1+$3 AND z BETWEEN $2-$3 AND $2+$3 ORDER BY id LIMIT 40",
    )
    .bind(at[0])
    .bind(at[1])
    .bind(radius)
    .fetch_all(&mut *conn)
    .await
    .map_err(database_error)?
    .iter()
    .map(|row| Plot {
        id: row.get("id"),
        stage: row.get::<i16, _>("stage") as u8,
        position: [row.get("x"), row.get("z")],
        ready: row.get::<bool, _>("ready") && row.get::<i16, _>("stage") == 2,
        owner: row.get("owner_id"),
        fertility: row.get::<i16, _>("fertility") as u8,
    })
    .collect();
    Ok((nodes, plots))
}
/// Fills the public offer of each stall. Contents stay hidden until the owner opens it.
pub async fn offers(conn: &mut PgConnection, containers: &mut [Container]) -> Result<(), String> {
    for container in containers.iter_mut().filter(|c| c.kind == items::STALL) {
        container.price = sqlx::query_scalar("SELECT price FROM stall_prices WHERE object_id=$1")
            .bind(container.id)
            .fetch_optional(&mut *conn)
            .await
            .map_err(database_error)?
            .map(|price: i64| price as u64);
        container.offer = sqlx::query_scalar(
            "SELECT kind FROM items WHERE container_id=$1 AND kind NOT IN (11, 12, 27) ORDER BY id LIMIT 1",
        )
        .bind(container.id)
        .fetch_optional(&mut *conn)
        .await
        .map_err(database_error)?
        .map(|kind: i16| kind as u8);
    }
    Ok(())
}

pub async fn apply(
    conn: &mut PgConnection,
    world: &mortown_world::World,
    actor: Actor,
    command: Command,
) -> Result<Outcome, ExchangeFailure> {
    if actor.hp <= 0. || actor.downed || actor.bound {
        return Err("Сейчас вы не можете работать".into());
    }
    inventory::transaction(conn, async |tx| {
        change(tx, world, actor, command)
            .await
            .map_err(ExchangeFailure::Rejected)
    })
    .await
}

async fn change(
    tx: &mut PgConnection,
    world: &mortown_world::World,
    mut actor: Actor,
    command: Command,
) -> Result<Outcome, String> {
    if let Command::Buy { container, price } = command {
        return buy(tx, world, actor, container, price).await;
    }
    let mut character = inventory::load(&mut *tx, &actor).await?;
    let before = character.items.clone();
    let mut letter = None;
    let mut bird = None;
    let message = match command {
        Command::Gather { node } => gather(tx, world, &mut actor, &mut character, node).await?,
        Command::Tend { plot } => tend(tx, world, &mut actor, &mut character, plot).await?,
        Command::Design {
            material,
            geometry,
            process,
        } => design(&mut actor, &mut character, material, geometry, process)?,
        Command::WriteBlueprint => write_plan(&mut actor, &mut character)?,
        Command::CraftPlan { paper } => craft_plan(&mut actor, &mut character, paper)?,
        Command::Cook => cook(&mut actor, &mut character)?,
        Command::Salt { item } => salt(&mut actor, &mut character, item)?,
        Command::Eat { item } => eat(&mut actor, &mut character, item)?,
        Command::Drink { item } => drink(&mut actor, &mut character, item)?,
        Command::MakeStall => make_stall(&mut actor, &mut character)?,
        Command::MakeTimber => crate::build::make_timber(&mut character)?,
        Command::PlaceTimber => crate::build::place(tx, world, &actor, &mut character).await?,
        Command::Damage { block } => crate::build::damage(tx, world, &actor, block).await?,
        Command::MakeCart => crate::build::make_cart(&mut character)?,
        Command::Haul => crate::build::haul(tx, world, &mut actor).await?,
        Command::WriteDocument { text } => {
            letter = Some(crate::society::write_letter(&mut character, &text)?);
            "Письмо написано. Его можно передать или отправить птицей".into()
        }
        Command::DispatchDocument => {
            bird = Some(crate::society::dispatch_letter(
                world,
                &actor,
                &mut character,
            )?);
            String::new()
        }
        Command::Price { container, price } => {
            price_of(tx, world, &actor, container, price).await?
        }
        Command::Buy { .. } => unreachable!(),
    };
    sync_actor(tx, &mut actor, &mut character, &before).await?;
    if let Some(text) = letter {
        crate::society::attach_letter(tx, actor.id, &text).await?;
    }
    let message = if let Some((item, dest)) = bird {
        crate::society::launch(tx, item, dest).await?
    } else {
        message
    };
    let sheet = crate::store::sheet(&mut *tx, actor.id).await?;
    actor.equipment = items::EquipmentAppearance::from_items(&sheet.items);
    Ok(Outcome {
        view: None,
        message: message.clone(),
        update: Some((sheet, actor)),
        changed: true,
    })
}

fn reach(actor: &Actor, at: [f32; 2], world: &mortown_world::World) -> Result<(), String> {
    let from = [actor.position[0], actor.position[2]];
    if mortown_world::distance(from, at) > mortown_protocol::objects::REACH
        || !world.segment_clear(from, at)
    {
        return Err("Слишком далеко".into());
    }
    Ok(())
}
fn now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
async fn gather(
    tx: &mut PgConnection,
    world: &mortown_world::World,
    actor: &mut Actor,
    character: &mut Character,
    node: i64,
) -> Result<String, String> {
    let row =
        sqlx::query("SELECT product, x, z, remaining FROM resource_nodes WHERE id=$1 FOR UPDATE")
            .bind(node)
            .fetch_optional(&mut *tx)
            .await
            .map_err(database_error)?
            .ok_or("Источника здесь больше нет")?;
    let remaining: i32 = row.get("remaining");
    if remaining <= 0 {
        return Err("Источник иссяк".into());
    }
    reach(actor, [row.get("x"), row.get("z")], world)?;
    let product: u8 = row.get::<i16, _>("product") as u8;
    sqlx::query("UPDATE resource_nodes SET remaining = remaining - 1 WHERE id=$1")
        .bind(node)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
    transfer::put(
        &mut character.items,
        Holder::Character,
        Stack::new(product, 1, Location::Bag { cell: 0 }),
    )?;
    let name = items::definition(product)
        .map(|d| d.name)
        .unwrap_or("ресурс");
    actor.reward_status = format!("Добыто: {name}");
    Ok(actor.reward_status.clone())
}

async fn tend(
    tx: &mut PgConnection,
    world: &mortown_world::World,
    actor: &mut Actor,
    character: &mut Character,
    plot: i64,
) -> Result<String, String> {
    let row = sqlx::query(
        "SELECT x, z, stage, owner_id, fertility, COALESCE(ready_at <= now(), false) AS ready FROM plots WHERE id=$1 FOR UPDATE",
    )
        .bind(plot)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .ok_or("Грядки здесь нет")?;
    reach(actor, [row.get("x"), row.get("z")], world)?;
    let owner: Option<i64> = row.get("owner_id");
    if owner.is_some() && owner != Some(actor.id) {
        return Err("Это чужая грядка".into());
    }
    let stage: i16 = row.get("stage");
    let ready: bool = row.get("ready");
    let message = if stage == 0 {
        transfer::consume(&mut character.items, work::GRAIN, 1)?;
        sqlx::query("UPDATE plots SET stage=1, ready_at=NULL WHERE id=$1")
            .bind(plot)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
        "Посеяно. Теперь нужен полив".into()
    } else if stage == 1 {
        transfer::consume(&mut character.items, work::WATER, 1)?;
        sqlx::query(
            "UPDATE plots SET stage=2, ready_at=now() + make_interval(secs => $2) WHERE id=$1",
        )
        .bind(plot)
        .bind(work::grow_for(row.get::<i16, _>("fertility") as u8) as f64)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
        "Полито. Осталось дождаться всходов".into()
    } else if stage == 2 && ready {
        transfer::put(
            &mut character.items,
            Holder::Character,
            Stack::new(
                work::GRAIN,
                work::harvest_amount(),
                Location::Bag { cell: 0 },
            ),
        )?;
        sqlx::query("UPDATE plots SET stage=0, ready_at=NULL WHERE id=$1")
            .bind(plot)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
        format!("Собрано зерна: {}", work::harvest_amount())
    } else {
        return Err("Посев ещё не созрел".into());
    };
    actor.reward_status = message.clone();
    Ok(message)
}

fn retired_blade() -> Result<String, String> {
    Err("Каталоговый клинок больше не куётся. Нарисуйте форму в литье у горна".into())
}
fn design(
    _actor: &mut Actor,
    _character: &mut Character,
    _material: u8,
    _geometry: u8,
    _process: u8,
) -> Result<String, String> {
    retired_blade()
}
fn write_plan(_actor: &mut Actor, _character: &mut Character) -> Result<String, String> {
    retired_blade()
}
fn craft_plan(
    _actor: &mut Actor,
    _character: &mut Character,
    _paper: Option<i64>,
) -> Result<String, String> {
    retired_blade()
}
fn cook(actor: &mut Actor, character: &mut Character) -> Result<String, String> {
    transfer::consume(&mut character.items, work::GRAIN, 2)?;
    transfer::consume(&mut character.items, work::WOOD, 1)?;
    let mut loaf = Stack::new(work::BREAD, 1, Location::Bag { cell: 0 });
    loaf.lock = work::expiry(work::BREAD, now());
    transfer::put(&mut character.items, Holder::Character, loaf)?;
    actor.reward_status = "Испечён хлеб. Он испортится".into();
    Ok(actor.reward_status.clone())
}
fn salt(actor: &mut Actor, character: &mut Character, item: i64) -> Result<String, String> {
    let loaf = character
        .items
        .iter()
        .find(|s| s.id == item)
        .copied()
        .ok_or("Нет такого хлеба")?;
    if loaf.kind != work::BREAD || !work::edible(loaf.kind, loaf.lock, now()) {
        return Err("Солить можно только свежий хлеб".into());
    }
    transfer::consume(&mut character.items, work::SALT, 1)?;
    transfer::split(&mut character.items, Holder::Character, item, 1)?;
    let mut kept = Stack::new(work::SALTED, 1, Location::Bag { cell: 0 });
    kept.lock = work::expiry(work::SALTED, now());
    transfer::put(&mut character.items, Holder::Character, kept)?;
    actor.reward_status = "Хлеб посолен и хранится дольше".into();
    Ok(actor.reward_status.clone())
}
fn eat(actor: &mut Actor, character: &mut Character, item: i64) -> Result<String, String> {
    let food = character
        .items
        .iter()
        .find(|s| s.id == item)
        .copied()
        .ok_or("Еды нет")?;
    if !matches!(food.kind, work::BREAD | work::SALTED) {
        return Err("Это не еда".into());
    }
    if !work::edible(food.kind, food.lock, now()) {
        transfer::split(&mut character.items, Holder::Character, item, 1)?;
        actor.hunger = (actor.hunger + 5.).min(100.);
        actor.reward_status = "Еда испортилась и выброшена".into();
        return Ok(actor.reward_status.clone());
    }
    transfer::split(&mut character.items, Holder::Character, item, 1)?;
    actor.hunger = (actor.hunger - work::meal()).max(0.);
    actor.hp = (actor.hp + 5.).min(actor.max_hp);
    actor.reward_status = "Вы поели".into();
    Ok(actor.reward_status.clone())
}
fn drink(actor: &mut Actor, character: &mut Character, item: i64) -> Result<String, String> {
    let water = character
        .items
        .iter()
        .find(|s| s.id == item && s.kind == work::WATER)
        .copied()
        .ok_or("Нет воды")?;
    let _ = water;
    transfer::consume(&mut character.items, work::WATER, 1)?;
    actor.thirst = (actor.thirst - work::drink()).max(0.);
    actor.reward_status = "Вы попили".into();
    Ok(actor.reward_status.clone())
}
fn make_stall(actor: &mut Actor, character: &mut Character) -> Result<String, String> {
    transfer::consume(&mut character.items, work::WOOD, 4)?;
    transfer::put(
        &mut character.items,
        Holder::Character,
        Stack::new(items::STALL, 1, Location::Bag { cell: 0 }),
    )?;
    actor.reward_status = "Сколочен прилавок".into();
    Ok(actor.reward_status.clone())
}

async fn price_of(
    tx: &mut PgConnection,
    world: &mortown_world::World,
    actor: &Actor,
    container: i64,
    price: u64,
) -> Result<String, String> {
    let row = sqlx::query("SELECT x, z, kind, placed_by FROM world_objects WHERE id=$1 FOR UPDATE")
        .bind(container)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .ok_or("Прилавка нет")?;
    if row.get::<i16, _>("kind") as u8 != items::STALL
        || row.get::<Option<i64>, _>("placed_by") != Some(actor.id)
    {
        return Err("Цену назначает хозяин прилавка".into());
    }
    reach(actor, [row.get("x"), row.get("z")], world)?;
    sqlx::query(
        "INSERT INTO stall_prices(object_id, price) VALUES($1,$2) \
         ON CONFLICT (object_id) DO UPDATE SET price=EXCLUDED.price",
    )
    .bind(container)
    .bind(price as i64)
    .execute(&mut *tx)
    .await
    .map_err(database_error)?;
    Ok(format!("Цена: {}", items::format_coins(price)))
}

async fn buy(
    tx: &mut PgConnection,
    world: &mortown_world::World,
    mut buyer: Actor,
    container: i64,
    expected: u64,
) -> Result<Outcome, String> {
    let peek = sqlx::query("SELECT kind, placed_by FROM world_objects WHERE id=$1")
        .bind(container)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .ok_or("Прилавка нет")?;
    if peek.get::<i16, _>("kind") as u8 != items::STALL {
        return Err("Это не прилавок".into());
    }
    let seller = peek
        .get::<Option<i64>, _>("placed_by")
        .ok_or("У прилавка нет хозяина")?;
    if seller == buyer.id {
        return Err("Свой товар так не покупают".into());
    }
    let ids = if seller < buyer.id {
        [seller, buyer.id]
    } else {
        [buyer.id, seller]
    };
    sqlx::query("SELECT id FROM characters WHERE id = ANY($1) ORDER BY id FOR NO KEY UPDATE")
        .bind(&ids[..])
        .fetch_all(&mut *tx)
        .await
        .map_err(database_error)?;
    let row = sqlx::query("SELECT x, z, kind, placed_by FROM world_objects WHERE id=$1 FOR UPDATE")
        .bind(container)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    if row.get::<Option<i64>, _>("placed_by") != Some(seller) {
        return Err("Хозяин прилавка сменился".into());
    }
    reach(&buyer, [row.get("x"), row.get("z")], world)?;
    let price: i64 = sqlx::query_scalar("SELECT price FROM stall_prices WHERE object_id=$1")
        .bind(container)
        .fetch_optional(&mut *tx)
        .await
        .map_err(database_error)?
        .ok_or("Хозяин ещё не назвал цену")?;
    if price as u64 != expected {
        return Err("Цена изменилась".into());
    }
    let mut buyer_sheet = inventory::load(&mut *tx, &buyer).await?;
    let buyer_before = buyer_sheet.items.clone();
    let mut contents = inventory::contents(&mut *tx, container).await?;
    let offer = contents
        .iter()
        .find(|s| items::coin_value(s.kind).is_none())
        .copied()
        .ok_or("Прилавок пуст")?;
    let before_contents = contents.clone();
    contents.retain(|s| s.id != offer.id);
    let mut seller_items = inventory::carried(&mut *tx, seller).await?;
    let seller_before = seller_items.clone();
    let mut moving = offer;
    moving.location = Location::Bag { cell: 0 };
    transfer::put(&mut buyer_sheet.items, Holder::Character, moving)?;
    transfer::pay(&mut buyer_sheet.items, Holder::Character, expected)?;
    let purse = items::purse_value(&seller_items).saturating_add(expected);
    transfer::set_coins(&mut seller_items, Holder::Character, purse)?;
    transfer::check_carry(&buyer_before, &buyer_sheet.items)?;
    transfer::check_carry(&seller_before, &seller_items)?;
    inventory::sync(
        &mut *tx,
        &[
            Change {
                owner: Some(buyer.id),
                before: &buyer_before,
                after: &buyer_sheet.items,
            },
            Change {
                owner: Some(seller),
                before: &seller_before,
                after: &seller_items,
            },
            Change {
                owner: None,
                before: &before_contents,
                after: &contents,
            },
        ],
    )
    .await?;
    buyer.reward_status = format!("Куплено за {}", items::format_coins(expected));
    let sheet = crate::store::sheet(&mut *tx, buyer.id).await?;
    buyer.equipment = items::EquipmentAppearance::from_items(&sheet.items);
    Ok(Outcome {
        view: None,
        message: buyer.reward_status.clone(),
        update: Some((sheet, buyer)),
        changed: true,
    })
}

async fn sync_actor(
    tx: &mut PgConnection,
    actor: &mut Actor,
    character: &mut Character,
    before: &[Stack],
) -> Result<(), String> {
    transfer::check_carry(before, &character.items)?;
    actor.hunger = actor.hunger.clamp(0., 100.);
    actor.thirst = actor.thirst.clamp(0., 100.);
    character.stats.insert("current_hp".into(), actor.hp);
    character.stats.insert("current_mana".into(), actor.mana);
    character
        .stats
        .insert("current_stamina".into(), actor.stamina);
    character.stats.insert("hunger".into(), actor.hunger);
    character.stats.insert("thirst".into(), actor.thirst);
    actor.progress.write_stats(&mut character.stats);
    inventory::sync(
        &mut *tx,
        &[Change {
            owner: Some(actor.id),
            before,
            after: &character.items,
        }],
    )
    .await?;
    let changed = sqlx::query(
        "UPDATE characters SET x=$1, z=$2, yaw=$3, saved_at=now(), stats=$4 \
         WHERE id=$5 AND coalesce((stats->>'admin_revision')::double precision,0)=$6",
    )
    .bind(actor.position[0])
    .bind(actor.position[2])
    .bind(actor.yaw)
    .bind(serde_json::to_value(&character.stats).unwrap())
    .bind(actor.id)
    .bind(f64::from(actor.progress.admin_revision))
    .execute(&mut *tx)
    .await
    .map_err(database_error)?;
    if changed.rows_affected() != 1 {
        return Err(crate::store::STALE_ADMIN.into());
    }
    Ok(())
}
