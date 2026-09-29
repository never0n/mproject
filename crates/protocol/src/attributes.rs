use std::collections::BTreeMap;
pub const ATTRIBUTES: [(&str, &str); 6] = [
    ("STR", "Сила"),
    ("DEX", "Ловкость"),
    ("CON", "Выносливость"),
    ("INT_", "Интеллект"),
    ("WIT", "Мудрость"),
    ("MEN", "Дух"),
];

/// Rebuild level-one bases, avoiding accumulated rounding and equipment bonuses.
pub fn increase(
    stats: &mut BTreeMap<String, f64>,
    race: u8,
    class: u8,
    key: &str,
    expected: u32,
) -> Result<(), &'static str> {
    if !ATTRIBUTES.iter().any(|(k, _)| *k == key) {
        return Err("Эту характеристику нельзя повысить");
    }
    let old = stats.get(key).copied().ok_or("Характеристика не найдена")?;
    if old != expected as f64 {
        return Err("Значение изменилось; повторите выбор");
    }
    let mut primary = stats.clone();
    primary.insert(key.into(), old + 1.);
    let baseline = crate::starting_stats_with_primary(race, class, &primary)
        .ok_or("Не удалось пересчитать характеристики")?;
    for (k, v) in baseline {
        if k.starts_with("f_")
            || k.starts_with("m_")
            || [
                "hp",
                "mana",
                "stamina",
                "speed",
                "initiative",
                "STR",
                "DEX",
                "CON",
                "INT_",
                "WIT",
                "MEN",
                "LUC",
            ]
            .contains(&k.as_str())
        {
            stats.insert(k, v);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rebase_preserves_progress_and_rejects_stale_or_unknown_requests() {
        let mut stats = crate::starting_stats(1, 1).unwrap();
        stats.insert("level".into(), 6.);
        stats.insert("coins".into(), 123.);
        stats.insert("current_hp".into(), 50.);
        let con = stats["CON"] as u32;
        increase(&mut stats, 1, 1, "CON", con).unwrap();
        assert_eq!(stats["CON"], con as f64 + 1.);
        assert_eq!(stats["coins"], 123.);
        assert_eq!(stats["current_hp"], 50.);
        assert_eq!(stats["level"], 6.);
        let saved = stats.clone();
        assert!(increase(&mut stats, 1, 1, "CON", con).is_err());
        assert!(increase(&mut stats, 1, 1, "coins", 123).is_err());
        assert_eq!(stats, saved);
    }
}
