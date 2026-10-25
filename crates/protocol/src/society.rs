//! What a stranger may learn, and the physical letter a bird carries.
//! A claimed name and a private note do not certify identity.
//! The voice check is distance plus a clear segment: not the final acoustics.
use serde::{Deserialize, Serialize};

/// [PROPOSAL] How far a raised voice carries before walls are considered.
pub const VOICE_RADIUS: f32 = 25.;
/// [PROPOSAL] How far a bird flies in one hop, and how long the route is.
pub const BIRD_STEP: f32 = 15.;
pub const BIRD_RANGE: f32 = 40.;
pub const DOCUMENT: u8 = 43;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// Say a name. It is a claim, not proof.
    Present { name: String },
    /// Private note about someone whose face is open and nearby.
    Remember { person: i64, note: String },
    /// A covered face removes the automatic recognition hint.
    Cover { hidden: bool },
}
pub fn claim(name: &str) -> Result<&str, &'static str> {
    let name = name.trim();
    if !(2..=16).contains(&name.chars().count()) || name.chars().any(|c| c.is_control()) {
        return Err("Имя для представления: 2–16 символов");
    }
    Ok(name)
}
pub fn note(text: &str) -> Result<&str, &'static str> {
    let text = text.trim();
    if !(1..=80).contains(&text.chars().count()) || text.chars().any(|c| c.is_control()) {
        return Err("Заметка: 1–80 символов");
    }
    Ok(text)
}
/// Label placed in a packet another player receives. The account nickname is not an argument.
pub fn public_label(presented: &str, face_hidden: bool, private_note: Option<&str>) -> String {
    if face_hidden {
        return "Закрытое лицо".into();
    }
    if let Some(note) = private_note.filter(|text| !text.is_empty()) {
        return note.to_string();
    }
    if !presented.trim().is_empty() {
        return presented.trim().to_string();
    }
    "Незнакомец".into()
}
pub fn in_earshot(from: [f32; 2], to: [f32; 2], clear: bool) -> bool {
    clear && (from[0] - to[0]).hypot(from[1] - to[1]) <= VOICE_RADIUS
}
/// One hop of a bird. The letter stays a ground item; a blocked hop ends the flight.
pub fn hop(from: [f32; 2], dest: [f32; 2]) -> ([f32; 2], bool) {
    let delta = [dest[0] - from[0], dest[1] - from[1]];
    let left = delta[0].hypot(delta[1]);
    if left <= BIRD_STEP {
        return (dest, true);
    }
    let scale = BIRD_STEP / left;
    (
        [from[0] + delta[0] * scale, from[1] + delta[1] * scale],
        false,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strangers_do_not_receive_the_account_name() {
        assert_eq!(public_label("", false, None), "Незнакомец");
        assert_eq!(public_label("Борис", false, None), "Борис");
        assert_eq!(
            public_label("Борис", false, Some("грабитель с перевала")),
            "грабитель с перевала"
        );
        assert_eq!(
            public_label("Борис", true, Some("грабитель с перевала")),
            "Закрытое лицо"
        );
        assert!(!in_earshot([0., 0.], [1., 0.], false));
        assert!(in_earshot([0., 0.], [10., 0.], true));
        assert!(!in_earshot([0., 0.], [0., 40.], true));
        let (next, done) = hop([0., 0.], [40., 0.]);
        assert!(!done);
        assert!((next[0] - BIRD_STEP).abs() < 0.01);
    }
}
