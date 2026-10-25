//! One basic level-one entry per weapon family. Existing upgraded items keep their IDs.
pub const AXE: u8 = 19;
pub const HAMMER: u8 = 20;
pub const GREATSWORD: u8 = 21;
pub const GREATAXE: u8 = 22;
pub const GREATCLUB: u8 = 23;
pub const PIKE: u8 = 24;
pub const BOW: u8 = 25;
pub const DAGGER: u8 = 26;
pub const BASIC: &[u8] = &[
    1, AXE, HAMMER, GREATSWORD, GREATAXE, GREATCLUB, PIKE, BOW, DAGGER, 2,
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Family {
    Sword,
    Axe,
    Hammer,
    Greatsword,
    Greataxe,
    Greatclub,
    Pike,
    Bow,
    Dagger,
    Staff,
}
#[derive(Clone, Copy)]
pub struct Definition {
    pub family: Family,
    pub name: &'static str,
    pub hands: u8,
    pub level: u32,
    pub range: f32,
    pub interval_scale: f64,
}
pub fn definition(item: u8) -> Option<Definition> {
    let (family, name, hands, range, interval_scale) = match item {
        1 | 4 | 17 => (Family::Sword, "Меч", 1, 3., 1.),
        2 | 5 | 18 => (Family::Staff, "Магический посох", 2, 12., 1.),
        AXE => (Family::Axe, "Топор", 1, 3., 1.05),
        HAMMER => (Family::Hammer, "Молот", 1, 3., 1.15),
        GREATSWORD => (Family::Greatsword, "Двуручный меч", 2, 3.5, 1.20),
        GREATAXE => (Family::Greataxe, "Двуручный топор", 2, 3.5, 1.30),
        GREATCLUB => (Family::Greatclub, "Двуручная дубина", 2, 3.5, 1.40),
        PIKE => (Family::Pike, "Пика · древковое", 2, 4.5, 1.15),
        BOW => (Family::Bow, "Лук", 2, 18., 1.20),
        DAGGER => (Family::Dagger, "Кинжал", 1, 2.5, 0.85),
        _ => return None,
    };
    Some(Definition {
        family,
        name,
        hands,
        level: 1,
        range,
        interval_scale,
    })
}
/// Staves channel mana: they switch damage, resource and skill variants to magic.
pub fn magical(item: Option<u8>) -> bool {
    item.and_then(definition)
        .is_some_and(|d| d.family == Family::Staff)
}
/// Bare hands fight at melee range.
pub fn attack_range(item: Option<u8>) -> f32 {
    item.and_then(definition).map_or(3., |d| d.range)
}
pub fn interval_scale(item: Option<u8>) -> f64 {
    item.and_then(definition).map_or(1., |d| d.interval_scale)
}
