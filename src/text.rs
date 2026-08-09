// SPDX-License-Identifier: MIT OR Apache-2.0
//
// Which characters are the mark, and which are apparatus.
//
// A count is not a mark. Where a notation writes a byte count, a bit count or
// a family subscript riding its own glyph (`⧉₂`, `β₃₂`, `■140`), all of that
// is annotation ABOUT the notation rather than notation itself. So figures are
// not part of a mark for any of this engine's purposes: nothing traces the
// shape of a figure, a figure contributes nothing to the box a vine grows off,
// and a figure is ordinary text to be grown AROUND like any other.
//
// (Enclosed digits — `⓪`, `①`–`⑯` — are unaffected: those are not numerals
// riding a mark, they ARE marks, and they carry outlines of their own.)

/// Whether a character reads as a figure rather than as notation.
fn is_numeral(c: char) -> bool {
    c.is_ascii_digit()
        || matches!(c, '\u{00B9}' | '\u{00B2}' | '\u{00B3}')   // ¹ ² ³
        || ('\u{2070}'..='\u{2079}').contains(&c)               // ⁰–⁹
        || ('\u{2080}'..='\u{2089}').contains(&c)               // ₀–₉
}

/// JavaScript's `\s`: Unicode whitespace plus the zero-width no-break space.
fn is_space(c: char) -> bool {
    c.is_whitespace() || c == '\u{feff}'
}

/// How much of a seed's text is the mark itself: its leading run of characters
/// that are neither space nor figure, counted in UTF-16 code units so the
/// result can be used directly as a DOM text offset.
///
/// Composite marks keep all their parts (`¬⟨` is two characters, `|·|` three),
/// a mark wearing a count sheds it (`⧉₂` -> `⧉`), and a seed that is really a
/// CONTAINER for a mark — a citation line, the paragraph a drop cap opens —
/// yields just its opening token, which is the only part of it that is
/// notation at all.
///
/// A caller that knows better than this rule can say so with `override_len`,
/// and one does: a CSS `::first-letter` drop cap makes the whole PARAGRAPH the
/// seed, and the mark is its first letter alone. An override past the end of
/// the text is clamped rather than trusted blind.
pub fn mark_lead_length(text: &str, override_len: Option<f64>) -> usize {
    let total: usize = text.chars().map(char::len_utf16).sum();
    if let Some(n) = override_len {
        if n.is_finite() {
            return (n.max(0.0) as usize).min(total);
        }
    }
    let mut units = 0;
    for c in text.chars() {
        if is_space(c) || is_numeral(c) {
            break;
        }
        units += c.len_utf16();
    }
    units
}

/// The leading whitespace of `text`, in UTF-16 code units — where a seed's
/// mark actually starts.
pub fn leading_space_len(text: &str) -> usize {
    text.chars().take_while(|&c| is_space(c)).map(char::len_utf16).sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_figure_riding_a_mark_is_not_part_of_it() {
        assert_eq!(mark_lead_length("⧉₂", None), 1, "a family subscript is apparatus");
        assert_eq!(mark_lead_length("β₃₂", None), 1);
        assert_eq!(mark_lead_length("■140", None), 1, "a height written after its mark is not part of it");
        assert_eq!(mark_lead_length("⌘", None), 1);
        assert_eq!(mark_lead_length("¬⟨", None), 2, "a composite mark keeps all of its parts");
        assert_eq!(mark_lead_length("|·|", None), 3);
        assert_eq!(mark_lead_length("⁶⁹", None), 0, "a bare count is no mark at all");
        assert_eq!(mark_lead_length("", None), 0);
    }

    #[test]
    fn a_container_seed_yields_only_its_opening_token() {
        assert_eq!(mark_lead_length("∅ 4a5e1e4b", None), 1);
        assert_eq!(mark_lead_length("Beauty sue to tap out a tired roof.", None), 6);
    }

    #[test]
    fn an_override_wins_and_is_clamped() {
        assert_eq!(mark_lead_length("Beauty sue to tap", Some(1.0)), 1);
        assert_eq!(mark_lead_length("Beauty", Some(99.0)), 6, "an override past the text is clamped");
        assert_eq!(mark_lead_length("Beauty", Some(-3.0)), 0);
        assert_eq!(mark_lead_length("⧉₂", None), 1, "no override falls back to the rule");
        assert_eq!(mark_lead_length("Beauty", Some(f64::NAN)), 6, "a nonsense override falls back too");
    }

    #[test]
    fn lengths_are_utf16_units_so_they_can_index_host_text() {
        // An astral character is two code units in the host's own string.
        assert_eq!(mark_lead_length("𝔅", None), 2);
        assert_eq!(leading_space_len("  ⌘"), 2);
        assert_eq!(leading_space_len("⌘"), 0);
    }
}
