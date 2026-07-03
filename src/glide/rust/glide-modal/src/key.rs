// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Key-notation conversion ported from `utils/keys.mts`.
//!
//! The single public entry point is [`key_notation_from_event`], which mirrors
//! `event_to_key_notation` from the TypeScript layer.  Keeping this logic in
//! Rust means the JS side no longer needs to touch key-notation at all: it
//! hands the raw DOM event fields over the uniffi boundary and receives back a
//! ready-to-use notation string (or `None` for modifier-only / dead-key events).

use crate::actions::KeyEventInfo;

// ---------------------------------------------------------------------------
// Lookup helpers (replace the TS `Map` constants with `match` arms)
// ---------------------------------------------------------------------------

/// Maps a raw DOM `event.key` string to its canonical Vim-style notation name.
///
/// Mirrors `SPECIAL_KEY_MAP` in `utils/keys.mts`.  Returns `None` for keys
/// that are not in the map (i.e. ordinary printable characters).
fn special_key_lookup(key: &str) -> Option<&'static str> {
    Some(match key {
        "BS" | "Backspace" => "BS",
        "CR" | "Enter" => "CR",
        "Esc" | "Escape" => "Esc",
        " " | "Space" => "Space",
        "Up" | "ArrowUp" => "Up",
        "Down" | "ArrowDown" => "Down",
        "Left" | "ArrowLeft" => "Left",
        "Right" | "ArrowRight" => "Right",
        "Tab" => "Tab",
        "Del" | "Delete" => "Del",
        "Home" => "Home",
        "End" => "End",
        "PageUp" => "PageUp",
        "PageDown" => "PageDown",
        "leader" => "leader",
        "Insert" => "Insert",
        "F1" => "F1",
        "F2" => "F2",
        "F3" => "F3",
        "F4" => "F4",
        "F5" => "F5",
        "F6" => "F6",
        "F7" => "F7",
        "F8" => "F8",
        "F9" => "F9",
        "F10" => "F10",
        "F11" => "F11",
        "F12" => "F12",
        // These three are also in TS DOWNCAST_SPECIAL_KEY_MAP.
        "<" => "lt",
        "|" => "Bar",
        "\\" => "Bslash",
        _ => return None,
    })
}

/// Returns `true` when `key` is a canonical special-key name — i.e. it appears
/// as a *value* in `SPECIAL_KEY_MAP` and therefore needs to be wrapped in `<>`.
///
/// Mirrors the check `REVERSE_SPECIAL_KEY_MAP.get(key) != null` in the TS.
fn is_canonical_special_name(key: &str) -> bool {
    matches!(
        key,
        "BS" | "CR"
            | "Esc"
            | "Space"
            | "Up"
            | "Down"
            | "Left"
            | "Right"
            | "Tab"
            | "Del"
            | "Home"
            | "End"
            | "PageUp"
            | "PageDown"
            | "leader"
            | "Insert"
            | "F1"
            | "F2"
            | "F3"
            | "F4"
            | "F5"
            | "F6"
            | "F7"
            | "F8"
            | "F9"
            | "F10"
            | "F11"
            | "F12"
            | "lt"
            | "Bar"
            | "Bslash"
    )
}

/// Maps canonical names back to their literal character form when no modifiers
/// are present, mirroring `DOWNCAST_SPECIAL_KEY_MAP` in the TS.
///
/// e.g. `"lt"` → `"<"`, `"Bar"` → `"|"`, `"Bslash"` → `"\\"`.
fn downcast_special_key(key: &str) -> Option<&'static str> {
    match key {
        "lt" => Some("<"),
        "Bar" => Some("|"),
        "Bslash" => Some("\\"),
        _ => None,
    }
}

/// Returns `true` for characters that are inherently produced by holding Shift
/// on a US keyboard (so they must not also receive the `S` modifier prefix).
///
/// Mirrors `SHIFTED_CHARACTERS` in `utils/keys.mts`.
fn is_shifted_char(key: &str) -> bool {
    matches!(
        key,
        "!" | "@"
            | "#"
            | "$"
            | "%"
            | "^"
            | "&"
            | "*"
            | "("
            | ")"
            | "_"
            | "+"
            | "{"
            | "}"
            | "|"
            | ":"
            | "\""
            | "<"
            | ">"
            | "?"
            | "~"
            // canonical names for the three downcasted specials also count
            | "lt"
            | "Bar"
            | "Bslash"
    )
}

