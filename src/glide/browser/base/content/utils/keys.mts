// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/**
 * Handles turning keyboard events into Vim-style notation, setting mappings,
 * matching mappings and normalizing mappings.
 *
 * Interesting test cases can be found in `glide/browser/base/content/test/utils/browser_keys.js`.
 *
 * Terminology:
 *
 *   key notation (a.k.a keyn)
 *     Notation for an individual key, optionally including modifiers.
 *     e.g. `a`, `<C-a>`, `<lt>`, `<D-lt>`, `<D-C-a>`
 *
 *   key sequence (a.k.a keyseq)
 *     A sequence of key notations.
 *     e.g. `ab`, `<C-a>b`, `x<lt>`, `<leader>sf`
 */

const { is_present } = ChromeUtils.importESModule("chrome://glide/content/utils/guards.mjs");

// NOTE: the modal *state machine* (key matching + mode tracking) now lives in
// the Rust `glide-modal` crate, driven via `modal-engine.mts`. This module only
// retains the pure key-notation helpers (event <-> notation, normalisation,
// splitting, printability) used for converting DOM key events and for display.


// TODO(glide): make sure this is exhaustive
/**
 * Mapping of key codes from `KeyEvent` to vim notation identifiers.
 *
 * https://github.com/neovim/neovim/blob/3a25995f304039517b99b8c7d79654adf65c7562/src/nvim/keycodes.c#L145
 */
const SPECIAL_KEY_MAP = new Map(Object.entries({
  BS: "BS",
  Backspace: "BS",
  CR: "CR",
  Enter: "CR",
  Esc: "Esc",
  Escape: "Esc",
  " ": "Space",
  Space: "Space",
  Up: "Up",
  ArrowUp: "Up",
  Down: "Down",
  ArrowDown: "Down",
  Left: "Left",
  ArrowLeft: "Left",
  Right: "Right",
  ArrowRight: "Right",
  Tab: "Tab",
  Del: "Del",
  Delete: "Del",
  Home: "Home",
  End: "End",
  PageUp: "PageUp",
  PageDown: "PageDown",
  leader: "leader",
  F1: "F1",
  F2: "F2",
  F3: "F3",
  F4: "F4",
  F5: "F5",
  F6: "F6",
  F7: "F7",
  F8: "F8",
  F9: "F9",
  F10: "F10",
  F11: "F11",
  F12: "F12",

  // Note: see below `DOWNCAST_SPECIAL_KEY_MAP`, these characters are treated differently
  // from the rest of the ones defined above.
  "<": "lt",
  "|": "Bar",
  "\\": "Bslash",
}));
const REVERSE_SPECIAL_KEY_MAP = new Map(Array.from(SPECIAL_KEY_MAP.entries()).map(([k, v]) => [v, k]));

/**
 * A mapping of special keys that should be downcast to non-special keys in certain scenarios.
 *
 * For example, if just `|` is pressed with *no modifiers*, then we need to keep it as `|` but if there
 * are modifiers then we need to keep it as a special char, e.g. `<D-Bar>`
 */
const DOWNCAST_SPECIAL_KEY_MAP = new Map(Object.entries({ lt: "<", Bar: "|", Bslash: "\\" }));

/**
 * Characters that are inherently "shifted" on a US keyboard and should not
 * include the S modifier when they appear in key combinations as they
 * inherintly require the shift key to type.
 */
const SHIFTED_CHARACTERS = new Set([
  "!",
  "@",
  "#",
  "$",
  "%",
  "^",
  "&",
  "*",
  "(",
  ")",
  "_",
  "+",
  "{",
  "}",
  "|",
  ":",
  "\"",
  "<",
  ">",
  "?",
  "~",
  // transformed special keys
  "lt",
  "Bar",
  "Bslash",
]);

/**
 * A minimal version of `KeyboardEvent` that only defines the properties we rely on.
 */
export type GlideMappingEvent =
  & Pick<
    KeyboardEvent,
    "key" | "ctrlKey" | "metaKey" | "shiftKey" | "altKey"
  >
  & { code?: string };

/**
 * Given a keyboard event, returns the corresponding Vim-notation for it.
 *
 * e.g.
 * ```ts
 * event_to_key_notation({ key: 'h' }) -> 'h'
 * event_to_key_notation({ metaKey: true, key: 'b' }) -> '<D-b>'
 * event_to_key_notation({ ctrlKey: true, key: '<' }) -> '<C-lt>'
 * event_to_key_notation({ shiftKey: true, key: 'b' }) -> 'B'
 * event_to_key_notation({ shiftKey: true, ctrlKey: true, key: 'b' }) -> '<C-S-B>'
 * ```
 */
