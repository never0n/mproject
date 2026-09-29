use mortown_protocol::{Actor, Character, items, quests};

pub enum Action {
    Trade {
        npc: u8,
        item: u8,
        buy: bool,
        sale: Option<mortown_protocol::trade::Sale>,
    },
    Potion,
    Craft {
        recipe: u8,
        expected: u32,
        specialization_revision: u32,
    },
    Specialize {
        profession: mortown_protocol::professions::Profession,
        expected: u32,
    },
    SpendAttribute {
        stat: String,
        expected: u32,
    },
    Arrange {
        source: mortown_protocol::layout::Place,
        target: mortown_protocol::layout::Place,
        expected: u64,
    },
    BindHotbar {
        slot: u8,
        binding: u8,
    },
    LearnSecondary,
    LearnAbility {
        skill: u8,
    },
    LearnSkill {
        rank: u8,
    },
    Quest {
        quest: u8,
        complete: bool,
    },
}

pub fn craft(
    actor: &mut Actor,
    item: u8,
    stats: &std::collections::BTreeMap<String, f64>,
) -> Result<(), &'static str> {
    if actor.hp <= 0. {
        return Err("Сначала возродитесь");
    }
    let recipe = mortown_protocol::crafting::definition(item).ok_or("Неизвестный рецепт")?;
    recipe.validate_with(&actor.loot, stats)?;
    // Validate everything before spending any ingredient. The caller persists this atomically.
    for (kind, count) in recipe.requirements(stats) {
        *actor.loot.get_mut(&kind).unwrap() -= count;
    }
    *actor.loot.entry(recipe.result).or_default() += 1;
    if let Some(q) = quests::definition(quests::CRAFT_QUEST)
        && q.status(&actor.progress) == quests::Status::Active
        && matches!(q.objective, quests::Objective::Craft(kind) if kind == recipe.result)
    {
        actor.progress.quest_crafted = true;
    }
    actor.reward_status = format!(
        "Создано: {} ×1",
        items::definition(recipe.result).unwrap().name
    );
    Ok(())
}

pub fn learn_ability(actor: &mut Actor, class: u8, race: u8, id: u8) -> Result<(), &'static str> {
    let ability = mortown_protocol::skills::definition(class, race, id)
        .ok_or("Навык недоступен персонажу")?;
    if actor.hp <= 0. {
        return Err("Сначала возродитесь");
    }
    if mortown_protocol::skills::learned(&actor.progress, id) {
        return Err("Навык уже изучен");
    }
    if actor.progress.level < ability.level {
        return Err("Недостаточный уровень персонажа");
    }
    if actor.progress.sp < ability.sp {
        return Err("Недостаточно SP");
    }
    actor.progress.sp -= ability.sp;
    actor.progress.extra_skills |= 1 << (id - 3);
    actor.reward_status = format!("Изучено: {} · −{} SP", ability.skill.name, ability.sp);
    Ok(())
}

pub fn learn_secondary(actor: &mut Actor) -> Result<(), &'static str> {
    if actor.hp <= 0. {
        return Err("Сначала возродитесь");
    }
    if actor.progress.secondary_learned {
        return Err("Второй навык уже изучен");
    }
    if actor.progress.level < 3 {
        return Err("Требуется третий уровень");
    }
    if actor.progress.sp < 7. {
        return Err("Недостаточно SP: требуется 7");
    }
    actor.progress.sp -= 7.;
    actor.progress.secondary_learned = true;
    actor.reward_status = "Изучен второй навык [2] · −7 SP".into();
    Ok(())
}

pub fn learn(actor: &mut Actor, target: u8) -> Result<(), &'static str> {
    if actor.hp <= 0. {
        return Err("Сначала возродитесь");
    }
    let (level, cost) =
        mortown_protocol::skills::upgrade_cost(target).ok_or("Недопустимый ранг навыка")?;
    if target != actor.progress.skill_rank + 1 {
        return Err("Этот ранг уже изучен или предыдущий ещё недоступен");
    }
    if actor.progress.level < level {
        return Err("Недостаточный уровень персонажа");
    }
    if actor.progress.sp < cost {
        return Err("Недостаточно SP");
    }
    actor.progress.sp -= cost;
    actor.progress.skill_rank = target;
    actor.reward_status = format!("Навык улучшен до ранга {target} · −{cost:.0} SP");
    Ok(())
}

