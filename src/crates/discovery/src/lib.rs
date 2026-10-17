//! Discovery contracts and bounded networking helpers. No gameplay dependency.
pub mod lan;
pub mod http;
pub mod quic;
pub mod wire;
mod types;
pub use types::*;

/// Map labels in adverts are untrusted: accept map keys (`tower-complex`) and
/// display names (`Old Holler`), including maps this build doesn't know,
/// without a per-map list. ASCII letters, digits, single inner spaces and hyphens only.
/// A server's stated location ("New York, US"): empty, or up to 40 plain
/// characters (letters, digits, spaces, commas, periods, hyphens).
pub fn valid_location(text: &str) -> bool {
    text.is_empty() || (text.len() <= 40 && text.trim() == text && !text.contains("  ")
        && text.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b' ' | b',' | b'.' | b'-')))
}

pub fn valid_map_label(label: &str) -> bool {
    (1..=32).contains(&label.len())
        && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b' ' || b == b'-')
        && label.bytes().next().is_some_and(|b| b.is_ascii_alphanumeric())
        && label.bytes().last().is_some_and(|b| b.is_ascii_alphanumeric())
        && !label.contains("  ")
}

#[cfg(test)]
mod tests {
    use super::valid_map_label;
    #[test]
    fn map_labels_accept_keys_and_names_and_reject_hostile_input() {
        for ok in ["old-holler", "tower-complex", "cairnhold", "frostline",
            "dustreach", "ozarktic-blast", "valley", "Valley",
            "Old Holler", "Tower Complex", "Cairnhold", "Frostline", "Dustreach",
            "Ozarktic Blast", "Some Future Map",
            // Current servers add the mode.
            "Old Holler - CTF", "Tower Complex - CnH", "Longfield - Football"] {
            assert!(valid_map_label(ok), "{ok}");
        }
        for bad in ["", " ", " Old Holler", "Old Holler ", "Old  Holler", "<b>Valley</b>",
            "a&b", "tab\there", "line\nbreak", "bidi\u{202e}map", "zero\u{200b}width",
            "Holler\u{0}", "caf\u{e9}", &"x".repeat(33)] {
            assert!(!valid_map_label(bad), "{bad:?}");
        }
    }
}