export function event_to_key_notation(event: GlideMappingEvent): string {
  const modifiers: string[] = [];

  if (event.ctrlKey) {
    modifiers.push("C");
  }

  if (event.altKey) {
    modifiers.push("A");
  }

  if (event.metaKey) {
    modifiers.push("D");
  }

  const key = resolve_event_key(event);
  const special_key = REVERSE_SPECIAL_KEY_MAP.get(key) ?? null;

  // For inherently shifted characters (like +, !, @, etc.), we don't add the S modifier
  // because the character itself already represents the shifted state
  const is_single_char = key.length === 1;
  const is_shifted_char = SHIFTED_CHARACTERS.has(key);
  if (
    event.shiftKey
    && (!is_single_char || modifiers.length)
    && !is_shifted_char
  ) {
    modifiers.push("S");
  }

  // To match vim behaviour, we need to map certain would-be-special-keys into
  // their non-special form, e.g. `<`, `|`, `\\`, but only if there are *no* modifiers
  // present. If there are modifiers then these must be kept as special chars, e.g. `<D-lt>`
  const downcast_key = DOWNCAST_SPECIAL_KEY_MAP.get(key);
  if (downcast_key && !modifiers.length) {
    return downcast_key;
  }

  if (modifiers.length) {
    // e.g. `<C-f>`, `<C-D-l>`
    return `<${modifiers.join("-")}-${key}>`;
  }

  if (is_present(special_key)) {
    // e.g. `<Space>`, `<CR>`
    return `<${key}>`;
  }

  return key;
}

function resolve_event_key(event: GlideMappingEvent): string {
  const glide = GlideBrowser.api;

  let key = event.key;

  if (
    event.code
    && (glide.options.get("keymaps_use_physical_layout") === "force"
      || (event.altKey && glide.ctx.os === "macosx"
        && glide.options.get("keymaps_use_physical_layout") === "for_macos_option_modifier"))
  ) {
    const layout = glide.options.get("keyboard_layouts")[glide.options.get("keyboard_layout")];
    const translation = layout[event.code as keyof typeof layout];
    if (translation) {
      key = event.shiftKey ? translation[1] : translation[0];
    }
  }

  const special_key = SPECIAL_KEY_MAP.get(key) ?? null;

  // Firefox handles the shift key differently under two circumstances:
  //
  // 1. If the keypress is *just* shift+c then firefox would set `key` to `C`
  // 2. If the keypress includes other modifiers, e.g. cmd+shift+c then firefox would set `key` to `c`
  //
  // So we just manually make sure the given key has always been uppercased if the shift flag is set.
  return special_key ?? (event.shiftKey
      // don't transform keys like `<Bslash>`
      && key.length === 1
    ? key.toLocaleUpperCase()
    : key);
}

/**
 * Given a key notation, normalise it including:
 * - transforming char aliases `<lt>` -> `<`
 * - wrap special keys with `<>`, e.g. `Space` -> `<Space>`
 * - transform shifted characters `<S-h>` -> `H`
 * - consistent ordering of modifiers `<D-C-A-h>` -> `<C-A-D-h>`
 *
 * Note: this function expects a single key notation, *not* a key sequence.
 */
export function normalize(keyn: string): string {
  const parsed = parse_modifiers(keyn);

  // case-insensitive normalizing of special keys, e.g.
  // `<space>` -> `<Space>`
  const lower_key = parsed.key.toLowerCase();
  for (const special_key of SPECIAL_KEY_MAP.values()) {
    if (lower_key === special_key.toLowerCase()) {
      parsed.key = special_key;
      break;
    }
  }

  // simulates the event that Firefox will send, as shifted key events are received
  // with `{ shiftKey: true, key: 'UPPER_CHAR'}`
  if (parsed.shiftKey && parsed.key.length === 1) {
    parsed.key = parsed.key.toLocaleUpperCase();
  }

  // For shifted characters, remove the S modifier if present so that we always normalise to
  // the same key notation, no matter if you provide `<C-S-+>` or `<C-+>`.
  if (
    parsed.shiftKey
    && SHIFTED_CHARACTERS.has(REVERSE_SPECIAL_KEY_MAP.get(parsed.key) || parsed.key)
  ) {
    parsed.shiftKey = false;
  }

  return event_to_key_notation(parsed);
}

/**
 * Take a single key, without modifiers or <>, and return the string identifier
 * that Firefox would send if the given key had been physically pressed.
 *
 * @example "Space" -> " "
 */
function keyn_to_event_repr(key: string): string {
  switch (key.toLowerCase()) {
    case "space":
      return " ";
    case "up":
    case "arrowup":
      return "ArrowUp";
    case "down":
    case "arrowdown":
      return "ArrowDown";
    case "left":
    case "arrowleft":
      return "ArrowLeft";
    case "right":
    case "arrowright":
      return "ArrowRight";
    case "bs":
      return "Backspace";
    case "cr":
      return "Enter";
    case "del":
      return "Delete";
    case "esc":
      return "Escape";
    default:
      return key;
  }
}

