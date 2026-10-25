//! Foundry session. The numerical pour runs outside the simulation lock and outside
//! a database transaction, on a blocking worker. One pour calculates at a time.
//! Reserving bronze and writing the blank are short commits.
//! A repeated pour with the same reserved charge does not create a second blank.

use crate::{
    crafting_physics::{
        FeedMode, PourRequest, SolidCell, bevel_cells, c90700, cool_lumped, extrude, feed_account,
        heat, mass_axes, pour,
    },
    inventory::{self, database_error},
    store::{self, ExchangeFailure},
};
use mortown_protocol::{
    Actor, Character,
    casting::{
        BRONZE_MG, Command, Defect, DefectKind, MATERIAL_ID, MATERIAL_VERSION, MAX_POINTS, Phase,
        SOLVER_VERSION, Side, Span, TIME_SCALE, View,
    },
    items::{self, Location, Stack},
    objects::REACH,
    transfer::{self, Holder, InventoryUpdate},
};
use mortown_world::stations::{Furnishing, STATIONS};
use serde::{Deserialize, Serialize};
use sqlx::{Connection, PgConnection, Row};
use std::sync::{Arc, OnceLock};
use tokio::sync::Semaphore;

/// Longest supported one-path mold: 2 m at the 2.5 mm pitch.
const MAX_CELLS: usize = 800;
/// Other pours wait. The wait does not hold the simulation lock or a transaction.
const MAX_CONCURRENT_POURS: usize = 1;

fn pour_slots() -> &'static Arc<Semaphore> {
    static SLOTS: OnceLock<Arc<Semaphore>> = OnceLock::new();
    SLOTS.get_or_init(|| Arc::new(Semaphore::new(MAX_CONCURRENT_POURS)))
}

pub async fn handle(
    conn: &mut PgConnection,
    world: &mortown_world::World,
    actor: Actor,
    command: Command,
) -> Result<View, ExchangeFailure> {
    if actor.hp <= 0. || actor.downed || actor.bound {
        return Err("Сейчас вы не можете работать у горна".into());
    }
    let station = smithy(world);
    let from = [actor.position[0], actor.position[2]];
    if !at_anvil(world, from, station) {
        return Err("Подойдите к горну".into());
    }
    match command {
        Command::Save {
            revision,
            points,
            thickness_mm,
        } => save(conn, &actor, revision, &points, &thickness_mm).await,
        Command::Charge { revision, units } => charge(conn, actor, revision, units).await,
        Command::Heat { revision, target_c } => heat_charge(conn, &actor, revision, target_c).await,
        Command::Pour {
            revision,
            gate_mm2,
            ideal_feed,
            seconds,
        } => {
            pour_cavity(
                conn, &actor, station, revision, gate_mm2, ideal_feed, seconds,
            )
            .await
        }
        Command::Cool { item } => cool_blank(conn, &actor, item).await,
        Command::Machine {
            item,
            edge_tenth_mm,
            side,
            from_mm,
            to_mm,
        } => {
            let pass = Pass {
                edge_tenth_mm,
                side,
                from_mm,
                to_mm,
            };
            machine_blank(conn, &actor, item, pass).await
        }
    }
}

async fn save(
    conn: &mut PgConnection,
    actor: &Actor,
    revision: u64,
    points: &[[i32; 2]],
    thickness: &[u16],
) -> Result<View, ExchangeFailure> {
    let verdict = inspect(points, thickness);
    let pourable = match &verdict {
        Ok(_) => true,
        Err(OutlineProblem::Unsupported(_)) => false,
        Err(OutlineProblem::Rejected(message)) => return Err(message.clone().into()),
    };
    let mut tx = conn.begin().await.map_err(database_error)?;
    let existing = lock_session(&mut tx, actor.id).await?;
    if let Some(row) = &existing
        && row.phase != Phase::Draft
        && row.phase != Phase::Poured
    {
        return Err("Металл уже в тигле. Этот чертёж сохранён не будет".into());
    }
    let seen = existing.as_ref().map(|row| row.revision).unwrap_or(0);
    if seen != revision {
        return Err("Чертёж уже изменился".into());
    }
    let outline = serde_json::json!({ "points": points, "thickness_mm": thickness });
    let blueprint: i64 = sqlx::query_scalar(
        "INSERT INTO blueprints(character_id,solver_version,material_id,material_version,time_scale,outline,pourable) \
         VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id",
    )
    .bind(actor.id)
    .bind(SOLVER_VERSION as i32)
    .bind(MATERIAL_ID)
    .bind(MATERIAL_VERSION as i32)
    .bind(TIME_SCALE as i32)
    .bind(&outline)
    .bind(pourable)
    .fetch_one(&mut *tx)
    .await
    .map_err(database_error)?;
    let next = revision + 1;
    if existing.is_some() {
        sqlx::query(
            "UPDATE craft_sessions SET blueprint_id=$2, revision=$3, phase='draft', charge_mg=0, \
             temperature_c=NULL, gate_mm2=NULL, ideal_feed=false, checkpoint=NULL, result_item=NULL \
             WHERE character_id=$1",
        )
        .bind(actor.id)
        .bind(blueprint)
        .bind(next as i64)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
    } else {
        sqlx::query(
            "INSERT INTO craft_sessions(character_id,blueprint_id,revision,phase) VALUES($1,$2,$3,'draft')",
        )
        .bind(actor.id)
        .bind(blueprint)
        .bind(next as i64)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
    }
    tx.commit().await.map_err(|_| ExchangeFailure::Uncertain)?;
    let message = if pourable {
        "Чертёж сохранён. Металл не израсходован".into()
    } else {
        "Чертёж сохранён, но эта форма ветвится или имеет полость. Такой расчёт заливки пока неверен, металл не берём".into()
    };
    Ok(View {
        revision: next,
        phase: Phase::Draft,
        pourable,
        message,
        charge_mg: 0,
        temperature_c: None,
        spans: verdict.ok().unwrap_or_default(),
        defects: Vec::new(),
        result_item: None,
        center_tenth_mm: None,
        inertia_mg_m2: None,
        update: None,
    })
}

