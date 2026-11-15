//! Shuffle and repeat (contract C-006): the wire types and the pure rules for reading and
//! reaching a state through Apple Music's two transport-bar buttons. SMTC cannot do this (it
//! accepts the calls and does nothing, see `docs/spikes/mvp4.md`), so the driver in `uia.rs` presses
//! the buttons and uses these rules.

use serde::{Deserialize, Serialize};

/// AutomationId of the shuffle toggle in the transport bar (`TogglePattern`, 0 = off, 1 = on).
// verified: spike mvp4, 2026-11-14
pub const SHUFFLE_BUTTON_ID: &str = "ShuffleButton";
/// AutomationId of the repeat button (`InvokePattern`; each press cycles, the Name is the state).
// verified: spike mvp4, 2026-11-14
pub const REPEAT_BUTTON_ID: &str = "RepeatButton";

/// The repeat button's Name for each state (English UI).
// verified: spike mvp4, 2026-11-14 (English UI only; other languages UNVERIFIED)
const NAME_OFF: &str = "Do Not Repeat";
const NAME_ALL: &str = "Repeat All";
const NAME_ONE: &str = "Repeat One";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Repeat {
    Off,
    All,
    One,
}

/// `Modes` on the wire: `{ "shuffle": false, "repeat": "off" }`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub struct Modes {
    pub shuffle: bool,
    pub repeat: Repeat,
}

/// The repeat state a button Name stands for; `None` for any other text (another language, an
/// app update): the caller then reports "cannot read" instead of guessing.
pub fn repeat_from_name(name: &str) -> Option<Repeat> {
    let name = name.trim();
    [
        (NAME_OFF, Repeat::Off),
        (NAME_ALL, Repeat::All),
        (NAME_ONE, Repeat::One),
    ]
    .into_iter()
    .find(|(text, _)| text.eq_ignore_ascii_case(name))
    .map(|(_, repeat)| repeat)
}

fn position(repeat: Repeat) -> usize {
    match repeat {
        Repeat::Off => 0,
        Repeat::All => 1,
        Repeat::One => 2,
    }
}

/// How many presses of the repeat button lead from `from` to `to`: the button cycles
/// off -> all -> one -> off, so it is never more than 2.
pub fn repeat_presses(from: Repeat, to: Repeat) -> usize {
    (position(to) + 3 - position(from)) % 3
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_map_to_states() {
        assert_eq!(repeat_from_name("Do Not Repeat"), Some(Repeat::Off));
        assert_eq!(repeat_from_name("Repeat All"), Some(Repeat::All));
        assert_eq!(repeat_from_name("Repeat One"), Some(Repeat::One));
        assert_eq!(repeat_from_name("  repeat one "), Some(Repeat::One));
    }

    #[test]
    fn unknown_names_are_not_guessed() {
        for name in ["", "Repeat", "Ulangi Semua", "Shuffle", "Repeat Two"] {
            assert_eq!(repeat_from_name(name), None, "{name:?}");
        }
    }

    #[test]
    fn presses_follow_the_cycle() {
        use Repeat::*;
        let expected = [
            (Off, Off, 0),
            (Off, All, 1),
            (Off, One, 2),
            (All, All, 0),
            (All, One, 1),
            (All, Off, 2),
            (One, One, 0),
            (One, Off, 1),
            (One, All, 2),
        ];
        for (from, to, presses) in expected {
            assert_eq!(repeat_presses(from, to), presses, "{from:?} -> {to:?}");
        }
    }

    #[test]
    fn modes_serialise_like_the_contract() {
        let modes = Modes {
            shuffle: true,
            repeat: Repeat::One,
        };
        assert_eq!(
            serde_json::to_value(modes).unwrap(),
            serde_json::json!({ "shuffle": true, "repeat": "one" })
        );
        assert!(serde_json::from_str::<Repeat>("\"sometimes\"").is_err());
    }
}