pub fn quest(
    actor: &mut Actor,
    character: &mut Character,
    id: u8,
    complete: bool,
    world: &mortown_world::World,
) -> Result<(), &'static str> {
    if actor.hp <= 0. {
        return Err("Сначала возродитесь");
    }
    if !mortown_world::npcs::definition(mortown_world::npcs::PLAZA_TRADER)
        .unwrap()
        .available(world, actor.position)
    {
        return Err("Подойдите к Марену у центральной площади");
    }
    let q = quests::definition(id).ok_or("Неизвестное поручение")?;
    if complete {
        if !q.ready(&actor.progress, &actor.loot) {
            return Err("Задание ещё не выполнено или награда уже получена");
        }
        // Work on a copy: failed delivery or a full reward stack must spend nothing.
        let mut bag = character.clone();
        bag.loot = actor.loot.clone();
        actor.progress.write_stats(&mut bag.stats);
        items::include_loot(&mut bag.inventory, &bag.loot);
        if let quests::Objective::Supplies(materials) = q.objective {
            for &(kind, quantity) in materials {
                mortown_protocol::transfer::take(
                    &mut bag,
                    &mortown_protocol::mail::AttachmentRequest {
                        token: mortown_protocol::layout::token(kind, 0),
                        quantity,
                        price: 0,
                    },
                )?;
            }
        }
        for &(kind, count) in q.rewards {
            mortown_protocol::transfer::give(&mut bag, kind, count)?;
        }
        actor.progress.quest_stage = q.active_stage() + 1;
        actor.progress.coins += f64::from(q.coins);
        actor.progress.sp += f64::from(q.sp);
        actor.loot = bag.loot.clone();
        *character = bag;
        actor.reward_status = format!("{} выполнено: {}", q.title, q.reward_text());
    } else {
        if q.status(&actor.progress) != quests::Status::Available {
            return Err("Задание недоступно, уже принято или выполнено");
        }
        actor.progress.quest_stage = q.active_stage();
        if id == 1 {
            actor.progress.quest_kills = 0;
        }
        if id == quests::CRAFT_QUEST {
            actor.progress.quest_crafted = false;
        }
        actor.reward_status = format!("Принято поручение: {}", q.title);
    }
    Ok(())
}

