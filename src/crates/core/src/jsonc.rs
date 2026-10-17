//! JSON with comments, for the files people write by hand: mods'
//! definitions may explain themselves with `// line` and `/* block */`
//! comments, as old game scripts did. Comments become spaces (so error
//! line and column numbers still point at the right place); everything else,
//! strings included, is left exactly as written.

/// `text` without its comments.
pub fn strip(text: &str) -> String {
    let b = text.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    let mut in_string = false;
    while i < b.len() {
        let c = b[i];
        if in_string {
            out.push(c);
            if c == b'\\' && i + 1 < b.len() { out.push(b[i + 1]); i += 2; continue; }
            if c == b'"' { in_string = false; }
            i += 1;
        } else if c == b'"' {
            in_string = true;
            out.push(c);
            i += 1;
        } else if c == b'/' && b.get(i + 1) == Some(&b'/') {
            while i < b.len() && b[i] != b'\n' { out.push(b' '); i += 1; }
        } else if c == b'/' && b.get(i + 1) == Some(&b'*') {
            out.extend_from_slice(b"  ");
            i += 2;
            while i < b.len() && !(b[i] == b'*' && b.get(i + 1) == Some(&b'/')) {
                out.push(if b[i] == b'\n' { b'\n' } else { b' ' });
                i += 1;
            }
            if i < b.len() { out.extend_from_slice(b"  "); i += 2; }
        } else {
            out.push(c);
            i += 1;
        }
    }
    // Only ASCII bytes were replaced, so the text is still UTF-8.
    String::from_utf8(out).unwrap_or_default()
}

/// Parse JSON that may hold comments.
pub fn parse(text: &str) -> serde_json::Result<serde_json::Value> { serde_json::from_str(&strip(text)) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_go_and_strings_stay() {
        let text = "// head\n{ \"a\": \"// not a comment\", /* gone */ \"b\": \"/* kept */\", \"c\": \"q\\\"//x\" } // tail";
        let v = parse(text).unwrap();
        assert_eq!(v["a"], "// not a comment");
        assert_eq!(v["b"], "/* kept */");
        assert_eq!(v["c"], "q\"//x");
        let multi = "{\n/* one\ntwo */ \"x\": 1\n}";
        assert_eq!(strip(multi).lines().count(), multi.lines().count(), "line numbers kept");
        assert_eq!(parse(multi).unwrap()["x"], 1);
        assert!(parse("{ \"x\": 1 /* never closed").is_err());
    }
}