/// Returns `true` for raw DOM `event.key` values that represent a pure modifier
/// or dead key.  Pressing one of these alone produces no typeable character, so
/// [`key_notation_from_event`] returns `None` for them.
fn is_ignored_key(key: &str) -> bool {
    matches!(
        key,
        "Shift"
            | "Control"
            | "Alt"
            | "Meta"
            | "OS"
            | "Dead"
            | "CapsLock"
            | "NumLock"
            | "ScrollLock"
            | "AltGraph"
            | "Hyper"
            | "Super"
            | "Fn"
            | "FnLock"
            | "Symbol"
            | "SymbolLock"
    )
}

// ---------------------------------------------------------------------------
// Core conversion
// ---------------------------------------------------------------------------

/// Resolve the effective key string from a DOM key event, mirroring the TS
/// `resolve_event_key` helper in `utils/keys.mts`.
///
/// 1. Canonical name lookup via [`special_key_lookup`].
/// 2. For bare single characters with Shift held, uppercase the character to
///    match the canonical representation Firefox produces for a bare Shift+c.
///
/// NOTE: Physical keyboard-layout translation (driven by the TS
/// `keymaps_use_physical_layout` option) requires access to browser options
/// that are not available in Rust.  This path is intentionally omitted; the
/// TS layer can pre-translate `event.code` before calling
/// [`key_notation_from_event`] if needed in the future.
fn resolve_event_key(event: &KeyEventInfo) -> String {
    // If the raw event.key is in SPECIAL_KEY_MAP, return the canonical name.
    if let Some(canonical) = special_key_lookup(&event.key) {
        return canonical.to_string();
    }

    // Firefox lowercases `event.key` for multi-modifier events (Ctrl+Shift+c
    // arrives with key="c").  We uppercase single characters when Shift is
    // held to produce the same canonical form that a bare Shift+c would give.
    if event.shift && event.key.chars().count() == 1 {
        return event.key.to_uppercase();
    }

    event.key.clone()
}

