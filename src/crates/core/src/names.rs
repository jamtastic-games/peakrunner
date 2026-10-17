//! Shared policy; the authoritative server always validates independently.
pub const MAX_LEN: usize = 24;
pub const HELP: &str = "1–24 letters (A–Z), numbers or spaces";

pub fn validate(raw: &str) -> Option<&str> {
    // Check before trimming: tabs/newlines and oversized inputs are not accepted.
    if raw.len() > MAX_LEN || !raw.bytes().all(|b| b.is_ascii_alphanumeric() || b == b' ') {
        return None;
    }
    let name = raw.trim_matches(' ');
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    #[test]
    fn names_are_bounded_ascii_and_not_empty() {
        assert_eq!(super::validate("  Pilot 42  "), Some("Pilot 42"));
        assert!(super::validate(&"a".repeat(24)).is_some());
        for s in ["", "   ", "A\n", "A\tB", "<script>", "A_B", "é", "A\u{202e}B", "A\0"] {
            assert!(super::validate(s).is_none(), "{s:?}");
        }
        assert!(super::validate(&"a".repeat(25)).is_none());
    }
}
