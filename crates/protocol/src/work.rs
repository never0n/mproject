//! Peaceful work: gathering, one blade family, a plot, food and a player stall.
//! Numbers live in `character_rules.json` under `work` and are [PROPOSAL].
//! Turning mined gold into coins is not described by canon and is not done here.
use serde::{Deserialize, Serialize};

pub const WOOD: u8 = 30;
pub const GRAIN: u8 = 31;
pub const WATER: u8 = 32;
pub const BREAD: u8 = 33;
pub const SALT: u8 = 34;
pub const SALTED: u8 = 35;
pub const GOLD_ORE: u8 = 36;
pub const STALL: u8 = 37;
pub const BLUEPRINT: u8 = 38;
pub const JUNK_BLADE: u8 = 39;
pub const CRUDE_BLADE: u8 = 40;
pub const SOUND_BLADE: u8 = 41;
pub const TIMBER: u8 = 42;
/// [PROPOSAL] Hits a placed timber block takes, and how wide it is.
pub const BLOCK_INTEGRITY: u8 = 4;
pub const BLOCK_RADIUS: f32 = 0.7;
/// [PROPOSAL] How many beams may stand in one column, counting the one on the ground.
pub const BLOCK_LEVELS: u8 = 4;
/// Beams closer than this share a column. The upper one stands only on the one beneath.
pub const COLUMN: f32 = 1.4;
/// [PROPOSAL] One haul moves the cart, the hauler and nearby cargo by this many metres.
pub const HAUL: f32 = 3.;
/// A container inside this distance of the cart is cargo and keeps its offset.
pub const CARGO: f32 = 3.;

struct Tuning {
    grow_seconds: i64,
    harvest: u32,
    fresh_seconds: i64,
    salted_seconds: i64,
    hunger_per_minute: f64,
    meal: f64,
    drink: f64,
}
fn tuning() -> Tuning {
    let rules: serde_json::Value =
        serde_json::from_str(include_str!("../data/character_rules.json")).expect("bundled rules");
    let work = &rules["work"];
    Tuning {
        grow_seconds: work["growSeconds"].as_i64().unwrap_or(30),
        harvest: work["harvest"].as_u64().unwrap_or(3) as u32,
        fresh_seconds: work["freshSeconds"].as_i64().unwrap_or(600),
        salted_seconds: work["saltedSeconds"].as_i64().unwrap_or(7200),
        hunger_per_minute: work["hungerPerMinute"].as_f64().unwrap_or(1.),
        meal: work["meal"].as_f64().unwrap_or(40.),
        drink: work["drink"].as_f64().unwrap_or(40.),
    }
}
pub fn node_stock() -> i32 {
    let rules: serde_json::Value =
        serde_json::from_str(include_str!("../data/character_rules.json")).expect("bundled rules");
    rules["work"]["nodeStock"].as_i64().unwrap_or(20) as i32
}
pub fn grow_seconds() -> i64 {
    tuning().grow_seconds.max(1)
}
/// [PROPOSAL] Better soil finishes sooner. 3 is the starting fertile ground, 1 is poor.
/// Fertilizer that would raise poor soil is not defined yet.
pub fn grow_for(fertility: u8) -> i64 {
    let base = grow_seconds();
    match fertility {
        3 => base,
        2 => base + base / 2,
        _ => base * 2,
    }
}
pub fn harvest_amount() -> u32 {
    tuning().harvest.max(1)
}
pub fn fresh_seconds() -> i64 {
    tuning().fresh_seconds.max(1)
}
pub fn salted_seconds() -> i64 {
    tuning().salted_seconds.max(fresh_seconds())
}

