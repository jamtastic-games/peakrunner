//! Bounded, authoritative match communications. Names are captured at event time.
use serde::{Deserialize, Serialize};

pub const CAPACITY: usize = 16;
pub const MAX_BYTES: usize = 240;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Entry {
    Chat { sender: String, text: String },
    TeamChat { sender: String, text: String, team: crate::sim::Team },
    Frag { killer: String, victim: String, weapon: String },
    /// A server-announced play (football passes, tackles, touchdowns).
    Play { text: String },
}

pub fn safe_text(text: &str) -> bool {
    !text.chars().any(|c| c.is_control() || matches!(c,
        '\u{061c}' | '\u{200e}' | '\u{200f}' | '\u{2028}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
}

pub fn message(text: &str) -> Option<String> {
    if text.len() > MAX_BYTES || !safe_text(text) { return None; }
    let text = text.trim();
    (!text.is_empty() && text.len() <= MAX_BYTES && text.chars().count() <= 160 && safe_text(text))
        .then(|| text.to_owned())
}

pub fn push(feed: &mut Vec<Entry>, entry: Entry) {
    if feed.len() >= CAPACITY { feed.drain(..feed.len() - CAPACITY + 1); }
    feed.push(entry);
}

impl Entry {
    pub fn line(&self) -> String {
        match self {
            Self::Chat { sender, text } => format!("{sender}: {text}"),
            Self::TeamChat { sender, text, .. } => format!("[TEAM] {sender}: {text}"),
            Self::Frag { killer, victim, weapon } if killer == victim && weapon == "Suicide" => format!("{victim} took the quick way out"),
            Self::Frag { killer, victim, weapon } => format!("{killer} fragged {victim} · {weapon}"),
            Self::Play { text } => text.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_and_plain() {
        for text in ["", "  ", "hi\nthere", "\nhello", "hello\t", "hi\u{202e}there", &"x".repeat(161), &"界".repeat(81)] {
            assert!(message(text).is_none());
        }
        assert_eq!(message("  hello 世界  ").unwrap(), "hello 世界");
        let mut feed = Vec::new();
        for i in 0..40 { push(&mut feed, Entry::Chat { sender: "Pilot".into(), text: i.to_string() }); }
        assert_eq!(feed.len(), CAPACITY);
        assert!(feed[0].line().ends_with("24"));
    }
}