async fn charge(
    conn: &mut PgConnection,
    mut actor: Actor,
    revision: u64,
    units: u16,
) -> Result<View, ExchangeFailure> {
    if !(1..=40).contains(&units) {
        return Err("В тигель берётся от 100 г до 4 кг бронзы".into());
    }
    let mut tx = conn.begin().await.map_err(database_error)?;
    let row = lock_session(&mut tx, actor.id)
        .await?
        .ok_or("Сначала сохраните чертёж")?;
    if row.revision != revision {
        return Err("Чертёж уже изменился".into());
    }
    if row.phase != Phase::Draft {
        return Err("Тигель уже загружен".into());
    }
    if !row.pourable {
        return Err("Эту форму нельзя залить текущим расчётом".into());
    }
    let mut character = inventory::load(&mut tx, &actor).await?;
    let before = character.items.clone();
    transfer::consume(&mut character.items, items::BRONZE, u32::from(units))
        .map_err(str::to_string)?;
    transfer::check_carry(&before, &character.items).map_err(str::to_string)?;
    inventory::sync(
        &mut tx,
        &[inventory::Change {
            owner: Some(actor.id),
            before: &before,
            after: &character.items,
        }],
    )
    .await?;
    let mass = u64::from(units) * BRONZE_MG;
    let next = revision + 1;
    sqlx::query(
        "UPDATE craft_sessions SET revision=$2, phase='charged', charge_mg=$3 WHERE character_id=$1",
    )
    .bind(actor.id)
    .bind(next as i64)
    .bind(mass as i64)
    .execute(&mut *tx)
    .await
    .map_err(database_error)?;
    let sheet = store::sheet(&mut tx, actor.id).await?;
    tx.commit().await.map_err(|_| ExchangeFailure::Uncertain)?;
    actor.equipment = items::EquipmentAppearance::from_items(&sheet.items);
    Ok(view_of(
        &row,
        next,
        Phase::Charged,
        true,
        mass,
        None,
        format!("В тигле {} г бронзы C90700", mass / 1000),
        Vec::new(),
        Some(sheet),
        actor,
    ))
}

async fn heat_charge(
    conn: &mut PgConnection,
    actor: &Actor,
    revision: u64,
    target_c: u16,
) -> Result<View, ExchangeFailure> {
    if !(20..=1200).contains(&target_c) {
        return Err("Такую температуру печь не держит".into());
    }
    let mut tx = conn.begin().await.map_err(database_error)?;
    let row = lock_session(&mut tx, actor.id)
        .await?
        .ok_or("Тигель пуст")?;
    if row.revision != revision {
        return Err("Чертёж уже изменился".into());
    }
    if row.phase != Phase::Charged && row.phase != Phase::Hot {
        return Err("Сначала загрузите бронзу".into());
    }
    let mass_kg = row.charge_mg as f64 / 1e6;
    let (energy, seconds) = heat(&c90700(), mass_kg, 20., f64::from(target_c), 1000.);
    if !energy.is_finite() || !seconds.is_finite() {
        return Err("Нагрев не посчитался".into());
    }
    let next = revision + 1;
    sqlx::query(
        "UPDATE craft_sessions SET revision=$2, phase='hot', temperature_c=$3, physical_s=physical_s+$4 \
         WHERE character_id=$1",
    )
    .bind(actor.id)
    .bind(next as i64)
    .bind(f64::from(target_c))
    .bind(seconds)
    .execute(&mut *tx)
    .await
    .map_err(database_error)?;
    tx.commit().await.map_err(|_| ExchangeFailure::Uncertain)?;
    Ok(view_of(
        &row,
        next,
        Phase::Hot,
        true,
        row.charge_mg,
        Some(target_c as i16),
        format!(
            "Металл нагрет до {target_c} °C за {:.0} с физического времени",
            seconds
        ),
        Vec::new(),
        None,
        actor.clone(),
    ))
}