/// Iron, wood or gold ore. Gold ore is a soft metal for this family: it does not become coins.
pub fn blade_material(kind: u8) -> bool {
    matches!(kind, WOOD | crate::items::IRON | GOLD_ORE)
}
fn material_base(kind: u8) -> Option<i32> {
    match kind {
        WOOD => Some(2),
        crate::items::IRON => Some(8),
        GOLD_ORE => Some(1),
        _ => None,
    }
}
/// One blade family. The same material, geometry and process always yield the same kind.
/// `geometry` is 0 short, 1 balanced, 2 long. `process` is 0 crude, 1 tempered.
/// Craft talent rank is the highest parameter the maker may set.
pub fn design(material: u8, geometry: u8, process: u8, talent: u32) -> Result<u8, &'static str> {
    let base = material_base(material).ok_or("Этот материал не для клинка")?;
    if geometry > 2 || process > 1 {
        return Err("Нет такого параметра");
    }
    if u32::from(geometry) > talent || u32::from(process) > talent {
        return Err("Не хватает ремесленного таланта");
    }
    let mut score =
        base * match geometry {
            0 => 7,
            1 => 10,
            _ => 12,
        } / 10;
    if geometry == 2 && base < 4 {
        score -= 6;
    }
    if process == 1 && base >= 4 {
        score += 4;
    }
    Ok(if score < 4 {
        JUNK_BLADE
    } else if score < 12 {
        CRUDE_BLADE
    } else {
        SOUND_BLADE
    })
}
pub fn pack_plan(material: u8, geometry: u8, process: u8) -> i64 {
    (i64::from(material) << 16) | (i64::from(geometry) << 8) | i64::from(process)
}
pub fn unpack_plan(code: i64) -> Option<(u8, u8, u8)> {
    let material = u8::try_from((code >> 16) & 0xff).ok()?;
    let geometry = u8::try_from((code >> 8) & 0xff).ok()?;
    let process = u8::try_from(code & 0xff).ok()?;
    blade_material(material).then_some((material, geometry, process))
}
pub fn plan_key(part: &str) -> String {
    format!("plan_{part}")
}