type GlideParsedMapping = Mutable<GlideMappingEvent> & { is_special: boolean };

/**
 * Returns exactly the modifiers and the key string that were present in the
 * given notation.
 *
 * ```ts
 * `parse_modifiers('H') -> {shiftKey: false, key: 'H'}`
 * `parse_modifiers('<S-H>') -> {shiftKey: true, key: 'H'}`
 * `parse_modifiers('<Space>') -> {key: ' '}`
 * ```
 */
export function parse_modifiers(
  keyn: string,
  { use_event_repr = true }: { use_event_repr?: boolean } = {},
): GlideParsedMapping {
  const parsed: GlideParsedMapping = {
    altKey: false,
    ctrlKey: false,
    metaKey: false,
    shiftKey: false,
    is_special: false,
    key: keyn,
  };

  if (!keyn.startsWith("<") || !keyn.endsWith(">")) {
    // no modifiers, or not valid notation
    return parsed;
  }

  // <C-S-h> -> C-S-h
  const stripped = keyn.slice(1, -1);
  // C-S-h -> [C,S,h]
  const parts = stripped.split("-");
  // [C,S,h] -> [C,S]
  const modifier_parts = parts.slice(0, -1);

  const raw_key = parts.at(-1)!;
  if (!raw_key) {
    throw new Error(`Invalid key string: ${keyn}`);
  }
  if (use_event_repr) {
    parsed.key = keyn_to_event_repr(raw_key);
  } else {
    parsed.key = to_special_key(raw_key) ?? raw_key;
  }

  if (to_special_key(raw_key)) {
    parsed.is_special = true;
  }

  for (const part of modifier_parts) {
    switch (part.toLowerCase()) {
      case "c": {
        parsed.ctrlKey = true;
        break;
      }
      case "a": {
        parsed.altKey = true;
        break;
      }
      case "d": {
        parsed.metaKey = true;
        break;
      }
      case "s": {
        parsed.shiftKey = true;
        break;
      }
      default: {
        throw new Error(`Unexpected modifier character ${part}, expected one of C, A, D, or S`);
      }
    }
  }

  return parsed;
}

/**
 * Given a string like `space` or `<sPace>`, returns the special key normalized version,
 * `<Space>`.
 */
function to_special_key(key: string): string | null {
  if (key.startsWith("<") && key.endsWith(">")) {
    key = key.slice(1, -1);
  }

  const lower_key = key.toLowerCase();
  for (const special_key of SPECIAL_KEY_MAP.values()) {
    if (lower_key === special_key.toLowerCase()) {
      return "<" + special_key + ">";
    }
  }

  return null;
}

/**
 * Split a string key sequence into an array of individual key notation entries.
 *
 * ```ts
 * split('abc') === ['a', 'b', 'c'];
 * split('<D-a>bc') === ['<D-a>', 'b', 'c'];
 * split('<D-lt>bc') === ['<D-lt>', 'b', 'c'];
 * ```
 */
export function split(seq: string): string[] {
  if (!seq.length) {
    return [];
  }

  const parts = [];

  let i = 0;

  while (true) {
    const char = seq[i];
    if (char == undefined) {
      break;
    }

    if (char !== "<") {
      // not a special char / doesn't use any modifiers, we don't need to do anything special
      parts.push(char);
      i++;
      continue;
    }

    const right_angle_bracket_index = seq.indexOf(">", i);
    if (
      right_angle_bracket_index === -1
      || right_angle_bracket_index === i + 1
    ) {
      // if there is no corresponding `>` **or** if the very next char is a `>`, e.g. `<>`
      // then we need to treat the `<` as an `<lt>` directly, instead of as part of the syntax
      // for special characters / modifiers
      parts.push(char);
      i++;
      continue;
    }

    var next_index = right_angle_bracket_index + 1;
    parts.push(seq.slice(i, next_index));
    i = next_index;
  }

  return parts;
}

/**
 * Returns whether or not a given key notation should
 * be "printable", i.e. whether or not it should be inserted
 * into text when editing.
 *
 * All non-special keys are printable and certain special keys
 * are printable, e.g. `<Enter>`, `<Tab>`
 */
export function is_printable(keyn: string): boolean {
  if (!keyn.startsWith("<")) {
    // all non-special keys are treated as printable
    return true;
  }

  return !(
    keyn === "<Esc>"
    || keyn === "<BS>"
    || keyn === "<Up>"
    || keyn === "<Down>"
    || keyn === "<Left>"
    || keyn === "<Right>"
    || keyn === "<Del>"
    || keyn === "<Home>"
    || keyn === "<End>"
    || keyn.startsWith("<F") // fn keys
  );
}