async fn pour_cavity(
    conn: &mut PgConnection,
    actor: &Actor,
    station: [f32; 2],
    revision: u64,
    gate_mm2: u16,
    ideal_feed: bool,
    seconds: u16,
) -> Result<View, ExchangeFailure> {
    if !(1..=100).contains(&gate_mm2) {
        return Err("Площадь входа от 1 до 100 мм²".into());
    }
    if seconds > 60 {
        return Err("Подачу можно закрыть через 1–60 с".into());
    }
    let prepared = {
        let mut tx = conn.begin().await.map_err(database_error)?;
        let row = lock_session(&mut tx, actor.id)
            .await?
            .ok_or("Нет загруженного тигля")?;
        if row.phase == Phase::Poured {
            return Ok(finished(&row, "Эта заливка уже записана"));
        }
        if row.phase == Phase::Pouring {
            if let Some(done) = row.checkpoint.clone() {
                return finish_from_checkpoint(tx, actor, &row, station, done).await;
            }
        } else if row.phase != Phase::Hot {
            return Err("Сначала нагрейте металл".into());
        }
        if row.revision != revision && row.phase != Phase::Pouring {
            return Err("Чертёж уже изменился".into());
        }
        let temperature = row.temperature_c.unwrap_or(0.);
        if temperature < 999. {
            return Err("Металл ещё не жидкий".into());
        }
        let outline = outline_of(&mut tx, row.blueprint_id).await?;
        let spans =
            inspect(&outline.points, &outline.thickness_mm).map_err(|problem| problem.message())?;
        let (gate_mm2, ideal_feed, seconds) = if row.phase == Phase::Pouring {
            let stored_gate = row.gate_mm2.unwrap_or(0);
            if !(1..=100).contains(&stored_gate) || !(0..=60).contains(&row.pour_seconds) {
                sqlx::query(
                    "UPDATE craft_sessions SET phase='hot' WHERE character_id=$1 AND phase='pouring'",
                )
                .bind(actor.id)
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
                tx.commit().await.map_err(|_| ExchangeFailure::Uncertain)?;
                return Err("Запись начатой заливки повреждена. Металл остался в тигле".into());
            }
            (stored_gate as u16, row.ideal_feed, row.pour_seconds as u16)
        } else {
            sqlx::query(
                "UPDATE craft_sessions SET phase='pouring', revision=$2, gate_mm2=$3, ideal_feed=$4, \
                 pour_seconds=$5 WHERE character_id=$1",
            )
            .bind(actor.id)
            .bind((row.revision + 1) as i64)
            .bind(i32::from(gate_mm2))
            .bind(ideal_feed)
            .bind(i32::from(seconds))
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
            (gate_mm2, ideal_feed, seconds)
        };
        tx.commit().await.map_err(|_| ExchangeFailure::Uncertain)?;
        Prepared {
            charge_mg: row.charge_mg,
            temperature,
            spans,
            gate_mm2,
            ideal_feed,
            seconds,
        }
    };
    let mold = extrude(&slices_from(&prepared.spans));
    if mold.cells.len() > MAX_CELLS {
        let mut tx = conn.begin().await.map_err(database_error)?;
        sqlx::query(
            "UPDATE craft_sessions SET phase='hot' WHERE character_id=$1 AND phase='pouring'",
        )
        .bind(actor.id)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
        tx.commit().await.map_err(|_| ExchangeFailure::Uncertain)?;
        return Err("Сетка чертежа больше 800 клеток. Металл остался в тигле".into());
    }
    let gate_area = f64::from(prepared.gate_mm2) * 1e-6;
    let source_temperature = prepared.temperature;
    let source_mass = prepared.charge_mg as f64 / 1e6;
    let time_limit = if prepared.seconds == 0 {
        400.
    } else {
        f64::from(prepared.seconds)
    };
    let permit = pour_slots()
        .clone()
        .acquire_owned()
        .await
        .map_err(|_| "Расчёт заливки недоступен. Металл остался в тигле")?;
    let solving = mold.clone();
    let outcome = match tokio::task::spawn_blocking(move || {
        let _slot = permit;
        let metal = c90700();
        pour(PourRequest {
            mold: &solving,
            material: &metal,
            gate_area,
            head: 0.15,
            discharge: 0.65,
            mold_temperature: 200.,
            film: 60.,
            source_temperature,
            source_mass,
            dt_max: 0.005,
            time_limit,
            step_budget: 2_000_000,
            sample_at: &[],
        })
    })
    .await
    {
        Ok(outcome) => outcome,
        Err(_) => {
            return Err(
                "Расчёт заливки оборвался. Повторите заливку: металл остался в тигле".into(),
            );
        }
    };
    let player_halt =
        prepared.seconds > 0 && matches!(outcome.stop, crate::crafting_physics::Stop::TimeLimit);
    let technical = matches!(
        outcome.stop,
        crate::crafting_physics::Stop::Numerical | crate::crafting_physics::Stop::StepBudget
    ) || (matches!(outcome.stop, crate::crafting_physics::Stop::TimeLimit)
        && !player_halt);
    if technical {
        let mut tx = conn.begin().await.map_err(database_error)?;
        sqlx::query(
            "UPDATE craft_sessions SET phase='hot' WHERE character_id=$1 AND phase='pouring'",
        )
        .bind(actor.id)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
        tx.commit().await.map_err(|_| ExchangeFailure::Uncertain)?;
        return Err("Расчёт остановлен по техническому пределу. Металл остался в тигле".into());
    }
    let liquid_mg = (outcome.mass_entered * 1e6).round();
    if !(0. ..prepared.charge_mg as f64 + 1.).contains(&liquid_mg) {
        return Err("Баланс массы заливки не сошёлся. Металл оставлен в тигле".into());
    }
    let liquid_mg = liquid_mg as u64;
    if liquid_mg == 0 {
        let mut tx = conn.begin().await.map_err(database_error)?;
        sqlx::query(
            "UPDATE craft_sessions SET phase='hot' WHERE character_id=$1 AND phase='pouring'",
        )
        .bind(actor.id)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
        tx.commit().await.map_err(|_| ExchangeFailure::Uncertain)?;
        return Err("В форму ничего не вошло. Металл остался в тигле".into());
    }
    let metal = c90700();
    let account = feed_account(
        &mold,
        &metal,
        outcome.filled_volume,
        if prepared.ideal_feed {
            FeedMode::IdealFeeder
        } else {
            FeedMode::Unresolved
        },
    );
    let mut feed_mg = (account.feed_mass * 1e6).round().max(0.) as u64;
    let mut feed_status = match account.mode {
        FeedMode::IdealFeeder => "ideal_feeder",
        FeedMode::Unresolved => "feeding_unresolved",
    };
    if liquid_mg + feed_mg > prepared.charge_mg {
        feed_mg = 0;
        feed_status = "feeding_unresolved";
    }
    let (spans, defects, message) =
        describe(&prepared.spans, &outcome, liquid_mg, feed_mg, player_halt);
    let returned_units =
        u32::try_from(prepared.charge_mg.saturating_sub(liquid_mg + feed_mg) / BRONZE_MG)
            .unwrap_or(0);
    let mass_mg = liquid_mg + feed_mg;
    let (center_m, inertia) = mass_axes(&solid_cells(&spans), mass_mg as f64 / 1e6);
    let stored = Stored {
        spans: spans.clone(),
        defects: defects.clone(),
        mass_mg,
        feed_status: feed_status.into(),
        physical_s: outcome.time,
        message: message.clone(),
        returned_units,
        center_m: Some(center_m),
        inertia: Some(inertia),
    };
    let mut tx = conn.begin().await.map_err(database_error)?;
    let row = lock_session(&mut tx, actor.id)
        .await?
        .ok_or("Сессия литья пропала")?;
    if row.phase == Phase::Poured {
        return Ok(finished(&row, "Эта заливка уже записана"));
    }
    if row.phase != Phase::Pouring {
        return Err("Заливка была прервана".into());
    }
    finish_from_checkpoint(tx, actor, &row, station, stored).await
}