pub fn apply(
    actor: &mut Actor,
    inventory: &[items::Item],
    action: Option<(u8, u8, bool)>,
    world: &mortown_world::World,
) -> Result<(), &'static str> {
    apply_at(actor, inventory, action, world, None)
}
pub fn apply_at(
    actor: &mut Actor,
    inventory: &[items::Item],
    action: Option<(u8, u8, bool)>,
    world: &mortown_world::World,
    resident: Option<mortown_world::npcs::Npc>,
) -> Result<(), &'static str> {
    if actor.hp <= 0. {
        return Err("Погибший персонаж не может торговать или пить зелья");
    }
    if let Some((npc, kind, buy)) = action {
        // Old starter items predate explicit loot quantities; absence means one owned copy.
        let owned = actor
            .loot
            .get(&kind)
            .copied()
            .unwrap_or_else(|| u32::from(kind < 4 && inventory.iter().any(|i| i.kind == kind)));
        let trader = resident
            .or_else(|| mortown_world::npcs::definition(npc).copied())
            .ok_or("Неизвестный торговец")?;
        if trader.role != mortown_world::npcs::Role::Merchant
            || !trader.available(world, actor.position)
        {
            return Err("Подойдите к торговцу: лавка недоступна через стены или издалека");
        }
        if buy {
            let price = items::purchase_price(kind, &actor.progress)
                .ok_or("Торговец не продаёт этот предмет")?;
            if actor.progress.coins < price as f64 {
                return Err("Недостаточно монет");
            }
            let count = owned.checked_add(1).ok_or("Нет места в стопке")?;
            actor.progress.coins -= price as f64;
            actor.loot.insert(kind, count);
            actor.reward_status = format!(
                "Куплено: {} · −{price} монет",
                items::definition(kind).unwrap().name
            );
        } else {
            let price = items::sell_price(kind).ok_or("Этот предмет нельзя продать")?;
            let count = owned;
            if count == 0 {
                return Err("У вас нет этого предмета");
            }
            if count == 1 && inventory.iter().any(|i| i.kind == kind && i.equipped) {
                return Err("Сначала снимите надетый предмет");
            }
            actor.loot.insert(kind, count - 1);
            actor.progress.coins += price as f64;
            actor.reward_status = format!(
                "Продано: {} ×1 · +{} монет",
                items::definition(kind).unwrap().name,
                price
            );
        }
    } else {
        let count = actor.loot.get(&8).copied().unwrap_or(0);
        if count == 0 {
            return Err("Нет зелий здоровья");
        }
        if actor.hp >= actor.max_hp {
            return Err("Здоровье уже полное");
        }
        let healed = items::POTION_HEAL.min(actor.max_hp - actor.hp);
        actor.hp += healed;
        actor.loot.insert(8, count - 1);
        actor.reward_status = format!("Зелье здоровья: +{healed:.0} HP");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::Simulation;
    use std::sync::Arc;
    #[test]
    fn crafting_validates_all_inputs_before_spending_and_retains_recipe() {
        use mortown_protocol::crafting::{CUIRASS_RECIPE, LEATHER};
        let mut sim = Simulation::new(Arc::new(mortown_world::World::new()));
        let mut actor = sim
            .join(1, "Craft".into(), mortown_world::SPAWN, 0.)
            .unwrap();
        for (hp, recipe, loot) in [
            (
                100.,
                CUIRASS_RECIPE,
                [(CUIRASS_RECIPE, 0), (LEATHER, 12), (7, 8), (6, 0)],
            ),
            (
                100.,
                CUIRASS_RECIPE,
                [(CUIRASS_RECIPE, 1), (LEATHER, 12), (7, 3), (6, 0)],
            ),
            (
                100.,
                CUIRASS_RECIPE,
                [(CUIRASS_RECIPE, 1), (LEATHER, 5), (7, 8), (6, 0)],
            ),
            (
                100.,
                CUIRASS_RECIPE,
                [(CUIRASS_RECIPE, 1), (LEATHER, 12), (7, 8), (6, u32::MAX)],
            ),
            (
                0.,
                CUIRASS_RECIPE,
                [(CUIRASS_RECIPE, 1), (LEATHER, 12), (7, 8), (6, 0)],
            ),
            (
                100.,
                255,
                [(CUIRASS_RECIPE, 1), (LEATHER, 12), (7, 8), (6, 0)],
            ),
        ] {
            actor.hp = hp;
            actor.loot = loot.into();
            let before = serde_json::to_value(&actor).unwrap();
            assert!(craft(&mut actor, recipe, &Default::default()).is_err());
            assert_eq!(serde_json::to_value(&actor).unwrap(), before);
        }
        actor.loot = [(CUIRASS_RECIPE, 1), (LEATHER, 12), (7, 8)].into();
        let coins = actor.progress.coins;
        craft(&mut actor, CUIRASS_RECIPE, &Default::default()).unwrap();
        craft(&mut actor, CUIRASS_RECIPE, &Default::default()).unwrap();
        assert_eq!(actor.loot[&CUIRASS_RECIPE], 1);
        assert_eq!(actor.loot[&LEATHER], 0);
        assert_eq!(actor.loot[&7], 0);
        assert_eq!(actor.loot[&6], 2);
        assert_eq!(actor.progress.coins, coins);
        assert!(craft(&mut actor, CUIRASS_RECIPE, &Default::default()).is_err());
        let mut inventory = vec![];
        items::include_loot(&mut inventory, &actor.loot);
        assert!(items::equip(&mut inventory, 1, CUIRASS_RECIPE, true).is_err());
    }
    #[test]
    fn trade_validates_distance_money_equipment_and_potions() {
        let world = Arc::new(mortown_world::World::new());
        assert!(world.walkable(mortown_world::MERCHANT[0], mortown_world::MERCHANT[1]));
        assert!(
            mortown_world::npcs::definition(mortown_world::npcs::PLAZA_TRADER)
                .unwrap()
                .available(
                    &world,
                    [
                        mortown_world::SPAWN[0],
                        world.surface_height(mortown_world::SPAWN[0], mortown_world::SPAWN[1]),
                        mortown_world::SPAWN[1]
                    ]
                )
        );
        let mut sim = Simulation::new(world.clone());
        let mut actor = sim
            .join(1, "Shop".into(), mortown_world::SPAWN, 0.)
            .unwrap();
        actor.hp = 10.;
        actor.max_hp = 100.;
        actor.progress.coins = 15.;
        let mut inventory = vec![items::Item {
            kind: 4,
            equipped: true,
        }];
        actor.loot.insert(4, 2);
        apply(
            &mut actor,
            &inventory,
            Some((mortown_world::npcs::PLAZA_TRADER, 4, false)),
            &world,
        )
        .unwrap();
        assert_eq!(actor.loot[&4], 1);
        assert!(
            apply(
                &mut actor,
                &inventory,
                Some((mortown_world::npcs::PLAZA_TRADER, 4, false)),
                &world
            )
            .is_err()
        );
        assert!(
            apply(
                &mut actor,
                &inventory,
                Some((mortown_world::npcs::PLAZA_TRADER, 1, false)),
                &world
            )
            .is_err()
        );
        assert!(
            apply(
                &mut actor,
                &inventory,
                Some((mortown_world::npcs::PLAZA_TRADER, 4, true)),
                &world
            )
            .is_err()
        );
        assert_eq!(actor.progress.coins, 23.);
        apply(
            &mut actor,
            &inventory,
            Some((mortown_world::npcs::PLAZA_TRADER, 8, true)),
            &world,
        )
        .unwrap();
        assert_eq!(actor.progress.coins, 8.);
        assert!(
            apply(
                &mut actor,
                &inventory,
                Some((mortown_world::npcs::PLAZA_TRADER, 8, true)),
                &world
            )
            .is_err()
        );
        apply(&mut actor, &inventory, None, &world).unwrap();
        assert_eq!(actor.hp, 60.);
        assert_eq!(actor.loot[&8], 0);
        assert!(apply(&mut actor, &inventory, None, &world).is_err());
        actor.loot.insert(8, 1);
        actor.hp = 100.;
        assert!(apply(&mut actor, &inventory, None, &world).is_err());
        assert_eq!(actor.loot[&8], 1);
        actor.hp = 0.;
        assert!(apply(&mut actor, &inventory, None, &world).is_err());
        assert!(
            apply(
                &mut actor,
                &inventory,
                Some((mortown_world::npcs::PLAZA_TRADER, 4, false)),
                &world
            )
            .is_err()
        );
        actor.hp = 10.;
        actor.position[0] += 100.;
        inventory[0].equipped = false;
        assert!(
            apply(
                &mut actor,
                &inventory,
                Some((mortown_world::npcs::PLAZA_TRADER, 4, false)),
                &world
            )
            .is_err()
        );
        actor.position[0] -= 100.;
        apply(
            &mut actor,
            &inventory,
            Some((mortown_world::npcs::PLAZA_TRADER, 4, false)),
            &world,
        )
        .unwrap();
        items::include_loot(&mut inventory, &actor.loot);
        assert!(!inventory.iter().any(|i| i.kind == 4));
        assert!(items::equip(&mut inventory, 1, 4, true).is_err());
        assert!(items::equip(&mut inventory, 1, 8, true).is_err());
    }
}