/// Convert a DOM key event into Vim-style key notation.
///
/// This is the Rust port of `event_to_key_notation` from `utils/keys.mts`.
/// Returns `None` when the event represents a modifier-only keypress (`Shift`,
/// `Control`, …) or a dead key that Glide cannot handle.
///
/// # Examples (mirroring the TS doc-comments)
///
/// | event fields                           | result       |
/// |----------------------------------------|--------------|
/// | `key="h"`                              | `"h"`        |
/// | `meta=true, key="b"`                   | `"<D-b>"`    |
/// | `ctrl=true, key="<"`                   | `"<C-lt>"`   |
/// | `shift=true, key="b"` (Firefox: "B")   | `"B"`        |
/// | `ctrl=true, shift=true, key="b"→"B"`   | `"<C-S-B>"`  |
pub fn key_notation_from_event(event: &KeyEventInfo) -> Option<String> {
    // Pure modifier / dead keys produce no typeable notation.
    if is_ignored_key(&event.key) {
        return None;
    }

    // Build modifier prefix in the same order as the TS implementation:
    // C (ctrl) → A (alt) → D (meta/cmd) → S (shift, added conditionally below).
    let mut modifiers: Vec<&'static str> = Vec::new();
    if event.ctrl {
        modifiers.push("C");
    }
    if event.alt {
        modifiers.push("A");
    }
    if event.meta {
        // macOS uses "D" (from "super"/"Command") to match the TS convention.
        modifiers.push("D");
    }

    let key = resolve_event_key(event);

    // Add the S modifier only when:
    //   (a) Shift is held, AND
    //   (b) the key is not a plain single character with no other modifiers
    //       (bare Shift+a → "A", not "<S-A>"), AND
    //   (c) the character is not inherently shifted (e.g. "!" already implies Shift).
    let is_single_char = key.chars().count() == 1;
    let is_shifted = is_shifted_char(&key);
    if event.shift && (!is_single_char || !modifiers.is_empty()) && !is_shifted {
        modifiers.push("S");
    }

    // Downcast "lt" / "Bar" / "Bslash" back to their literal characters when
    // there are no modifiers — bare `<` stays `<`, bare `|` stays `|`.
    if let Some(downcast) = downcast_special_key(&key) {
        if modifiers.is_empty() {
            return Some(downcast.to_string());
        }
    }

    if !modifiers.is_empty() {
        // e.g. `<C-f>`, `<C-S-B>`, `<C-lt>`
        return Some(format!("<{}-{}>", modifiers.join("-"), key));
    }

    // Canonical special keys without modifiers are wrapped in `<>`.
    // e.g. `<Space>`, `<CR>`, `<BS>`, `<Up>`.
    if is_canonical_special_name(&key) {
        return Some(format!("<{}>", key));
    }

    // Plain character, no modifiers — return as-is.
    Some(key)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::KeyEventInfo;

    fn ev(key: &str) -> KeyEventInfo {
        KeyEventInfo { key: key.into(), code: "".into(), ctrl: false, alt: false, shift: false, meta: false }
    }

    fn ev_mods(key: &str, ctrl: bool, alt: bool, shift: bool, meta: bool) -> KeyEventInfo {
        KeyEventInfo { key: key.into(), code: "".into(), ctrl, alt, shift, meta }
    }

    #[test]
    fn plain_char_returns_itself() {
        assert_eq!(key_notation_from_event(&ev("h")), Some("h".into()));
    }

    #[test]
    fn shift_single_char_returns_uppercase() {
        // Firefox sends "b" for bare Shift+b in some contexts; resolve_event_key uppercases it.
        let e = ev_mods("b", false, false, true, false);
        assert_eq!(key_notation_from_event(&e), Some("B".into()));
    }

    #[test]
    fn ctrl_char_wraps_in_angle_brackets() {
        let e = ev_mods("a", true, false, false, false);
        assert_eq!(key_notation_from_event(&e), Some("<C-a>".into()));
    }

    #[test]
    fn meta_char_uses_d_prefix() {
        let e = ev_mods("b", false, false, false, true);
        assert_eq!(key_notation_from_event(&e), Some("<D-b>".into()));
    }

    #[test]
    fn ctrl_shift_char_produces_c_s_prefix() {
        // Firefox sends lowercase for Ctrl+Shift combos; we uppercase in resolve_event_key.
        let e = ev_mods("b", true, false, true, false);
        assert_eq!(key_notation_from_event(&e), Some("<C-S-B>".into()));
    }

    #[test]
    fn enter_becomes_cr() {
        assert_eq!(key_notation_from_event(&ev("Enter")), Some("<CR>".into()));
    }

    #[test]
    fn escape_becomes_esc() {
        assert_eq!(key_notation_from_event(&ev("Escape")), Some("<Esc>".into()));
    }

    #[test]
    fn backspace_becomes_bs() {
        assert_eq!(key_notation_from_event(&ev("Backspace")), Some("<BS>".into()));
    }

    #[test]
    fn space_becomes_space_notation() {
        assert_eq!(key_notation_from_event(&ev(" ")), Some("<Space>".into()));
    }

    #[test]
    fn arrow_up_becomes_up() {
        assert_eq!(key_notation_from_event(&ev("ArrowUp")), Some("<Up>".into()));
    }

    #[test]
    fn f5_becomes_f5_notation() {
        assert_eq!(key_notation_from_event(&ev("F5")), Some("<F5>".into()));
    }

    #[test]
    fn ctrl_lt_becomes_c_lt() {
        let e = ev_mods("<", true, false, false, false);
        assert_eq!(key_notation_from_event(&e), Some("<C-lt>".into()));
    }

    #[test]
    fn bare_lt_downcasts_to_literal() {
        // `<` with no modifiers should stay as `<` (downcast from "lt").
        assert_eq!(key_notation_from_event(&ev("<")), Some("<".into()));
    }

    #[test]
    fn bare_pipe_downcasts_to_literal() {
        assert_eq!(key_notation_from_event(&ev("|")), Some("|".into()));
    }

    #[test]
    fn modifier_key_returns_none() {
        assert_eq!(key_notation_from_event(&ev("Shift")), None);
        assert_eq!(key_notation_from_event(&ev("Control")), None);
        assert_eq!(key_notation_from_event(&ev("Alt")), None);
        assert_eq!(key_notation_from_event(&ev("Meta")), None);
    }

    #[test]
    fn dead_key_returns_none() {
        assert_eq!(key_notation_from_event(&ev("Dead")), None);
    }

    #[test]
    fn inherently_shifted_char_no_s_prefix() {
        // `!` is inherently shift-produced; no `<S-!>` even with Shift held.
        let e = ev_mods("!", false, false, true, false);
        assert_eq!(key_notation_from_event(&e), Some("!".into()));
    }

    #[test]
    fn ctrl_shifted_char_no_extra_s_prefix() {
        // `<C-!>` — shift is inherent, so the modifier string is just `C`.
        let e = ev_mods("!", true, false, true, false);
        assert_eq!(key_notation_from_event(&e), Some("<C-!>".into()));
    }
}