async fn finish_from_checkpoint(
    mut tx: sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &Actor,
    row: &Session,
    station: [f32; 2],
    mut stored: Stored,
) -> Result<View, ExchangeFailure> {
    let character = inventory::load(&mut tx, actor).await?;
    let mut held = character.items.clone();
    let blank = admit(&held, items::CASTING, 1, Some(stored.mass_mg), station);
    let place = blank.location;
    if matches!(place, Location::Bag { .. }) {
        held.push(blank);
    }
    let item = inventory::insert(&mut tx, Some(actor.id), &blank).await?;
    if stored.returned_units > 0 {
        let bronze = admit(&held, items::BRONZE, stored.returned_units, None, station);
        stored
            .message
            .push_str(&if matches!(bronze.location, Location::Bag { .. }) {
                format!(" В сумку вернулось {} сл. бронзы", stored.returned_units)
            } else {
                format!(
                    " {} сл. бронзы лежат у горна: в сумку они не влезли",
                    stored.returned_units
                )
            });
        inventory::insert(&mut tx, Some(actor.id), &bronze).await?;
    }
    sqlx::query(
        "INSERT INTO item_bodies(item_id,solver_version,material_id,material_version,mass_mg,feed_status,geometry,defects,center_m,inertia_kg_m2) \
         VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(item)
    .bind(SOLVER_VERSION as i32)
    .bind(MATERIAL_ID)
    .bind(MATERIAL_VERSION as i32)
    .bind(stored.mass_mg as i64)
    .bind(&stored.feed_status)
    .bind(serde_json::to_value(&stored.spans).map_err(|_| "Не удалось записать форму")?)
    .bind(serde_json::to_value(&stored.defects).map_err(|_| "Не удалось записать дефекты")?)
    .bind(stored.center_m)
    .bind(stored.inertia)
    .execute(&mut *tx)
    .await
    .map_err(database_error)?;
    let next = row.revision + 1;
    sqlx::query(
        "UPDATE craft_sessions SET phase='poured', revision=$2, checkpoint=$3, result_item=$4, \
         physical_s=physical_s+$5 WHERE character_id=$1",
    )
    .bind(actor.id)
    .bind(next as i64)
    .bind(serde_json::to_value(&stored).map_err(|_| "Не удалось записать заливку")?)
    .bind(item)
    .bind(stored.physical_s)
    .execute(&mut *tx)
    .await
    .map_err(database_error)?;
    let sheet = store::sheet(&mut tx, actor.id).await?;
    tx.commit().await.map_err(|_| ExchangeFailure::Uncertain)?;
    let mut actor = actor.clone();
    actor.equipment = items::EquipmentAppearance::from_items(&sheet.items);
    let where_text = if matches!(place, Location::Ground { .. }) {
        "Сумка полна, отливка лежит у горна"
    } else {
        "Отливка в сумке"
    };
    let (center_tenth_mm, inertia_mg_m2) = shown_axes(stored.center_m, stored.inertia);
    Ok(View {
        revision: next,
        phase: Phase::Poured,
        pourable: true,
        message: format!("{}. {where_text}", stored.message),
        charge_mg: row.charge_mg,
        temperature_c: metal_temperature(&stored.spans),
        spans: stored.spans,
        defects: stored.defects,
        result_item: Some(item),
        center_tenth_mm,
        inertia_mg_m2,
        update: Some(Box::new(InventoryUpdate {
            character: sheet,
            actor,
        })),
    })
}

async fn cool_blank(
    conn: &mut PgConnection,
    actor: &Actor,
    item: i64,
) -> Result<View, ExchangeFailure> {
    let mut tx = conn.begin().await.map_err(database_error)?;
    let _session = lock_session(&mut tx, actor.id).await?;
    let mut held = lock_blank(&mut tx, actor, item).await?;
    let already_cold = held
        .spans
        .iter()
        .filter(|span| span.fill_milli > 0)
        .all(|span| span.temperature_c <= 300);
    let (cooled, mut seconds) = cool_lumped(
        &solid_cells(&held.spans),
        held.mass_mg as f64 / 1e6,
        60.,
        200.,
        &c90700(),
    );
    apply_cells(&mut held.spans, &cooled);
    if already_cold {
        seconds = 0.;
    }
    let message = if seconds == 0. {
        "Заготовка уже холодная".to_string()
    } else {
        format!(
            "Заготовка остыла до 300 °C за {:.0} с. Это усреднение всей массы, не поклеточный расчёт",
            seconds
        )
    };
    publish(tx, actor, item, &held, seconds, message).await
}

struct Pass {
    edge_tenth_mm: u16,
    side: Side,
    from_mm: Option<i32>,
    to_mm: Option<i32>,
}

async fn machine_blank(
    conn: &mut PgConnection,
    actor: &Actor,
    item: i64,
    pass: Pass,
) -> Result<View, ExchangeFailure> {
    let Pass {
        edge_tenth_mm,
        side,
        from_mm,
        to_mm,
    } = pass;
    if !(1..=40).contains(&edge_tenth_mm) {
        return Err("Кромка задаётся от 0,1 до 4 мм".into());
    }
    let from = from_mm.unwrap_or(-2000);
    let to = to_mm.unwrap_or(2000);
    if from.abs() > 2000 || to.abs() > 2000 || from >= to {
        return Err("Зона обработки задаётся внутри чертежа, начало раньше конца".into());
    }
    let mut tx = conn.begin().await.map_err(database_error)?;
    let _session = lock_session(&mut tx, actor.id).await?;
    let mut held = lock_blank(&mut tx, actor, item).await?;
    let edge = f64::from(edge_tenth_mm) * 1e-4;
    let cut = bevel_cells(
        &solid_cells(&held.spans),
        edge,
        side.sides(),
        (f64::from(from) / 1000., f64::from(to) / 1000.),
        held.mass_mg as f64 / 1e6,
        c90700().solidus,
    )
    .map_err(str::to_string)?;
    let mass_mg = (cut.mass * 1e6).round().clamp(0., held.mass_mg as f64) as u64;
    if mass_mg == 0 || mass_mg > held.mass_mg {
        return Err("Съём не сходится с массой заготовки".into());
    }
    apply_cells(&mut held.spans, &cut.cells);
    held.mass_mg = mass_mg;
    held.center_m = Some(cut.center);
    held.inertia = Some(cut.inertia);
    let where_text = match side {
        Side::Both => "с обеих сторон",
        Side::Left => "с левой стороны",
        Side::Right => "с правой стороны",
    };
    let zone_text = match (from_mm, to_mm) {
        (None, None) => String::new(),
        _ => format!(" на участке {from}…{to} мм"),
    };
    let message = if cut.removed <= 0. {
        "Скос уже снят до этой кромки. Металл второй раз не снимается и не прибавляется".to_string()
    } else {
        format!(
            "Снят скос до {:.1} мм {where_text}{zone_text}, ушло {:.1} г. Стружка не возвращается в тигель, рукоять не прибавляется",
            f64::from(edge_tenth_mm) / 10.,
            cut.removed * 1e3
        )
    };
    publish(tx, actor, item, &held, 0., message).await
}

/// A new stack goes into the bag when the carry limit still allows it.
/// An overweight load keeps what it already holds; the new stack stays at the smithy.
fn admit(
    held: &[Stack],
    kind: u8,
    quantity: u32,
    mass_mg: Option<u64>,
    station: [f32; 2],
) -> Stack {
    let cell = Holder::Character.free(held).unwrap_or(0);
    let mut stack = Stack::new(kind, quantity, Location::Bag { cell });
    stack.mass_mg = mass_mg;
    let mut next = held.to_vec();
    next.push(stack);
    if transfer::check_carry(held, &next).is_err() {
        stack.location = Location::Ground {
            x: station[0],
            z: station[1],
        };
    }
    stack
}

struct Held {
    mass_mg: u64,
    spans: Vec<Span>,
    defects: Vec<Defect>,
    center_m: Option<f64>,
    inertia: Option<f64>,
}

async fn lock_blank(
    tx: &mut PgConnection,
    actor: &Actor,
    item: i64,
) -> Result<Held, ExchangeFailure> {
    let row = sqlx::query(
        "SELECT i.mass_mg, i.character_id, i.place, i.x, i.z, b.geometry, b.defects, \
         b.center_m, b.inertia_kg_m2 \
         FROM items i JOIN item_bodies b ON b.item_id = i.id \
         WHERE i.id=$1 AND i.kind=$2 FOR UPDATE OF i",
    )
    .bind(item)
    .bind(i16::from(items::CASTING))
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?
    .ok_or("Этой отливки нет")?;
    let owner: Option<i64> = row.get("character_id");
    let place: String = row.get("place");
    let near_ground = place == "ground"
        && row
            .get::<Option<f32>, _>("x")
            .zip(row.get::<Option<f32>, _>("z"))
            .is_some_and(|(x, z)| {
                mortown_world::distance([actor.position[0], actor.position[2]], [x, z]) <= REACH
            });
    if owner != Some(actor.id) && !near_ground {
        return Err("Отливка не у вас".into());
    }
    let mass_mg = u64::try_from(row.get::<i64, _>("mass_mg")).unwrap_or(0);
    let spans: Vec<Span> =
        serde_json::from_value(row.get("geometry")).map_err(|_| "Форма отливки повреждена")?;
    let defects: Vec<Defect> = serde_json::from_value(row.get("defects")).unwrap_or_default();
    Ok(Held {
        mass_mg,
        spans,
        defects,
        center_m: row.get("center_m"),
        inertia: row.get("inertia_kg_m2"),
    })
}

fn solid_cells(spans: &[Span]) -> Vec<SolidCell> {
    spans
        .iter()
        .map(|span| SolidCell {
            x0: f64::from(span.x0_mm) / 1000.,
            length: f64::from(span.x1_mm - span.x0_mm) / 1000.,
            width: f64::from(span.width_mm) / 1000.,
            thickness: f64::from(span.thickness_mm) / 1000.,
            fill: f64::from(span.fill_milli) / 1000.,
            temperature: f64::from(span.temperature_c),
            edges: span.edges_tenth_mm.map(|edge| f64::from(edge) / 10_000.),
        })
        .collect()
}

fn apply_cells(spans: &mut [Span], cells: &[SolidCell]) {
    for (span, cell) in spans.iter_mut().zip(cells) {
        span.temperature_c = cell.temperature.round().clamp(-32_768., 32_767.) as i16;
        span.edges_tenth_mm = cell
            .edges
            .map(|edge| (edge * 10_000.).round().clamp(0., 65_535.) as u16);
        span.fill_milli = (cell.fill * 1000.).round().clamp(0., 1000.) as u16;
    }
}

async fn publish(
    mut tx: sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &Actor,
    item: i64,
    held: &Held,
    seconds: f64,
    message: String,
) -> Result<View, ExchangeFailure> {
    sqlx::query("UPDATE items SET mass_mg=$2 WHERE id=$1")
        .bind(item)
        .bind(held.mass_mg as i64)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
    sqlx::query(
        "UPDATE item_bodies SET mass_mg=$2, geometry=$3, physical_s=physical_s+$4, \
         center_m=$5, inertia_kg_m2=$6 WHERE item_id=$1",
    )
    .bind(item)
    .bind(held.mass_mg as i64)
    .bind(serde_json::to_value(&held.spans).map_err(|_| "Не удалось записать форму")?)
    .bind(seconds)
    .bind(held.center_m)
    .bind(held.inertia)
    .execute(&mut *tx)
    .await
    .map_err(database_error)?;
    let revision = sqlx::query_scalar::<_, i64>(
        "UPDATE craft_sessions SET revision=revision+1 WHERE character_id=$1 RETURNING revision",
    )
    .bind(actor.id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?
    .unwrap_or(0);
    let sheet = store::sheet(&mut tx, actor.id).await?;
    tx.commit().await.map_err(|_| ExchangeFailure::Uncertain)?;
    let mut actor = actor.clone();
    actor.equipment = items::EquipmentAppearance::from_items(&sheet.items);
    let temperature = metal_temperature(&held.spans);
    Ok(View {
        revision: u64::try_from(revision).unwrap_or(0),
        phase: Phase::Poured,
        pourable: true,
        message,
        charge_mg: 0,
        temperature_c: temperature,
        spans: held.spans.clone(),
        defects: held.defects.clone(),
        result_item: Some(item),
        center_tenth_mm: shown_axes(held.center_m, held.inertia).0,
        inertia_mg_m2: shown_axes(held.center_m, held.inertia).1,
        update: Some(Box::new(InventoryUpdate {
            character: sheet,
            actor,
        })),
    })
}

fn describe(
    plan: &[Span],
    outcome: &crate::crafting_physics::PourOutcome,
    liquid_mg: u64,
    feed_mg: u64,
    halted: bool,
) -> (Vec<Span>, Vec<Defect>, String) {
    let mut spans = Vec::new();
    let mut unfilled = 0.;
    let mut unfilled_from = None;
    let mut unfilled_to = 0i32;
    for (plan, cell) in plan.iter().zip(&outcome.cells) {
        let fill = if cell.mass <= 0. {
            0.
        } else {
            cell.fill.clamp(0., 1.)
        };
        if fill < 0.999 {
            unfilled += (1. - fill)
                * (plan.x1_mm - plan.x0_mm) as f64
                * f64::from(plan.width_mm)
                * f64::from(plan.thickness_mm);
            unfilled_from.get_or_insert(plan.x0_mm);
            unfilled_to = plan.x1_mm;
        }
        let temperature = if cell.mass > 0. && cell.temperature.is_finite() {
            cell.temperature.round() as i16
        } else {
            0
        };
        spans.push(Span {
            fill_milli: (fill * 1000.).round().clamp(0., 1000.) as u16,
            temperature_c: temperature,
            ..*plan
        });
    }
    let mut defects = Vec::new();
    if let crate::crafting_physics::Stop::Plug { cell } = outcome.stop
        && let Some(span) = spans.get(cell)
    {
        defects.push(Defect {
            kind: DefectKind::SolidifiedFeed,
            x0_mm: span.x0_mm,
            x1_mm: span.x1_mm,
            time_ms: (outcome.time * 1000.).round().max(0.) as u64,
            volume_mm3: 0,
        });
    }
    if unfilled > 1. {
        defects.push(Defect {
            kind: if halted {
                DefectKind::Halted
            } else if matches!(outcome.stop, crate::crafting_physics::Stop::SourceSpent) {
                DefectKind::ShortCharge
            } else {
                DefectKind::Misrun
            },
            x0_mm: unfilled_from.unwrap_or(0),
            x1_mm: unfilled_to,
            time_ms: (outcome.time * 1000.).round().max(0.) as u64,
            volume_mm3: unfilled.round().max(0.) as u64,
        });
    }
    let grams = (liquid_mg + feed_mg) / 1000;
    let message = if defects.iter().any(|d| d.kind == DefectKind::SolidifiedFeed) {
        format!("Поток встал на {grams} г. Недолитая часть на чертеже пустая")
    } else if defects.iter().any(|d| d.kind == DefectKind::Halted) {
        format!("Подача закрыта на {grams} г. Недолитая часть пустая")
    } else if defects.iter().any(|d| d.kind == DefectKind::ShortCharge) {
        format!("Бронза кончилась на {grams} г")
    } else if feed_mg > 0 {
        format!("Полость заполнена, {grams} г с идеальным питателем")
    } else {
        format!("Полость заполнена жидким металлом, {grams} г. Усадка не посчитана")
    };
    (spans, defects, message)
}

fn slices_from(spans: &[Span]) -> Vec<[f64; 5]> {
    spans
        .iter()
        .map(|span| {
            let width = f64::from(span.width_mm) / 1000.;
            let thick = f64::from(span.thickness_mm) / 1000.;
            [
                f64::from(span.x0_mm) / 1000.,
                f64::from(span.x1_mm) / 1000.,
                width,
                width,
                thick,
            ]
        })
        .collect()
}

#[derive(Debug)]
enum OutlineProblem {
    Rejected(String),
    Unsupported(String),
}
impl OutlineProblem {
    fn message(&self) -> String {
        match self {
            Self::Rejected(text) | Self::Unsupported(text) => text.clone(),
        }
    }
}

/// Planned cells with empty fill. Unsupported topology is not a mold.
fn inspect(points: &[[i32; 2]], thickness: &[u16]) -> Result<Vec<Span>, OutlineProblem> {
    if !(4..=MAX_POINTS).contains(&points.len()) {
        return Err(OutlineProblem::Rejected("Контур — от 4 до 48 точек".into()));
    }
    if !(2..=8).contains(&thickness.len()) || thickness.iter().any(|t| !(1..=40).contains(t)) {
        return Err(OutlineProblem::Rejected(
            "Толщина задаётся в 2–8 местах, от 1 до 40 мм".into(),
        ));
    }
    if points
        .iter()
        .any(|p| p[0].abs() > 2000 || p[1].abs() > 2000)
    {
        return Err(OutlineProblem::Rejected("Чертёж выходит за 2 м".into()));
    }
    if crosses_itself(points) {
        return Err(OutlineProblem::Rejected(
            "Контур пересекает сам себя".into(),
        ));
    }
    let x_min = points.iter().map(|p| p[0]).min().unwrap_or(0);
    let x_max = points.iter().map(|p| p[0]).max().unwrap_or(0);
    let y_min = points.iter().map(|p| p[1]).min().unwrap_or(0);
    let y_max = points.iter().map(|p| p[1]).max().unwrap_or(0);
    if x_max - x_min < 20 || x_max - x_min < y_max - y_min {
        return Err(OutlineProblem::Rejected(
            "Положите длину формы вдоль чертежа, не короче 20 мм".into(),
        ));
    }
    let mut spans = Vec::new();
    let mut x = (f64::from(x_min) / 2.5).floor() * 2.5;
    let end = f64::from(x_max);
    let mut gap = false;
    let mut branched = false;
    while x + 2.5 <= end + 1e-6 {
        let mid = x + 1.25;
        let hits = crossings(points, mid);
        if hits.len() > 2 {
            branched = true;
        } else if hits.len() == 2 {
            if gap {
                branched = true;
            }
            let width = hits[1] - hits[0];
            if !(1. ..200.).contains(&width) {
                return Err(OutlineProblem::Rejected(
                    "Ширина сечения от 1 до 200 мм".into(),
                ));
            }
            let along = (mid - f64::from(x_min)) / f64::from(x_max - x_min);
            let thick = thickness_mm(thickness, along);
            spans.push(Span {
                x0_mm: x.round() as i32,
                x1_mm: (x + 2.5).round() as i32,
                width_mm: width.round() as u16,
                thickness_mm: thick.round() as u16,
                fill_milli: 0,
                temperature_c: 0,
                edges_tenth_mm: [0, 0],
            });
        } else if !spans.is_empty() {
            gap = true;
        }
        x += 2.5;
    }
    if branched {
        return Err(OutlineProblem::Unsupported(
            "У формы несколько просветов на одном сечении".into(),
        ));
    }
    if spans.len() < 4 {
        return Err(OutlineProblem::Rejected("Контур не даёт полости".into()));
    }
    Ok(spans)
}

fn thickness_mm(samples: &[u16], along: f64) -> f64 {
    let scaled = along.clamp(0., 1.) * (samples.len() - 1) as f64;
    let index = scaled.floor() as usize;
    let next = (index + 1).min(samples.len() - 1);
    let mix = scaled - index as f64;
    f64::from(samples[index]) * (1. - mix) + f64::from(samples[next]) * mix
}

fn crossings(points: &[[i32; 2]], x: f64) -> Vec<f64> {
    let mut ys = Vec::new();
    for index in 0..points.len() {
        let mut x0 = f64::from(points[index][0]);
        let mut y0 = f64::from(points[index][1]);
        let next = points[(index + 1) % points.len()];
        let mut x1 = f64::from(next[0]);
        let mut y1 = f64::from(next[1]);
        if (x0 - x1).abs() < 1e-9 {
            continue;
        }
        if x0 > x1 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut y0, &mut y1);
        }
        if x < x0 || x >= x1 {
            continue;
        }
        let t = (x - x0) / (x1 - x0);
        ys.push(y0 + t * (y1 - y0));
    }
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ys.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    ys
}

