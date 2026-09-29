use super::*;
use mortown_protocol::admin::{Entry, Target, View};
pub async fn view(
    server: &crate::Server,
    conn: &mut PgConnection,
    owner: i64,
    search: &str,
) -> Result<Reply, String> {
    let staff = role(conn, owner).await?;
    if staff < 2 {
        return Err("Админка недоступна этой роли".into());
    }
    let rows=sqlx::query("SELECT c.id,c.nickname,a.role,c.stats FROM characters c JOIN accounts a ON a.id=c.account_id WHERE ($1='' OR c.id::text=$1 OR c.nickname ILIKE '%' || $1 || '%') ORDER BY c.id LIMIT 200").bind(search).fetch_all(&mut *conn).await.map_err(db)?;
    let history=sqlx::query("SELECT actor_id,action,target,detail,to_char(created_at AT TIME ZONE 'UTC','DD.MM HH24:MI') AS time FROM admin_audit WHERE ($1>=4 OR actor_id=$2) ORDER BY id DESC LIMIT 25").bind(i32::from(staff)).bind(owner).fetch_all(&mut *conn).await.map_err(db)?;
    let sim = server.simulation.lock().await;
    let mut entries = vec![];
    for r in rows {
        let id = r.get("id");
        let a = sim.actor(id);
        let stats: serde_json::Value = r.get("stats");
        entries.push(Entry {
            target: Target::Player(id),
            name: r.get("nickname"),
            role: r.get("role"),
            level: stats["level"].as_f64().unwrap_or(1.) as u32,
            position: a.as_ref().map(|a| a.position),
            hp: a.map_or(0., |a| a.hp),
        });
    }
    for n in &sim.residents {
        entries.push(Entry {
            target: Target::Npc(n.id),
            name: mortown_world::npcs::definition(n.template)
                .map_or("NPC", |n| n.name)
                .into(),
            role: 0,
            level: 1,
            position: Some(n.position),
            hp: n.hp,
        });
    }
    for m in sim.monsters() {
        entries.push(Entry {
            target: Target::Mob(m.id),
            name: m.name,
            role: 0,
            level: m.level,
            position: Some(m.position),
            hp: m.hp,
        });
    }
    let audit = history
        .iter()
        .map(|r| {
            format!(
                "{} · #{} {} → {} {}",
                r.get::<String, _>("time"),
                r.get::<i64, _>("actor_id"),
                r.get::<String, _>("action"),
                r.get::<String, _>("target"),
                r.get::<String, _>("detail")
            )
        })
        .collect();
    Ok(Reply::Dialog {
        dialog: Dialog {
            kind: DialogKind::Admin,
            title: format!(
                "Управление игрой · {}",
                mortown_protocol::admin::role_name(staff)
            ),
            text: String::new(),
            entries: vec![],
            token: None,
            admin: Some(Box::new(View {
                role: staff,
                entries,
                audit,
                online: sim.snapshot().len(),
                powers: sim.actor(owner).map(|a| a.powers).unwrap_or_default(),
            })),
        },
    })
}
async fn mode(
    server: &crate::Server,
    conn: &mut PgConnection,
    owner: i64,
    name: &str,
) -> Result<Reply, String> {
    let mut sim = server.simulation.lock().await;
    let mut actor = sim.actor(owner).ok_or("Вы не в мире".to_string())?;
    let (label, enabled) = match name {
        "//immortal" => {
            actor.powers.immortal ^= true;
            ("Бессмертие", actor.powers.immortal)
        }
        "//invisible" => {
            actor.powers.invisible ^= true;
            if actor.powers.invisible {
                let tick = sim.tick();
                sim.combat.forget_player(owner, tick);
            }
            ("Невидимость", actor.powers.invisible)
        }
        _ => {
            actor.powers.click_teleport ^= true;
            ("Телепорт по клику", actor.powers.click_teleport)
        }
    };
    sim.replace_actor(actor);
    drop(sim);
    audit(
        conn,
        owner,
        name,
        &owner.to_string(),
        if enabled { "вкл" } else { "выкл" },
    )
    .await?;
    server.admin_events.send_replace(());
    let mut reply = view(server, conn, owner, "").await?;
    if let Reply::Dialog { dialog } = &mut reply {
        dialog.text = format!(
            "{label}: {}",
            if enabled {
                "включено"
            } else {
                "выключено"
            }
        );
    }
    Ok(reply)
}
async fn permitted(
    conn: &mut PgConnection,
    owner: i64,
    t: Target,
    staff: i16,
) -> Result<(), String> {
    if let Target::Player(id) = t {
        let (_, _, _, r) = target(conn, &id.to_string()).await?;
        if id != owner && r >= staff {
            return Err("Цель имеет равные или старшие права".into());
        }
    }
    Ok(())
}
fn location(sim: &crate::simulation::Simulation, t: Target) -> Result<[f32; 3], String> {
    match t {
        Target::Player(id) => sim.actor(id).map(|a| a.position),
        Target::Npc(id) => sim
            .residents
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.position),
        Target::Mob(id) => sim
            .monsters()
            .iter()
            .find(|m| m.id == id)
            .map(|m| m.position),
    }
    .ok_or("Цель отсутствует в мире".into())
}
pub async fn execute(
    server: &crate::Server,
    conn: &mut PgConnection,
    owner: i64,
    state: &mut crate::chat::Session,
    call: &Invocation<'_>,
) -> Result<Reply, String> {
    let name = call.spec.name;
    let staff = role(conn, owner).await?;
    if staff < call.spec.role {
        return Err("Недостаточно прав".into());
    }
    if name == "//clearbag" {
        let id = id(call.args[0])?;
        permitted(conn, owner, Target::Player(id), staff).await?;
        return Ok(confirmation(
            state,
            "Очистка инвентаря",
            format!("Удалить все вещи и валюту персонажа #{id}, включая надетое?"),
            PendingAction::ClearBag(id),
        ));
    }
    if name == "//admin" {
        return view(
            server,
            conn,
            owner,
            call.args.first().copied().unwrap_or(""),
        )
        .await;
    }
    if matches!(name, "//immortal" | "//invisible" | "//clicktp") {
        return mode(server, conn, owner, name).await;
    }
    if name == "//resetworld" {
        return Ok(confirmation(state,"Восстановление мира","Убрать созданных мобов и NPC и восстановить исходных жителей и противников? Персонажи и их вещи сохраняются.".into(),PendingAction::ResetWorld));
    }
    if name == "//role" {
        let player = id(call.args[0])?;
        let role_id = i16::try_from(number(call.args[1])?).map_err(|_| "Роль: 1–5")?;
        if !(1..=5).contains(&role_id) {
            return Err("Роль: 1–5".into());
        }
        let mut tx = conn.begin().await.map_err(db)?;
        require(&mut tx, owner, 5).await?;
        let (_, nick, account, current) = target(&mut tx, &player.to_string()).await?;
        if current == 5 {
            return Err("Роль существующего Creator через игру не изменяется".into());
        }
        let changed = sqlx::query("UPDATE accounts SET role=$2 WHERE id=$1 AND role<5")
            .bind(account)
            .bind(role_id)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        if changed.rows_affected() != 1 {
            return Err("Роль цели уже изменилась; обновите список".into());
        }
        audit(
            &mut tx,
            owner,
            name,
            &player.to_string(),
            &role_id.to_string(),
        )
        .await?;
        tx.commit().await.map_err(db)?;
        server.admin_events.send_replace(());
        return Ok(notice(format!(
            "{nick}: {}",
            mortown_protocol::admin::role_name(role_id)
        )));
    }
    if name == "//announce" {
        let message = call.args.join(" ");
        mortown_protocol::chat::validate(&message)?;
        audit(conn, owner, name, "все", &message).await?;
        let _ = server.announcements.send(message);
        return Ok(notice("Объявление отправлено"));
    }
    if name == "//spawn" {
        let template = number(call.args[0])?;
        let mut sim = server.simulation.lock().await;
        let a = sim.actor(owner).ok_or("Вы не в мире")?;
        if a.hp <= 0. {
            return Err("Сначала возродитесь".into());
        }
        if !sim.terrain().walkable(a.position[0], a.position[2]) {
            return Err("Точка спавна недоступна".into());
        }
        if (1..=3).contains(&template) {
            if sim.combat.mobs.len() >= 128 {
                return Err("Предел: 128 мобов".into());
            }
            audit(
                conn,
                owner,
                name,
                &template.to_string(),
                &format!("{:?}", a.position),
            )
            .await?;
            let kind = match template {
                1 => mortown_protocol::monsters::Kind::Bandit,
                2 => mortown_protocol::monsters::Kind::Marauder,
                _ => mortown_protocol::monsters::Kind::Apostate,
            };
            let id = sim.combat.spawn_admin(kind, a.position)?;
            return Ok(notice(format!("Создан моб #{id} на вашей позиции")));
        }
        let template = u8::try_from(template.checked_sub(100).ok_or("Мобы: 1–3, NPC: 101–107")?)
            .map_err(|_| "Мобы: 1–3, NPC: 101–107")?;
        let n = mortown_world::npcs::definition(template).ok_or("NPC: 101–107")?;
        let id =
            u8::try_from(sim.next_npc).map_err(|_| "Исчерпаны ID NPC до перезапуска сервера")?;
        audit(
            conn,
            owner,
            name,
            &(u16::from(template) + 100).to_string(),
            &format!("{:?}", a.position),
        )
        .await?;
        sim.next_npc += 1;
        sim.residents.push(mortown_protocol::admin::NpcState {
            id,
            template,
            position: a.position,
            yaw: a.yaw,
            hp: 100.,
        });
        return Ok(notice(format!(
            "Создан NPC {} #{id} на вашей позиции",
            n.name
        )));
    }
    let selected = state
        .selected
        .ok_or("Сначала выделите цель в мире или в админке")?;
    permitted(conn, owner, selected, staff).await?;
    if name == "//del" {
        return Ok(confirmation(
            state,
            "Удаление цели из мира",
            format!(
                "Убрать {selected:?}? NPC/моб исчезнет до восстановления мира или перезапуска сервера. Игрок будет отключён; его персонаж останется в базе."
            ),
            PendingAction::Remove(selected),
        ));
    }
    let mut sim = server.simulation.lock().await;
    let p = location(&sim, selected)?;
    if matches!(name, "//goto" | "//bring") {
        let (player, p) = if name == "//goto" {
            (owner, p)
        } else {
            let Target::Player(id) = selected else {
                return Err("Призвать можно игрока".into());
            };
            (id, sim.actor(owner).ok_or("Вы не в мире")?.position)
        };
        audit(conn, owner, name, &format!("{selected:?}"), "").await?;
        sim.exchanges
            .cancel(player, "Обмен отменён: перемещение администратором");
        sim.teleport(player, p)?;
        let a = sim.actor(player).unwrap();
        crate::store::save(conn, &a).await?;
        return Ok(notice("Перемещение выполнено"));
    }
    audit(conn, owner, name, &format!("{selected:?}"), "").await?;
    let kill = name == "//kill";
    match selected {
        Target::Npc(id) => {
            let n = sim
                .residents
                .iter_mut()
                .find(|n| n.id == id)
                .ok_or("NPC не найден")?;
            n.hp = if kill { 0. } else { 100. };
        }
        Target::Mob(id) => {
            let tick = sim.tick();
            if kill {
                sim.combat.kill_admin(id, tick)?;
            } else {
                sim.combat.revive_admin(id)?;
            }
        }
        Target::Player(id) => {
            let mut a = sim.actor(id).ok_or("Игрок не в мире")?;
            let tick = sim.tick();
            sim.combat.player_admin(&mut a, kill, tick);
            sim.exchanges
                .cancel(id, "Обмен отменён: действие администратора");
            sim.replace_actor(a.clone());
            crate::store::save(conn, &a).await?;
        }
    }
    server.admin_events.send_replace(());
    Ok(notice(if kill {
        "Цель убита без наград и добычи"
    } else {
        "Цель восстановлена"
    }))
}
pub async fn remove(
    server: &crate::Server,
    conn: &mut PgConnection,
    owner: i64,
    target_id: Target,
) -> Result<Reply, String> {
    let staff = role(conn, owner).await?;
    if staff < 3 {
        return Err("Недостаточно прав".into());
    }
    permitted(conn, owner, target_id, staff).await?;
    if let Target::Player(id) = target_id {
        let mut tx = conn.begin().await.map_err(db)?;
        require(&mut tx, owner, 3).await?;
        let (_, _, account, _) = target(&mut tx, &id.to_string()).await?;
        sqlx::query("UPDATE accounts SET session_epoch=session_epoch+1 WHERE id=$1")
            .bind(account)
            .execute(&mut *tx)
            .await
            .map_err(db)?;
        audit(
            &mut tx,
            owner,
            "//del",
            &id.to_string(),
            "Отключение из мира",
        )
        .await?;
        tx.commit().await.map_err(db)?;
        server.admin_events.send_replace(());
    } else {
        let mut sim = server.simulation.lock().await;
        location(&sim, target_id)?;
        audit(conn, owner, "//del", &format!("{target_id:?}"), "").await?;
        match target_id {
            Target::Npc(id) => sim.residents.retain(|n| n.id != id),
            Target::Mob(id) => sim.combat.remove_admin(id)?,
            _ => unreachable!(),
        }
    }
    Ok(notice("Цель убрана из мира"))
}
pub async fn reset(
    server: &crate::Server,
    conn: &mut PgConnection,
    owner: i64,
) -> Result<Reply, String> {
    if role(conn, owner).await? < 4 {
        return Err("Недостаточно прав".into());
    }
    let mut sim = server.simulation.lock().await;
    let world = sim.terrain();
    audit(conn, owner, "//resetworld", "мир", "").await?;
    sim.combat.reset_admin(&world);
    sim.residents = mortown_world::npcs::RESIDENTS
        .iter()
        .map(|n| mortown_protocol::admin::NpcState {
            id: n.id,
            template: n.id,
            position: n.feet(&world),
            yaw: n.yaw(),
            hp: 100.,
        })
        .collect();
    for mut a in sim.snapshot() {
        a.attack_target = None;
        sim.replace_actor(a);
    }
    Ok(notice("Исходные NPC и мобы восстановлены"))
}
