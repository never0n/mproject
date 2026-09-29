//! Every damage path produces one defeat. Only this module grants its reward pool.
use super::{Fighter, stat};
use crate::party::Parties;
use mortown_protocol::{Actor, crafting, items, monsters::Kind};
use std::collections::HashMap;

pub(super) struct Defeat {
    pub killer: i64,
    pub position: [f32; 3],
    pub name: String,
    pub kind: Kind,
}
pub(super) fn roll_drops(class: u8, kind: Kind, random: fn() -> f64) -> Vec<u8> {
    // New encounter materials are guaranteed; recipes remain available in the shop.
    if kind == Kind::Marauder {
        return vec![crafting::IRON, crafting::LEATHER];
    }
    if kind == Kind::Apostate {
        return vec![crafting::RUNE_SHARD, 7];
    }
    let mut drops = vec![7, crafting::LEATHER];
    if random() < 0.3 {
        drops.push(if class == 2 { 5 } else { 4 });
    }
    if random() < 0.2 {
        drops.push(6);
    }
    drops
}
pub(super) fn grant_share(
    actor: &mut Actor,
    fighter: &mut Fighter,
    experience: u32,
    coins: u32,
    drops: &[u8],
    grouped: bool,
    kind: Kind,
) {
    let experience = if actor.progress.level < 100 {
        experience
    } else {
        0
    };
    let gain =
        actor
            .progress
            .reward_enemy(&mut fighter.stats, experience, coins, kind == Kind::Bandit);
    actor.reward_status = format!(
        "{}+{experience} опыта · +{coins} монет",
        if grouped { "Группа: " } else { "" }
    );
    if gain > 0 {
        actor.max_hp = stat(&fighter.stats, "hp");
        actor.hp = actor.max_hp;
        actor.mana = stat(&fighter.stats, "mana");
        actor.stamina = stat(&fighter.stats, "stamina");
        actor
            .reward_status
            .push_str(&format!(" · Уровень {}!", actor.progress.level));
    }
    for &kind in drops {
        let quantity = actor.loot.entry(kind).or_default();
        *quantity = quantity.saturating_add(1);
        actor
            .reward_status
            .push_str(&format!(" · {} ×1", items::definition(kind).unwrap().name));
    }
}
pub(super) fn distribute(
    fighters: &mut HashMap<i64, Fighter>,
    actors: &mut [Actor],
    parties: &mut Parties,
    defeats: &mut Vec<Defeat>,
    random: fn() -> f64,
) {
    for defeat in defeats.drain(..) {
        let Some(fighter) = fighters.get(&defeat.killer) else {
            continue;
        };
        // One drop roll per enemy, using the finisher's class as in solo combat.
        let drops = roll_drops(fighter.class, defeat.kind, random);
        let spec = defeat.kind.definition();
        let plan = parties.reward_plan(defeat.killer, defeat.position, actors, drops.len());
        let count = plan.members.len() as u32;
        if count == 0 {
            if let Some(party) = plan.party {
                parties.record_loot(
                    party,
                    format!(
                        "{}: рядом нет живых участников, награда не получена",
                        defeat.name
                    ),
                );
            }
            continue;
        }
        let mut delivery = vec![];
        for (n, id) in plan.members.iter().enumerate() {
            let Some(actor) = actors.iter_mut().find(|a| a.id == *id) else {
                continue;
            };
            let Some(fighter) = fighters.get_mut(id) else {
                continue;
            };
            let experience =
                spec.experience / count + u32::from((n as u32) < spec.experience % count);
            let coins = spec.coins / count + u32::from((n as u32) < spec.coins % count);
            let owned: Vec<_> = drops
                .iter()
                .zip(&plan.loot)
                .filter_map(|(&kind, owner)| (*owner == *id).then_some(kind))
                .collect();
            for &kind in &owned {
                delivery.push(format!(
                    "{}: {} ×1",
                    actor.nickname,
                    items::definition(kind).unwrap().name
                ));
            }
            grant_share(
                actor,
                fighter,
                experience,
                coins,
                &owned,
                plan.party.is_some(),
                defeat.kind,
            );
        }
        if let Some(party) = plan.party {
            parties.record_loot(
                party,
                format!(
                    "{} · награда на {}\n{}",
                    defeat.name,
                    count,
                    delivery.join("\n")
                ),
            );
        }
    }
}