#[cfg(test)]
mod quest_tests {
    use super::*;
    use crate::simulation::Simulation;
    use std::sync::Arc;
    #[test]
    fn quest_requires_acceptance_counts_new_kills_and_pays_once() {
        let world = Arc::new(mortown_world::World::new());
        let mut sim = Simulation::new(world.clone());
        let mut actor = sim
            .join(1, "Quest".into(), mortown_world::SPAWN, 0.)
            .unwrap();
        let mut stats = mortown_protocol::starting_stats(1, 1).unwrap();
        actor.progress = mortown_protocol::progression::Progress::from_stats(&stats);
        let mut character = Character {
            id: actor.id,
            slot: 0,
            nickname: actor.nickname.clone(),
            race: 1,
            class: 1,
            gender: 1,
            stats: stats.clone(),
            inventory: Vec::new(),
            loot: actor.loot.clone(),
        };
        actor.progress.reward(&mut stats);
        assert_eq!(actor.progress.quest_kills, 0);
        assert!(quest(&mut actor, &mut character, 1, true, &world).is_err());
        actor.position[0] += 100.;
        assert!(quest(&mut actor, &mut character, 1, false, &world).is_err());
        actor.position[0] -= 100.;
        actor.hp = 0.;
        assert!(quest(&mut actor, &mut character, 1, false, &world).is_err());
        actor.hp = 100.;
        quest(&mut actor, &mut character, 1, false, &world).unwrap();
        assert!(quest(&mut actor, &mut character, 1, false, &world).is_err());
        for _ in 0..2 {
            actor.progress.reward(&mut stats);
        }
        assert!(quest(&mut actor, &mut character, 1, true, &world).is_err());
        actor.progress.reward(&mut stats);
        assert_eq!(actor.progress.quest_kills, 3);
        actor.progress.reward(&mut stats);
        assert_eq!(actor.progress.quest_kills, 3);
        let coins = actor.progress.coins;
        quest(&mut actor, &mut character, 1, true, &world).unwrap();
        assert_eq!(actor.progress.coins, coins + 75.);
        assert_eq!(actor.loot[&8], 2);
        assert!(quest(&mut actor, &mut character, 1, true, &world).is_err());
        assert!(quest(&mut actor, &mut character, 1, false, &world).is_err());
        assert_eq!(actor.loot[&8], 2);
        actor.progress.write_stats(&mut stats);
        assert_eq!(
            mortown_protocol::progression::Progress::from_stats(&stats),
            actor.progress
        );
    }
}

