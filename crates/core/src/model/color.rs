//! Color parsing and resolution.
//!
//! A color value is either a CSS hex color (`#rgb` / `#rrggbb`,
//! case-insensitive) or a name from the built-in palette. Parsing
//! produces the *canonical authoring form*: hex is lowercased and
//! `#rgb` is expanded to `#rrggbb`; palette names are kept as names
//! (lowercased). The name is only an authoring form — before any
//! computation (filtering, rendering, comparison) a canonical value is
//! resolved to its hex via [`resolve_color_to_hex`], so a palette tweak
//! reaches every item that stores the name.
//!
//! The palette is hardcoded on purpose: `color` is a built-in type, so
//! it ships with a built-in palette (no schema configuration in v1).

/// The built-in named palette: `(name, pinned hex)` pairs.
///
/// The hex values are pinned here and nowhere else — the UI receives
/// them through the schema payload rather than keeping its own copy.
pub const COLOR_PALETTE: [(&str, &str); 8] = [
    ("red", "#ef4444"),
    ("orange", "#f97316"),
    ("yellow", "#eab308"),
    ("green", "#22c55e"),
    ("blue", "#3b82f6"),
    ("purple", "#a855f7"),
    ("pink", "#ec4899"),
    ("gray", "#6b7280"),
];

/// The palette names in declaration order, for error messages and the
/// `allowed` list of a `FieldValueError::InvalidColor`.
pub fn color_palette_names() -> Vec<String> {
    COLOR_PALETTE
        .iter()
        .map(|(name, _)| (*name).to_owned())
        .collect()
}

/// Errors produced when parsing a color string.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseColorError {
    /// The input was empty or whitespace only.
    #[error("color is empty")]
    Empty,

    /// A `#`-prefixed value that is not 3 or 6 hex digits.
    #[error("'{value}' is not a valid hex color (expected #rgb or #rrggbb)")]
    InvalidHex { value: String },

    /// A bare word that is not a palette name.
    #[error("'{value}' is not a palette color name")]
    UnknownName { value: String },
}

/// Parse a color string into its canonical authoring form.
///
/// - `#RGB` / `#RRGGBB` (any case) → lowercase `#rrggbb`
/// - a palette name (any case) → the lowercase name, kept as a name
///
/// Everything else is an error; the caller decides how to report it
/// (the coercion layer maps it to `FieldValueError::InvalidColor`).
pub fn parse_color(input: &str) -> Result<String, ParseColorError> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(ParseColorError::Empty);
    }

    let lowered = trimmed.to_ascii_lowercase();

    if let Some(digits) = lowered.strip_prefix('#') {
        if !digits.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(ParseColorError::InvalidHex {
                value: trimmed.to_owned(),
            });
        }
        return match digits.len() {
            3 => {
                let expanded: String = digits.chars().flat_map(|c| [c, c]).collect();
                Ok(format!("#{expanded}"))
            }
            6 => Ok(lowered),
            _ => Err(ParseColorError::InvalidHex {
                value: trimmed.to_owned(),
            }),
        };
    }

    if COLOR_PALETTE.iter().any(|(name, _)| *name == lowered) {
        Ok(lowered)
    } else {
        Err(ParseColorError::UnknownName {
            value: trimmed.to_owned(),
        })
    }
}

/// Resolve a canonical color value to its `#rrggbb` hex.
///
/// Hex values pass through unchanged; palette names look up their
/// pinned hex. Returns `None` for anything that is not a canonical
/// value produced by [`parse_color`].
pub fn resolve_color_to_hex(canonical: &str) -> Option<String> {
    if canonical.starts_with('#') {
        return Some(canonical.to_owned());
    }
    COLOR_PALETTE
        .iter()
        .find(|(name, _)| *name == canonical)
        .map(|(_, hex)| (*hex).to_owned())
}

// ── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_full_hex_is_lowercased() {
        assert_eq!(parse_color("#AABBCC").unwrap(), "#aabbcc");
        assert_eq!(parse_color("#aabbcc").unwrap(), "#aabbcc");
    }

    #[test]
    fn parse_short_hex_is_expanded() {
        assert_eq!(parse_color("#ABC").unwrap(), "#aabbcc");
        assert_eq!(parse_color("#f04").unwrap(), "#ff0044");
    }

    #[test]
    fn parse_palette_name_stays_a_name() {
        assert_eq!(parse_color("red").unwrap(), "red");
        assert_eq!(parse_color("RED").unwrap(), "red");
        assert_eq!(parse_color("  Blue ").unwrap(), "blue");
    }

    #[test]
    fn parse_unknown_name_rejected() {
        assert_eq!(
            parse_color("teal"),
            Err(ParseColorError::UnknownName {
                value: "teal".to_owned()
            })
        );
    }

    #[test]
    fn parse_bad_hex_rejected() {
        // Wrong lengths, non-hex digits, alpha channels — all rejected.
        for input in ["#ab", "#abcd", "#abcde", "#aabbccdd", "#ggg", "#12345g"] {
            assert!(
                matches!(parse_color(input), Err(ParseColorError::InvalidHex { .. })),
                "expected InvalidHex for {input}"
            );
        }
    }

    #[test]
    fn parse_empty_rejected() {
        assert_eq!(parse_color(""), Err(ParseColorError::Empty));
        assert_eq!(parse_color("   "), Err(ParseColorError::Empty));
    }

    #[test]
    fn resolve_name_to_pinned_hex() {
        assert_eq!(resolve_color_to_hex("red").unwrap(), "#ef4444");
        assert_eq!(resolve_color_to_hex("gray").unwrap(), "#6b7280");
    }

    #[test]
    fn resolve_hex_passes_through() {
        assert_eq!(resolve_color_to_hex("#aabbcc").unwrap(), "#aabbcc");
    }

    #[test]
    fn resolve_non_canonical_returns_none() {
        assert_eq!(resolve_color_to_hex("teal"), None);
    }

    #[test]
    fn every_palette_entry_is_canonical_and_resolves() {
        for (name, hex) in COLOR_PALETTE {
            assert_eq!(parse_color(name).unwrap(), name);
            assert_eq!(parse_color(hex).unwrap(), hex);
            assert_eq!(resolve_color_to_hex(name).unwrap(), hex);
        }
    }
}
