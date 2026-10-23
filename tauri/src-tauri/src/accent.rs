//! Per-game accent colours: keeping them readable on Spool's dark surfaces.
//!
//! The accent is the most vibrant colour on a game's cover
//! (`steamgriddb::extract_vibrant_color`). HSL lightness doesn't track how
//! bright a colour looks — a saturated blue at 40% lightness is far darker to
//! the eye than a yellow at the same lightness — so a strongly blue or purple
//! cover could yield an accent that barely shows against the cards it tints.
//! [`readable_accent`] raises such a colour's lightness, keeping its hue and
//! saturation, until it meets [`MIN_CONTRAST`] against the card surface.
//!
//! New accents are adjusted when extracted; [`normalize_stored`] adjusts the
//! ones already in the library. Both the desktop app and the Decky plugin read
//! the stored value, so neither needs its own adjustment.

use crate::library::Library;

/// WCAG's minimum contrast for non-text UI (accent strips, icons, bars).
pub const MIN_CONTRAST: f32 = 3.0;

/// The card surface the accent is drawn on (`--color-bg-2` in `app.css`).
/// Accents are also drawn on the darker `--color-bg-1` and on Steam's own
/// page background, both of which give at least this much contrast.
const SURFACE: (u8, u8, u8) = (0x15, 0x18, 0x1d);

/// Standard RGB → HSL conversion. Returns (hue 0–360, sat 0–1, lum 0–1).
pub fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f32, f32, f32) {
    let r = r as f32 / 255.0;
    let g = g as f32 / 255.0;
    let b = b as f32 / 255.0;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d < 1e-6 {
        return (0.0, 0.0, l);
    }
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } * 60.0;
    (h, s, l)
}

/// Standard HSL → RGB conversion, the inverse of [`rgb_to_hsl`].
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (u8, u8, u8) {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h / 60.0).rem_euclid(6.0);
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to = |v: f32| ((v + m).clamp(0.0, 1.0) * 255.0).round() as u8;
    (to(r), to(g), to(b))
}

/// WCAG relative luminance of an sRGB colour.
fn luminance((r, g, b): (u8, u8, u8)) -> f32 {
    let lin = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.039_28 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
}

/// WCAG contrast ratio between two colours (1–21).
fn contrast(a: (u8, u8, u8), b: (u8, u8, u8)) -> f32 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
}

fn parse_hex(hex: &str) -> Option<(u8, u8, u8)> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(h, 16).ok()?;
    Some(((v >> 16) as u8, (v >> 8) as u8, v as u8))
}

/// `hex` (`#rrggbb`) with its lightness raised, in 1% steps, until it has at
/// least [`MIN_CONTRAST`] against the card surface; unchanged when it already
/// does. `None` when `hex` isn't a `#rrggbb` colour.
pub fn readable_accent(hex: &str) -> Option<String> {
    let rgb = parse_hex(hex)?;
    if contrast(rgb, SURFACE) >= MIN_CONTRAST {
        return Some(hex.to_ascii_lowercase());
    }
    let (h, s, l) = rgb_to_hsl(rgb.0, rgb.1, rgb.2);
    let mut out = rgb;
    let mut l = l;
    while contrast(out, SURFACE) < MIN_CONTRAST && l < 1.0 {
        l = (l + 0.01).min(1.0);
        out = hsl_to_rgb(h, s, l);
    }
    Some(format!("#{:02x}{:02x}{:02x}", out.0, out.1, out.2))
}

/// Rewrites stored accents that are below [`MIN_CONTRAST`] with their
/// [`readable_accent`]. Each write only applies if the accent is still the one
/// read, so a concurrent update isn't overwritten. Idempotent, so it runs at
/// every startup (tray GUI and headless server). Returns the number changed.
pub async fn normalize_stored(library: &Library) -> usize {
    let Ok(entries) = library.list().await else {
        return 0;
    };
    let mut changed = 0;
    for e in entries {
        let Some(current) = e.accent_color.as_deref() else {
            continue;
        };
        let Some(readable) = readable_accent(current) else {
            continue;
        };
        if readable == current.to_ascii_lowercase() {
            continue;
        }
        match library.replace_accent(&e.id, current, &readable).await {
            Ok(true) => changed += 1,
            Ok(false) => {}
            Err(err) => tracing::warn!(error = %err, "accent normalize: update failed"),
        }
    }
    if changed > 0 {
        tracing::info!(changed, "accent normalize: brightened low-contrast accents");
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(hex: &str) -> (u8, u8, u8) {
        parse_hex(hex).unwrap()
    }

    #[test]
    fn readable_accents_are_unchanged() {
        assert_eq!(readable_accent("#D7C9A0").as_deref(), Some("#d7c9a0"));
        assert_eq!(readable_accent("#d02000").as_deref(), Some("#d02000"));
    }

    #[test]
    fn dark_accents_are_raised_to_the_minimum_contrast() {
        for dark in ["#1000b8", "#685030", "#2a1060", "#003020"] {
            let out = readable_accent(dark).unwrap();
            assert!(
                contrast(rgb(&out), SURFACE) >= MIN_CONTRAST,
                "{dark} → {out} still below {MIN_CONTRAST}"
            );
        }
    }

    #[test]
    fn raising_keeps_the_hue() {
        let (h0, _, _) = rgb_to_hsl(0x10, 0x00, 0xb8);
        let out = rgb(&readable_accent("#1000b8").unwrap());
        let (h1, _, _) = rgb_to_hsl(out.0, out.1, out.2);
        assert!((h0 - h1).abs() < 3.0, "hue moved from {h0} to {h1}");
    }

    #[test]
    fn readable_accent_is_idempotent() {
        let once = readable_accent("#1000b8").unwrap();
        assert_eq!(readable_accent(&once).as_deref(), Some(once.as_str()));
    }

    #[test]
    fn non_hex_input_is_rejected() {
        assert_eq!(readable_accent("red"), None);
        assert_eq!(readable_accent("#fff"), None);
    }

    #[test]
    fn hsl_round_trips() {
        for c in [(0x10, 0x00, 0xb8), (0xd0, 0x20, 0x00), (0x68, 0x50, 0x30)] {
            let (h, s, l) = rgb_to_hsl(c.0, c.1, c.2);
            let back = hsl_to_rgb(h, s, l);
            for (a, b) in [(c.0, back.0), (c.1, back.1), (c.2, back.2)] {
                assert!(a.abs_diff(b) <= 1, "{c:?} → {back:?}");
            }
        }
    }
}
