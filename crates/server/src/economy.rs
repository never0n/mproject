use mortown_protocol::{
    Actor, Character,
    items::{self, Location, Stack},
    transfer::{self, Holder},
};

pub enum Action {
    Potion,
    Craft {
        recipe: u8,
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
    SpendTalent {
        talent: String,
        expected: u32,
    },
}

/// Spends the ingredients from the bag and puts the result there; the recipe stays.
pub fn craft(actor: &mut Actor, c: &mut Character, item: u8) -> Result<(), &'static str> {
    if actor.hp <= 0. {
        return Err("Сначала возродитесь");
    }
    let recipe = mortown_protocol::crafting::definition(item).ok_or("Неизвестный рецепт")?;
    recipe.validate(c)?;
    // Work on a copy: a rejected step never leaves ingredients half spent.
    let mut next = c.items.clone();
    for &(kind, count) in recipe.ingredients {
        transfer::consume(&mut next, kind, count)?;
    }
    transfer::put(
        &mut next,
        Holder::Character,
        Stack::new(recipe.result, 1, Location::Bag { cell: 0 }),
    )?;
    transfer::check_carry(&c.items, &next)?;
    c.items = next;
    actor.reward_status = format!(
        "Создано: {} ×1",
        items::definition(recipe.result).unwrap().name
    );
    Ok(())
}

/// Spends one talent point. Practice never calls this: a talent only moves when a point is spent.
pub fn spend_talent(
    actor: &mut Actor,
    c: &mut Character,
    talent: &str,
    expected: u32,
) -> Result<(), &'static str> {
    let key = mortown_protocol::progression::talent_key(talent).ok_or("Неизвестный талант")?;
    let name = mortown_protocol::progression::TALENTS
        .iter()
        .find(|(id, _)| *id == talent)
        .map(|(_, name)| *name)
        .unwrap();
    if actor.hp <= 0. {
        return Err("Сначала возродитесь");
    }
    if actor.progress.talent_points < 1. {
        return Err("Нет свободных очков таланта");
    }
    let rank = c.stats.get(&key).copied().unwrap_or(0.) as u32;
    if rank != expected {
        return Err("Талант уже изменён; повторите выбор");
    }
    c.stats.insert(key, f64::from(rank + 1));
    actor.progress.talent_points -= 1.;
    actor.progress.write_stats(&mut c.stats);
    actor.reward_status = format!("{name} +1 · потрачено 1 очко таланта");
    Ok(())
}

