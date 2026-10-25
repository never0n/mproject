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
/// `magical`: the killer fought with a staff, so the weapon drop is a staff.
pub(super) fn roll_drops(magical: bool, kind: Kind, random: fn() -> f64) -> Vec<u8> {
    // New encounter materials are guaranteed.
    if kind == Kind::Marauder {
        return vec![crafting::IRON, crafting::LEATHER];
    }
    if kind == Kind::Apostate {
        return vec![crafting::RUNE_SHARD, 7];
    }
    let mut drops = vec![7, crafting::LEATHER];
    if random() < 0.3 {
        drops.push(if magical { 5 } else { 4 });
    }
    if random() < 0.2 {
        drops.push(6);
    }
    drops
}
/// Experience applies at once; drops and coins are queued in `grants` and become physical
/// stacks when the owner's connection commits them (bag, or the ground when overloaded).
pub(super) fn grant_share(
    actor: &mut Actor,
    fighter: &mut Fighter,
    experience: u32,
    coins: u32,
    drops: &[u8],
    grouped: bool,
    grants: &mut Vec<(u8, u32)>,
) {
    let gain = actor.progress.reward_share(&mut fighter.stats, experience);
    actor.reward_status = format!(
        "{}+{experience} опыта · +{}",
        if grouped { "Группа: " } else { "" },
        items::format_coins(coins.into())
    );
    if coins > 0 {
        grants.push((items::COPPER, coins));
    }
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
        grants.push((kind, 1));
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
    grants: &mut HashMap<i64, Vec<(u8, u32)>>,
) {
    for defeat in defeats.drain(..) {
        if !fighters.contains_key(&defeat.killer) {
            continue;
        }
        // One drop roll per enemy, using the finisher's weapon as in solo combat.
        let magical = actors
            .iter()
            .find(|a| a.id == defeat.killer)
            .is_some_and(|a| mortown_protocol::weapons::magical(a.equipment.weapon));
        let drops = roll_drops(magical, defeat.kind, random);
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
                grants.entry(*id).or_default(),
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