fn crosses_itself(points: &[[i32; 2]]) -> bool {
    let n = points.len();
    for i in 0..n {
        for j in (i + 1)..n {
            if i == j || (i + 1) % n == j || (j + 1) % n == i {
                continue;
            }
            if proper_cross(
                points[i],
                points[(i + 1) % n],
                points[j],
                points[(j + 1) % n],
            ) {
                return true;
            }
        }
    }
    false
}

fn proper_cross(a: [i32; 2], b: [i32; 2], c: [i32; 2], d: [i32; 2]) -> bool {
    fn side(p: [i32; 2], q: [i32; 2], r: [i32; 2]) -> i32 {
        let value = (q[0] as i64 - p[0] as i64) * (r[1] as i64 - p[1] as i64)
            - (q[1] as i64 - p[1] as i64) * (r[0] as i64 - p[0] as i64);
        value.signum() as i32
    }
    side(a, b, c) * side(a, b, d) < 0 && side(c, d, a) * side(c, d, b) < 0
}

struct Session {
    blueprint_id: Option<i64>,
    revision: u64,
    phase: Phase,
    charge_mg: u64,
    temperature_c: Option<f64>,
    pourable: bool,
    gate_mm2: Option<i32>,
    ideal_feed: bool,
    pour_seconds: i32,
    checkpoint: Option<Stored>,
    result_item: Option<i64>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Stored {
    spans: Vec<Span>,
    defects: Vec<Defect>,
    mass_mg: u64,
    feed_status: String,
    physical_s: f64,
    message: String,
    /// Whole 100 g ingots that did not enter the mold.
    #[serde(default)]
    returned_units: u32,
    /// Metres along the drawing. Empty on a checkpoint written before this was stored.
    #[serde(default)]
    center_m: Option<f64>,
    /// kg·m² about the centre of mass.
    #[serde(default)]
    inertia: Option<f64>,
}

struct Prepared {
    charge_mg: u64,
    temperature: f64,
    spans: Vec<Span>,
    gate_mm2: u16,
    ideal_feed: bool,
    seconds: u16,
}

#[derive(Deserialize)]
struct Outline {
    points: Vec<[i32; 2]>,
    thickness_mm: Vec<u16>,
}

async fn lock_session(tx: &mut PgConnection, character: i64) -> Result<Option<Session>, String> {
    let row = sqlx::query(
        "SELECT s.blueprint_id, s.revision, s.phase, s.charge_mg, s.temperature_c, s.checkpoint, \
         s.result_item, s.gate_mm2, s.ideal_feed, s.pour_seconds, COALESCE(b.pourable, false) AS pourable \
         FROM craft_sessions s LEFT JOIN blueprints b ON b.id = s.blueprint_id \
         WHERE s.character_id=$1 FOR UPDATE OF s",
    )
    .bind(character)
    .fetch_optional(&mut *tx)
    .await
    .map_err(database_error)?;
    let Some(row) = row else {
        return Ok(None);
    };
    let phase = match row.get::<String, _>("phase").as_str() {
        "draft" => Phase::Draft,
        "charged" => Phase::Charged,
        "hot" => Phase::Hot,
        "pouring" => Phase::Pouring,
        "poured" => Phase::Poured,
        _ => return Err("Неизвестная стадия литья".into()),
    };
    let checkpoint = row
        .get::<Option<serde_json::Value>, _>("checkpoint")
        .map(serde_json::from_value)
        .transpose()
        .map_err(|_| "Повреждена запись заливки".to_string())?;
    Ok(Some(Session {
        blueprint_id: row.get("blueprint_id"),
        revision: u64::try_from(row.get::<i64, _>("revision")).unwrap_or(0),
        phase,
        charge_mg: u64::try_from(row.get::<i64, _>("charge_mg")).unwrap_or(0),
        temperature_c: row.get("temperature_c"),
        pourable: row.get("pourable"),
        gate_mm2: row.get("gate_mm2"),
        ideal_feed: row.get("ideal_feed"),
        pour_seconds: row.get("pour_seconds"),
        checkpoint,
        result_item: row.get("result_item"),
    }))
}

async fn outline_of(tx: &mut PgConnection, blueprint: Option<i64>) -> Result<Outline, String> {
    let id = blueprint.ok_or("Нет чертежа")?;
    let value: serde_json::Value = sqlx::query_scalar("SELECT outline FROM blueprints WHERE id=$1")
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;
    serde_json::from_value(value).map_err(|_| "Чертёж повреждён".into())
}

fn finished(row: &Session, message: &str) -> View {
    let stored = row.checkpoint.clone();
    let (center_tenth_mm, inertia_mg_m2) = stored
        .as_ref()
        .map(|done| shown_axes(done.center_m, done.inertia))
        .unwrap_or((None, None));
    View {
        revision: row.revision,
        phase: Phase::Poured,
        pourable: true,
        message: message.into(),
        charge_mg: row.charge_mg,
        temperature_c: stored
            .as_ref()
            .and_then(|done| metal_temperature(&done.spans)),
        spans: stored.as_ref().map(|s| s.spans.clone()).unwrap_or_default(),
        defects: stored.map(|s| s.defects).unwrap_or_default(),
        result_item: row.result_item,
        center_tenth_mm,
        inertia_mg_m2,
        update: None,
    }
}

fn metal_temperature(spans: &[Span]) -> Option<i16> {
    spans
        .iter()
        .filter(|span| span.fill_milli > 0)
        .map(|span| span.temperature_c)
        .min()
}

fn shown_axes(center_m: Option<f64>, inertia: Option<f64>) -> (Option<i32>, Option<u64>) {
    (
        center_m
            .filter(|value| value.is_finite())
            .map(|value| (value * 10_000.).round() as i32),
        inertia
            .filter(|value| value.is_finite() && *value >= 0.)
            .map(|value| (value * 1_000_000.).round() as u64),
    )
}

#[allow(clippy::too_many_arguments)]
fn view_of(
    row: &Session,
    revision: u64,
    phase: Phase,
    pourable: bool,
    charge_mg: u64,
    temperature_c: Option<i16>,
    message: String,
    spans: Vec<Span>,
    sheet: Option<Character>,
    actor: Actor,
) -> View {
    View {
        revision,
        phase,
        pourable,
        message,
        charge_mg,
        temperature_c,
        spans,
        defects: Vec::new(),
        result_item: row.result_item,
        center_tenth_mm: None,
        inertia_mg_m2: None,
        update: sheet.map(|character| Box::new(InventoryUpdate { character, actor })),
    }
}

fn at_anvil(world: &mortown_world::World, from: [f32; 2], anvil: [f32; 2]) -> bool {
    if mortown_world::distance(from, anvil) > REACH || !world.walkable(from[0], from[1]) {
        return false;
    }
    // The anvil top is solid, so the path stops on the last walkable point beside it.
    (0..=12).any(|step| {
        let t = step as f32 / 12.;
        let point = [
            anvil[0] + (from[0] - anvil[0]) * t,
            anvil[1] + (from[1] - anvil[1]) * t,
        ];
        mortown_world::distance(point, anvil) <= 1.8
            && world.walkable(point[0], point[1])
            && world.segment_clear(from, point)
    })
}

fn smithy(world: &mortown_world::World) -> [f32; 2] {
    let station = STATIONS
        .iter()
        .copied()
        .find(|s| s.furnishing == Furnishing::Smithy)
        .expect("the capital has a smithy");
    let place = station.work_surface(world);
    [place[0], place[2]]
}

#[cfg(test)]
mod tests {
    use super::{OutlineProblem, inspect};
    use mortown_protocol::casting::example_sword;

