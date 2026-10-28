// SPDX-License-Identifier: Apache-2.0
//! Safe text rendering helpers for untrusted metadata and values.

/// Escape control, bidi and zero-width characters and cap the result.
///
/// The preview pane renders this as plain text. Escaping keeps a file-provided
/// name from injecting line breaks or direction overrides into the UI.
pub fn escape_text(input: &str, max_chars: usize) -> String {
    let mut out = String::new();
    let mut used = 0usize;
    for ch in input.chars() {
        if used >= max_chars {
            out.push_str("...");
            break;
        }
        used += 1;
        match ch {
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\\' => out.push_str("\\\\"),
            c if c.is_control() => out.push_str(&format!("\\u{{{:04X}}}", c as u32)),
            '\u{202A}'..='\u{202E}'
            | '\u{2066}'..='\u{2069}'
            | '\u{200B}'..='\u{200F}'
            | '\u{FEFF}' => out.push_str(&format!("\\u{{{:04X}}}", ch as u32)),
            c => out.push(c),
        }
    }
    out
}

/// Hex preview of a binary value, bounded by byte count.
pub fn hex_preview(bytes: &[u8], max_bytes: usize) -> String {
    let take = bytes.len().min(max_bytes);
    let mut out = String::with_capacity(take * 2 + 16);
    for (index, byte) in bytes[..take].iter().enumerate() {
        if index > 0 && index % 2 == 0 {
            out.push(' ');
        }
        out.push_str(&format!("{byte:02x}"));
    }
    if bytes.len() > take {
        out.push_str(" ...");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_line_breaks_and_controls() {
        assert_eq!(escape_text("a\nb\tc\u{1}", 20), "a\\nb\\tc\\u{0001}");
    }

    #[test]
    fn escapes_bidi_controls() {
        assert_eq!(escape_text("a\u{202E}b", 20), "a\\u{202E}b");
    }

    #[test]
    fn caps_characters() {
        assert_eq!(escape_text("abcdef", 3), "abc...");
    }

    #[test]
    fn keeps_unicode_intact() {
        assert_eq!(escape_text("日本語", 10), "日本語");
    }

    #[test]
    fn hex_preview_is_bounded() {
        assert_eq!(hex_preview(&[0xde, 0xad, 0xbe, 0xef], 10), "dead beef");
        assert_eq!(hex_preview(&[0xde, 0xad, 0xbe, 0xef], 2), "dead ...");
    }
}