pub fn expiry(kind: u8, now: i64) -> Option<i64> {
    let life = match kind {
        BREAD => fresh_seconds(),
        SALTED => salted_seconds(),
        _ => return None,
    };
    Some(now.saturating_add(life))
}
pub fn edible(kind: u8, until: Option<i64>, now: i64) -> bool {
    matches!(kind, BREAD | SALTED) && until.is_some_and(|t| t > now)
}
pub fn meal() -> f64 {
    tuning().meal
}
pub fn drink() -> f64 {
    tuning().drink
}
/// One second of hunger and thirst, clamped at 100.
pub fn pass_time(hunger: f64, thirst: f64, seconds: f64) -> (f64, f64) {
    let step = tuning().hunger_per_minute / 60. * seconds;
    (
        (hunger + step).clamp(0., 100.),
        (thirst + step).clamp(0., 100.),
    )
}
pub fn starving(hunger: f64, thirst: f64) -> bool {
    hunger >= 100. || thirst >= 100.
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: i64,
    pub product: u8,
    pub position: [f32; 2],
    pub remaining: i32,
}
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plot {
    pub id: i64,
    /// 0 empty, 1 sown, 2 watered and growing.
    pub stage: u8,
    pub position: [f32; 2],
    pub ready: bool,
    /// Character who received the starting plot. Empty plots stay workable by anyone nearby.
    #[serde(default)]
    pub owner: Option<i64>,
    /// 1 poor, 2 ordinary, 3 fertile. Missing on an old packet means fertile.
    #[serde(default = "fertile")]
    pub fertility: u8,
}
fn fertile() -> u8 {
    3
}
/// A timber block sitting on the ground. Integrity falls only when someone hits it.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Block {
    pub id: i64,
    pub position: [f32; 2],
    pub integrity: u8,
    /// 0 stands on the ground. Each step above stands on the previous step of this column.
    #[serde(default)]
    pub level: u8,
}
pub fn same_column(a: [f32; 2], b: [f32; 2]) -> bool {
    (a[0] - b[0]).hypot(a[1] - b[1]) < COLUMN
}
/// Beams that fall when `removed` is gone. A neighbor with its own ground support stays.
/// What falls is only a beam that no longer has the beam directly under it.
pub fn collapse(blocks: &[Block], removed: i64) -> Vec<i64> {
    let left: Vec<&Block> = blocks.iter().filter(|block| block.id != removed).collect();
    let mut stands = vec![false; left.len()];
    for level in 0..BLOCK_LEVELS {
        for (i, block) in left.iter().enumerate() {
            if block.level != level {
                continue;
            }
            stands[i] = level == 0
                || left.iter().enumerate().any(|(j, under)| {
                    stands[j]
                        && under.level + 1 == level
                        && same_column(under.position, block.position)
                });
        }
    }
    left.iter()
        .enumerate()
        .filter(|(i, _)| !stands[*i])
        .map(|(_, block)| block.id)
        .collect()
}
/// True when the straight step comes within [BLOCK_RADIUS] of a placed block.
pub fn blocks_segment(blocks: &[Block], a: [f32; 2], b: [f32; 2]) -> bool {
    blocks.iter().any(|block| {
        let ab = [b[0] - a[0], b[1] - a[1]];
        let len2 = ab[0] * ab[0] + ab[1] * ab[1];
        let t = if len2 < 1e-6 {
            0.
        } else {
            (((block.position[0] - a[0]) * ab[0] + (block.position[1] - a[1]) * ab[1]) / len2)
                .clamp(0., 1.)
        };
        let closest = [a[0] + ab[0] * t, a[1] + ab[1] * t];
        (closest[0] - block.position[0]).hypot(closest[1] - block.position[1]) < BLOCK_RADIUS
    })
}
/// Shared shift of a person and a container standing on the same carrier.
/// Offsets stay fixed, so a later cart or ship can move both without baking them into the ground.
pub fn shift_ride(
    origin: &mut [f32; 2],
    actor: &mut [f32; 2],
    container: &mut [f32; 2],
    delta: [f32; 2],
) {
    origin[0] += delta[0];
    origin[1] += delta[1];
    actor[0] += delta[0];
    actor[1] += delta[1];
    container[0] += delta[0];
    container[1] += delta[1];
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    Gather {
        node: i64,
    },
    Tend {
        plot: i64,
    },
    Design {
        material: u8,
        geometry: u8,
        process: u8,
    },
    WriteBlueprint,
    CraftPlan {
        paper: Option<i64>,
    },
    Cook,
    Salt {
        item: i64,
    },
    Eat {
        item: i64,
    },
    Drink {
        item: i64,
    },
    MakeStall,
    /// Four pieces of wood become one timber beam.
    MakeTimber,
    /// Put the beam on the ground ahead of the character. The spot must be standing ground.
    PlaceTimber,
    /// One hit. At zero integrity the block is gone. Anything standing only on it falls.
    Damage {
        block: i64,
    },
    /// Eight pieces of wood become a cart.
    MakeCart,
    /// Push the nearest cart. The hauler and a container on it move by the same step.
    Haul,
    /// A physical letter. The text is on the item, not in a mailbox.
    WriteDocument {
        text: String,
    },
    /// A bird carries the letter ahead. The item can be picked up on the way.
    DispatchDocument,
    Price {
        container: i64,
        price: u64,
    },
    Buy {
        container: i64,
        price: u64,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_same_plan_is_stable_and_a_bad_one_is_junk() {
        let sound = design(crate::items::IRON, 1, 1, 1).unwrap();
        assert_eq!(sound, design(crate::items::IRON, 1, 1, 1).unwrap());
        assert_eq!(sound, SOUND_BLADE);
        assert_eq!(design(WOOD, 2, 0, 2).unwrap(), JUNK_BLADE);
        assert_eq!(design(GOLD_ORE, 1, 1, 1).unwrap(), JUNK_BLADE);
        assert!(design(WOOD, 2, 0, 0).is_err());
        assert!(design(7, 0, 0, 5).is_err());
        let code = pack_plan(crate::items::IRON, 1, 1);
        assert_eq!(unpack_plan(code), Some((crate::items::IRON, 1, 1)));
    }
    #[test]
    fn hunger_and_freshness_follow_the_proposal() {
        let (hunger, thirst) = pass_time(0., 10., 60.);
        assert!((hunger - 1.).abs() < 1e-6);
        assert!((thirst - 11.).abs() < 1e-6);
        assert_eq!(pass_time(99., 100., 120.).0, 100.);
        assert!(starving(100., 0.));
        assert!(!edible(BREAD, Some(10), 10));
        assert!(edible(BREAD, Some(11), 10));
        assert!(!edible(WOOD, Some(99), 0));
    }
    #[test]
    fn a_block_stops_a_step_and_a_ride_keeps_offsets() {
        let block = Block {
            id: 1,
            position: [2., 0.],
            integrity: BLOCK_INTEGRITY,
            level: 0,
        };
        assert!(blocks_segment(&[block], [0., 0.], [3., 0.]));
        assert!(!blocks_segment(&[block], [0., 0.], [0., 2.]));
        let (mut origin, mut actor, mut container) = ([0., 0.], [0.2, 0.], [0., 0.4]);
        shift_ride(&mut origin, &mut actor, &mut container, [5., -1.]);
        assert_eq!(origin, [5., -1.]);
        assert_eq!(actor, [5.2, -1.]);
        assert_eq!(container, [5., -0.6]);
    }
    #[test]
    fn a_beam_falls_only_when_its_own_support_is_gone() {
        let ground = Block {
            id: 1,
            position: [0., 0.],
            integrity: 4,
            level: 0,
        };
        let upper = Block {
            id: 2,
            position: [0., 0.],
            integrity: 4,
            level: 1,
        };
        let neighbor = Block {
            id: 3,
            position: [3., 0.],
            integrity: 4,
            level: 0,
        };
        assert_eq!(collapse(&[ground, upper, neighbor], 1), vec![2]);
        assert!(collapse(&[ground, upper, neighbor], 3).is_empty());
        assert_eq!(grow_for(3), grow_seconds());
        assert!(grow_for(1) > grow_for(3));
    }
}