    #[test]
    fn example_sword_is_one_cavity() {
        let (points, thickness) = example_sword();
        let spans = inspect(&points, &thickness).expect("the example is one cavity");
        assert!(spans.len() >= 4);
        assert!(spans.len() <= super::MAX_CELLS);
        assert!(spans.iter().all(|span| (1..=200).contains(&span.width_mm)));
        assert_eq!(spans[0].x0_mm, -120);
        let end = spans.last().unwrap().x1_mm;
        assert!((600..=650).contains(&end), "{end}");
        assert!(spans.iter().any(|span| span.width_mm == 20));
    }

    #[test]
    fn a_crossed_outline_is_rejected() {
        let points = [[0, 0], [200, 40], [0, 40], [200, 0]];
        let problem = inspect(&points, &[6, 6]).expect_err("crossed");
        assert!(problem.message().contains("пересекает"));
    }

    #[test]
    fn only_one_pour_calculates_at_a_time() {
        let held = super::pour_slots().try_acquire().expect("the slot is free");
        assert!(super::pour_slots().try_acquire().is_err());
        drop(held);
        assert!(super::pour_slots().try_acquire().is_ok());
    }

    #[test]
    fn an_overweight_bag_leaves_the_blank_at_the_smithy() {
        use mortown_protocol::items::{self, Location, Stack};
        let station = [1., 2.];
        let heavy = [Stack::new(items::IRON, 200, Location::Bag { cell: 0 })];
        let blank = super::admit(&heavy, items::CASTING, 1, Some(1_400_000), station);
        assert!(matches!(blank.location, Location::Ground { x: 1., z: 2. }));
        let room = [Stack::new(items::IRON, 158, Location::Bag { cell: 0 })];
        let blank = super::admit(&room, items::CASTING, 1, Some(1_400_000), station);
        assert!(matches!(blank.location, Location::Bag { cell: 1 }));
        let held = [room[0], blank];
        let bronze = super::admit(&held, items::BRONZE, 18, None, station);
        assert!(matches!(bronze.location, Location::Ground { .. }));
    }

    #[test]
    fn a_thicker_tip_does_not_thicken_the_base() {
        let (points, mut thickness) = example_sword();
        let base = inspect(&points, &thickness).expect("example");
        thickness[2] = 20;
        let tip = inspect(&points, &thickness).expect("thicker tip");
        assert_eq!(
            base.first().unwrap().thickness_mm,
            tip.first().unwrap().thickness_mm
        );
        assert!(tip.last().unwrap().thickness_mm > base.last().unwrap().thickness_mm);
        let (other, other_thickness) = (vec![[0, 0], [200, 0], [200, 30], [0, 30]], vec![4, 4, 4]);
        let short = inspect(&other, &other_thickness).expect("another outline");
        assert_ne!(short.len(), base.len());
        assert!(short.iter().all(|span| span.thickness_mm == 4));
    }

    #[test]
    fn branched_outline_is_saved_but_not_pourable() {
        let points = [
            [0, 0],
            [80, 0],
            [80, 15],
            [20, 15],
            [20, 45],
            [80, 45],
            [80, 60],
            [0, 60],
        ];
        let problem = inspect(&points, &[6, 6, 6]).expect_err("two intervals");
        assert!(matches!(problem, OutlineProblem::Unsupported(_)));
    }
}