#[cfg(test)]
mod skill_tests {
    use super::*;
    use crate::simulation::Simulation;
    use std::sync::Arc;
    #[test]
    fn skill_learning_requires_level_sp_and_next_rank_once() {
        let mut sim = Simulation::new(Arc::new(mortown_world::World::new()));
        let mut actor = sim
            .join(1, "Learn".into(), mortown_world::SPAWN, 0.)
            .unwrap();
        actor.progress.sp = 12.;
        assert!(learn(&mut actor, 2).is_err());
        actor.progress.level = 3;
        assert!(learn(&mut actor, 3).is_err());
        actor.hp = 0.;
        assert!(learn(&mut actor, 2).is_err());
        actor.hp = 100.;
        learn(&mut actor, 2).unwrap();
        assert_eq!(actor.progress.sp, 7.);
        assert!(learn(&mut actor, 2).is_err());
        assert_eq!(actor.progress.sp, 7.);
        actor.progress.sp = 6.;
        assert!(learn(&mut actor, 3).is_err());
        assert_eq!(actor.progress.skill_rank, 2);
        actor.progress.sp = 7.;
        learn(&mut actor, 3).unwrap();
        assert_eq!(actor.progress.sp, 0.);
        assert!(learn(&mut actor, 4).is_err());
        assert!(learn(&mut actor, 0).is_err());
        assert_eq!(actor.progress.skill_rank, 3);
    }
}

#[cfg(test)]
mod secondary_tests {
    use super::*;
    use crate::simulation::Simulation;
    use std::sync::Arc;
    #[test]
    fn second_skill_unlock_checks_life_level_sp_and_duplicate_requests() {
        let mut sim = Simulation::new(Arc::new(mortown_world::World::new()));
        let mut a = sim
            .join(1, "Second".into(), mortown_world::SPAWN, 0.)
            .unwrap();
        a.progress.sp = 7.;
        assert!(learn_secondary(&mut a).is_err());
        a.progress.level = 3;
        a.hp = 0.;
        assert!(learn_secondary(&mut a).is_err());
        a.hp = 100.;
        a.progress.sp = 6.;
        assert!(learn_secondary(&mut a).is_err());
        a.progress.sp = 7.;
        learn_secondary(&mut a).unwrap();
        assert!(a.progress.secondary_learned);
        assert_eq!(a.progress.sp, 0.);
        assert!(learn_secondary(&mut a).is_err());
        assert_eq!(a.progress.sp, 0.);
    }
}
