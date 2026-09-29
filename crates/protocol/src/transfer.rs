//! Moves owned bag contents without changing the identity of remaining equipment copies.
use crate::{Character, items, layout, mail::AttachmentRequest};
const STRIDE: u64 = 1 << 32;
pub const MAX_AMOUNT: u32 = 9999;
pub fn eligible(c: &Character) -> Vec<u64> {
    layout::bag(c)
        .values()
        .copied()
        .filter(|t| layout::kind(*t).is_some_and(|k| items::currency(k).is_none()))
        .collect()
}
pub fn take(c: &mut Character, request: &AttachmentRequest) -> Result<u8, &'static str> {
    let token = request.token;
    let kind = layout::kind(token).ok_or("Неизвестное вложение")?;
    if items::currency(kind).is_some() {
        return Err("Пересылка валюты пока недоступна");
    }
    let count = layout::quantity(c, kind);
    let amount = request.quantity;
    if amount == 0 || amount > MAX_AMOUNT || amount > count {
        return Err("Недостаточно вещей для вложения");
    }
    let gear = items::equippable(kind);
    if gear && amount != 1 {
        return Err("Экипировка отправляется по одному экземпляру");
    }
    let bag = layout::bag(c);
    if !bag.values().any(|t| *t == token) {
        return Err("Вещь отсутствует в рюкзаке или уже надета");
    }
    let removed = token % STRIDE;
    // Canonicalize legacy shortcuts while the original copy still exists.
    let bars: Vec<_> = (0..12)
        .map(|n| (n, layout::at(c, layout::Place::Bar(n))))
        .collect();
    let remap = |t: u64| -> u64 {
        if layout::kind(t) != Some(kind) {
            return t;
        }
        if !gear {
            return if count == amount { 0 } else { t };
        }
        let index = t % STRIDE;
        if index == removed {
            0
        } else if index > removed {
            layout::token(kind, (index - 1) as u32)
        } else {
            t
        }
    };
    c.loot.insert(kind, count - amount); // Explicit zero prevents starter equipment from reappearing.
    if gear {
        let key = format!("gear_copy_{kind}");
        if let Some(index) = c.stats.get_mut(&key)
            && *index > removed as f64
        {
            *index -= 1.;
        }
    }
    items::include_loot(&mut c.inventory, &c.loot);
    c.stats.retain(|k, _| !k.starts_with("bag_"));
    for (slot, t) in bag {
        let t = remap(t);
        if t != 0 {
            c.stats.insert(format!("bag_{slot}"), t as f64);
        }
    }
    for (slot, t) in bars {
        c.stats.insert(format!("hotbar_{slot}"), remap(t) as f64);
    }
    Ok(kind)
}
pub fn give(c: &mut Character, kind: u8, amount: u32) -> Result<(), &'static str> {
    if items::definition(kind).is_none()
        || items::currency(kind).is_some()
        || amount == 0
        || amount > MAX_AMOUNT
    {
        return Err("Недопустимое вложение");
    }
    let gear = items::equippable(kind);
    if gear && amount != 1 {
        return Err("Недопустимое количество экипировки");
    }
    let old = layout::quantity(c, kind);
    let count = old.checked_add(amount).ok_or("Нет места в стопке")?;
    let extra = if gear {
        amount as usize
    } else {
        usize::from(old == 0)
    };
    if layout::bag(c).len() + extra > 65536 {
        return Err("Освободите место в рюкзаке");
    }
    c.loot.insert(kind, count);
    items::include_loot(&mut c.inventory, &c.loot);
    // New copies fill free cells; existing bag cells and equipped-copy indices are unchanged.
    Ok(())
}