pub fn potion(actor: &mut Actor, c: &mut Character) -> Result<(), &'static str> {
    if actor.hp <= 0. && !actor.downed {
        return Err("Погибший персонаж не может пить зелья");
    }
    if mortown_protocol::crafting::in_bag(c, items::POTION) == 0 {
        return Err("Нет зелий здоровья");
    }
    if actor.hp >= actor.max_hp {
        return Err("Здоровье уже полное");
    }
    transfer::consume(&mut c.items, items::POTION, 1)?;
    let healed = items::POTION_HEAL.min(actor.max_hp - actor.hp);
    actor.hp += healed;
    actor.reward_status = format!("Зелье здоровья: +{healed:.0} HP");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::simulation::Simulation;
    use std::sync::Arc;
    pub(crate) fn sheet(stacks: &[(u8, u32)]) -> Character {
        Character {
            id: 1,
            slot: 1,
            nickname: "Test".into(),
            gender: 1,
            stats: mortown_protocol::starting_stats().unwrap(),
            items: stacks
                .iter()
                .enumerate()
                .map(|(n, &(kind, quantity))| Stack {
                    id: n as i64 + 1,
                    ..Stack::new(kind, quantity, Location::Bag { cell: n as u16 })
                })
                .collect(),
        }
    }
    #[test]
    fn crafting_validates_all_inputs_before_spending_and_retains_recipe() {
        use mortown_protocol::crafting::{CUIRASS_RECIPE, LEATHER, in_bag};
        let mut sim = Simulation::new(Arc::new(mortown_world::World::new()));
        let mut actor = sim
            .join(1, "Craft".into(), mortown_world::SPAWN, 0.)
            .unwrap();
        for (hp, recipe, stacks) in [
            (100., CUIRASS_RECIPE, vec![(LEATHER, 12), (7, 8)]),
            (
                100.,
                CUIRASS_RECIPE,
                vec![(CUIRASS_RECIPE, 1), (LEATHER, 12), (7, 3)],
            ),
            (
                100.,
                CUIRASS_RECIPE,
                vec![(CUIRASS_RECIPE, 1), (LEATHER, 5), (7, 8)],
            ),
            // Too heavy to carry the result.
            (
                100.,
                CUIRASS_RECIPE,
                vec![(CUIRASS_RECIPE, 1), (LEATHER, 12), (7, 8), (13, 160)],
            ),
            (
                0.,
                CUIRASS_RECIPE,
                vec![(CUIRASS_RECIPE, 1), (LEATHER, 12), (7, 8)],
            ),
            (100., 255, vec![(CUIRASS_RECIPE, 1), (LEATHER, 12), (7, 8)]),
        ] {
            actor.hp = hp;
            let mut c = sheet(&stacks);
            let before = (serde_json::to_value(&actor).unwrap(), c.clone());
            assert!(craft(&mut actor, &mut c, recipe).is_err());
            assert_eq!((serde_json::to_value(&actor).unwrap(), c), before);
        }
        let mut c = sheet(&[(CUIRASS_RECIPE, 1), (LEATHER, 12), (7, 8)]);
        craft(&mut actor, &mut c, CUIRASS_RECIPE).unwrap();
        craft(&mut actor, &mut c, CUIRASS_RECIPE).unwrap();
        assert_eq!(in_bag(&c, CUIRASS_RECIPE), 1);
        assert_eq!(in_bag(&c, LEATHER), 0);
        assert_eq!(in_bag(&c, 7), 0);
        // Two separate cuirasses, each its own stack.
        assert_eq!(c.items.iter().filter(|s| s.kind == 6).count(), 2);
        assert!(craft(&mut actor, &mut c, CUIRASS_RECIPE).is_err());
    }
    #[test]
    fn potion_heals_once_and_never_spends_on_full_or_dead_characters() {
        let mut sim = Simulation::new(Arc::new(mortown_world::World::new()));
        let mut actor = sim
            .join(1, "Potion".into(), mortown_world::SPAWN, 0.)
            .unwrap();
        actor.hp = 10.;
        actor.max_hp = 100.;
        let mut c = sheet(&[]);
        assert!(potion(&mut actor, &mut c).is_err());
        let mut c = sheet(&[(items::POTION, 1)]);
        potion(&mut actor, &mut c).unwrap();
        assert_eq!(actor.hp, 60.);
        assert!(c.items.is_empty());
        assert!(potion(&mut actor, &mut c).is_err());
        assert_eq!(actor.hp, 60.);
        let mut c = sheet(&[(items::POTION, 1)]);
        actor.hp = 100.;
        assert!(potion(&mut actor, &mut c).is_err());
        actor.hp = 0.;
        assert!(potion(&mut actor, &mut c).is_err());
        assert_eq!(c.items[0].quantity, 1);
        actor.hp = 80.;
        potion(&mut actor, &mut c).unwrap();
        assert_eq!(actor.hp, 100.);
    }
    #[test]
    fn talent_spend_is_separate_from_craft_experience() {
        use mortown_protocol::crafting::{CUIRASS_RECIPE, LEATHER};
        let mut sim = Simulation::new(Arc::new(mortown_world::World::new()));
        let mut actor = sim
            .join(1, "Talent".into(), mortown_world::SPAWN, 0.)
            .unwrap();
        let mut c = sheet(&[(CUIRASS_RECIPE, 1), (LEATHER, 6), (7, 4)]);
        let before = c.stats["current_exp"];
        craft(&mut actor, &mut c, CUIRASS_RECIPE).unwrap();
        let xp = mortown_protocol::progression::craft_experience();
        actor.progress.add_experience(xp);
        assert_eq!(actor.progress.experience, before + f64::from(xp));
        assert_eq!(actor.progress.talent_points, 0.);
        assert!(!c.stats.contains_key("talent_craft"));
        actor.progress.talent_points = 1.;
        actor.hp = 0.;
        assert!(spend_talent(&mut actor, &mut c, "craft", 0).is_err());
        actor.hp = 100.;
        spend_talent(&mut actor, &mut c, "craft", 0).unwrap();
        assert_eq!(c.stats["talent_craft"], 1.);
        assert_eq!(actor.progress.talent_points, 0.);
        actor.progress.talent_points = 1.;
        assert!(spend_talent(&mut actor, &mut c, "craft", 0).is_err());
        assert_eq!(c.stats["talent_craft"], 1.);
        assert!(spend_talent(&mut actor, &mut c, "class", 0).is_err());
    }
}
