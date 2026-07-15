// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/**
 * The modal engine is the bridge between Glide's browser layer and the Rust
 * `glide-modal` crate (built on top of modalkit).
 *
 * Rust is the single authority for the modal *state machine* — mode tracking,
 * key-sequence matching, operator-pending composition and edit planning. The
 * TypeScript side here keeps a registry that maps matched sequences back to the
 * `glide.ExcmdValue` to execute (which may be a JS callback that Rust cannot
 * hold), and exposes the same surface that `browser.mts` / `GlideDocsParent`
 * previously consumed from the TypeScript `KeyManager`.
 *
 * The legacy `KeyManager` trie state machine has been removed; only the pure
 * key-notation helpers in `utils/keys.mts` remain.
 */

import type * as RustGlideModalT from "../../../generated/@types/RustGlideModal.d.ts";

const Keys = ChromeUtils.importESModule("chrome://glide/content/utils/keys.mjs", { global: "current" });
const { BUILTIN_GLIDE_MODES, BUILTIN_MODE_CARET_CONFIG } = ChromeUtils.importESModule(
  "chrome://glide/content/mode-config.mjs",
);
const RustGlideModal = ChromeUtils.importESModule(
  "moz-src:///toolkit/components/uniffi-bindgen-gecko-js/components/generated/RustGlideModal.sys.mjs",
);
const { AppConstants } = ChromeUtils.importESModule("resource://gre/modules/AppConstants.sys.mjs");

export { BUILTIN_GLIDE_MODES, BUILTIN_MODE_CARET_CONFIG };

/**
 * Only built-in modes have a Rust representation. Custom modes (registered via
 * `glide.modes.register`) are resolved entirely JS-side and never reach
 * {@link rust_mode}.
 */
type BuiltinGlideMode =
  | "normal"
  | "insert"
  | "visual"
  | "op-pending"
  | "ignore"
  | "command"
  | "hint";

const MODE_TO_RUST: Record<BuiltinGlideMode, RustGlideModalT.GlideMode> = {
  normal: RustGlideModal.GlideMode.Normal,
  insert: RustGlideModal.GlideMode.Insert,
  visual: RustGlideModal.GlideMode.Visual,
  "op-pending": RustGlideModal.GlideMode.OperatorPending,
  ignore: RustGlideModal.GlideMode.Ignore,
  command: RustGlideModal.GlideMode.Command,
  hint: RustGlideModal.GlideMode.Hint,
};

function rust_mode(mode: GlideMode): RustGlideModalT.GlideMode {
  const rust = (MODE_TO_RUST as Record<string, RustGlideModalT.GlideMode | undefined>)[mode];
  if (rust === undefined) {
    throw new Error(`internal error: mode \`${mode}\` has no Rust representation`);
  }
  return rust;
}

/**
 * Caret style enum values, mirroring Rust `GlideMode::caret_style` and
 * `src/glide/cpp/Glide.h::GlideCaretStyle`. Used for custom (JS-registered)
 * modes that the Rust engine cannot enumerate.
 */
const CARET_STYLE_TO_ENUM: Record<"block" | "underline" | "line", number> = {
  block: 0,
  underline: 1,
  line: 2,
};

function is_builtin_mode(mode: GlideMode): boolean {
  return (BUILTIN_GLIDE_MODES as readonly string[]).includes(mode);
}

/**
 * Plain, structured-clone-safe representation of a typed editing action, sent
 * from the parent to the content process alongside `Glide::ExecuteContentCommand`.
 *
 * Rust uniffi objects can't cross the IPC boundary, so `modal-engine.mts`
 * converts the `EditingActionIntent` into this shape before forwarding.
 *
 * Stage A only supports motions and the `Motion`/`Delete`/`Change`/`Yank`
 * operations; text objects (`EditTarget::Range`/`Boundary`) remain on the
 * `RawDescription` fallback until Stage B.
 */
/**
 * One primitive step of an insert-entry / dot-repeated insert session
 * (`o`/`O`/`i`/`a`/`A`/`I` and their `.` replays). The content executor applies
 * these in order, then settles into the final mode.
 */
export type GlideInsertOp =
  | { kind: "open_line"; above: boolean }
  | { kind: "automove"; direction: "left" | "endline" }
  | { kind: "insert_text"; text: string }
  | { kind: "move"; target: GlideEditingAction["target"] };

export interface GlideEditingAction {
  operation: "motion" | "delete" | "change" | "yank" | "replace" | "insert" | "raw";
  /** Replacement character for the `replace` operation (`r{char}`). */
  character?: string;
  /** Ordered steps for the `insert` operation (insert-entry / `.` replay). */
  ops?: GlideInsertOp[];
  /** For the `insert` operation: whether to end in insert mode (live entry) or normal (`.` replay). */
  entersInsert?: boolean;
  target: {
    kind: "current-position" | "current-selection" | "line-range" | "motion" | "range" | "raw";
    motion?:
      | "column"
      | "line-start"
      | "line-end"
      | "first-word"
      | "line"
      | "word-begin"
      | "word-end"
      | "paragraph-begin"
      | "raw";
    range?:
      | "word"
      | "bracketed"
      | "quote"
      | "xml-tag"
      | "paragraph"
      | "sentence"
      | "line"
      | "buffer"
      | "item";
    count: number;
    direction?: "previous" | "next";
    wordStyle?: "little" | "big" | "keyword" | "non-alphanumeric";
    wrap?: boolean;
    includeLineBreak?: boolean;
    inclusive?: boolean;
    /** Single-character string for `bracketed.left` / `bracketed.right` / `quote.quote`. */
    left?: string;
    right?: string;
    quote?: string;
    description?: string;
  };
}

/** A fully-resolved instruction returned by {@link GlideModalEngine.process_key}. */
export type ProcessedInstruction =
  | { kind: "excmd"; command: string; editing_action?: GlideEditingAction }
  | { kind: "callback"; cb: glide.ExcmdValue; sequence: string[] }
  | { kind: "mode-change" };

/** Fully-translated result of {@link GlideModalEngine.process_key}. */
export interface ProcessedKeyDisposition {
  preventDefault: boolean;
  sequenceDisplay: string[];
  /** Next mode as a TS string, or null if no transition. */
  modeTransition: GlideMode | null;
  matchedMapping: boolean;
  hasPartialMatch: boolean;
  instructions: ProcessedInstruction[];
}

interface RegistryEntry {
  mode: GlideMode;
  sequence: string[];
  lhs: string;
  rhs: glide.ExcmdValue;
  description: string | undefined;
  retain_key_display: boolean;
  buffer: boolean;
  deleted: boolean;
  /** Set when `rhs` is a JS callback registered for a built-in mode. */
  callback_id?: number;
}

function registry_key(mode: GlideMode, sequence: string[]): string {
  return `${mode}\u0000${sequence.join("\u0001")}`;
}

interface ParsedModeChange {
  target: GlideMode;
  automove: "left" | "endline" | null;
}


function parse_mode_change(excmd: string): ParsedModeChange | null {
  const match = /^mode_change\s+(\S+)(.*)$/.exec(excmd.trim());
  if (!match) {
    return null;
  }

  const rest = match[2] ?? "";
  const automove_raw = /--automove=(\S+)/.exec(rest)?.[1] ?? null;

  return {
    target: match[1] as GlideMode,
    automove: automove_raw === "left" || automove_raw === "endline" ? automove_raw : null,
  };
}

export class GlideModalEngine {
  #bridge: RustGlideModalT.GlideModalBridge;

  /**
   * Mirror of the mappings registered with the Rust engine, keyed by mode +
   * sequence. Holds the original `glide.ExcmdValue` (which may be a JS callback)
   * and metadata for introspection (`list`, docs).
   */
  #registry: Map<string, RegistryEntry> = new Map();

  /**
   * Names of custom modes registered via `glide.modes.register`.  The caret
   * style and key-resolution authority now live in the Rust engine; this Set
   * only tracks which mode names have been registered so we can guard against
   * duplicate registration and enumerate modes in `mode_names`.
   */
  #custom_mode_names: Set<GlideMode> = new Set();

  /** Monotonically-increasing counter for allocating JS-closure callback IDs. */
  #next_callback_id: number = 0;

  /**
   * Maps callback IDs (allocated in {@link GlideModalEngine.set}) to the
   * original `glide.ExcmdValue` closures. When Rust matches a sequence whose
   * command is `EngineCommand::Callback`, it returns the id in
   * `Instruction::Callback.callbackId` and `process_key` looks it up here.
   */
  #callback_map: Map<number, glide.ExcmdValue> = new Map();

  constructor() {
    this.#bridge = new RustGlideModal.GlideModalBridge();
    this.#register_builtin_keymaps();
  }

  // ----- sequence / mode state -------------------------------------------

  get current_sequence(): string[] {
    return this.#bridge.currentSequence();
  }

  get has_partial_mapping(): boolean {
    return this.#bridge.currentSequence().length !== 0;
  }

  reset_sequence() {
    this.#bridge.resetSequence();
  }

  clear_buffer() {
    for (const [key, entry] of [...this.#registry]) {
      if (entry.buffer) {
        if (entry.callback_id !== undefined) {
          this.#callback_map.delete(entry.callback_id);
        }
        this.#registry.delete(key);
      }
    }
    this.#bridge.clearBuffer();
  }

  get mode_names(): GlideMode[] {
    return [...BUILTIN_GLIDE_MODES, ...this.#custom_mode_names];
  }

  get current_mode(): GlideMode {
    // Rust is the single authority: custom mode name (if any) takes precedence,
    // otherwise the built-in mode name comes from the Rust engine.
    return (this.#bridge.currentCustomMode() ?? this.#bridge.currentModeName()) as GlideMode;
  }

  /**
   * Register a custom mode. Mirrors the historical `glide.modes.register`:
   * a mode can only be registered once (built-in or custom).
   */
  register_mode(mode: GlideMode, opts: { caret: "block" | "underline" | "line" }) {
    if (is_builtin_mode(mode) || this.#custom_mode_names.has(mode)) {
      throw new Error(
        `The \`${mode}\` mode has already been registered. Modes can only be registered once`,
      );
    }
    this.#custom_mode_names.add(mode);
    // Register with Rust so it can resolve keys in this mode and report the
    // caret style via `customModeCaretStyle`.
    this.#bridge.registerCustomMode(mode, CARET_STYLE_TO_ENUM[opts.caret]);
  }

  /**
   * Keep the engine authoritative when the mode changes outside the key
   * pipeline (command bar, hints, sandbox `glide.ctx.mode = …`, custom modes).
   */
  set_mode(mode: GlideMode) {
    this.#bridge.resetSequence();
    if (this.#custom_mode_names.has(mode)) {
      // Delegate to Rust: it parks itself in Normal internally and records the
      // custom mode name so `resolveKeyNotation` routes keys correctly.
      this.#bridge.setCustomMode(mode);
      return;
    }

    this.#bridge.setMode(rust_mode(mode));
  }

  mode_to_style_enum(mode: GlideMode): number {
    if (!is_builtin_mode(mode)) {
      // Rust owns the caret style for custom modes; fall back to `block` (0)
      // if the mode was somehow never registered.
      return this.#bridge.customModeCaretStyle(mode) ?? 0;
    }
    return this.#bridge.modeCaretStyle(rust_mode(mode));
  }

  // ----- mapping management ----------------------------------------------

  set(
    modes: GlideMode | GlideMode[],
    lhs: string,
    rhs: glide.ExcmdValue,
    opts?: glide.KeymapOpts,
  ) {
    const sequence = Keys.split(lhs).map((keyn: string) => this.#resolve_leader(Keys.normalize(keyn)));
    const buffer = opts?.buffer ?? false;
    const retain_key_display = opts?.retain_key_display ?? false;

    for (const mode of typeof modes === "string" ? [modes] : modes) {
      // Allocate an opaque callback ID for any non-string rhs (both built-in
      // and custom modes), so Rust can identify which closure to dispatch via
      // `ResolvedKeyResult.matchedCallbackId` without a separate registry scan.
      let callback_id: number | undefined;
      if (typeof rhs !== "string") {
        // If a mapping already exists with a callback ID, clean up the old entry
        // to avoid leaking closures in #callback_map.
        const existing = this.#registry.get(registry_key(mode, sequence));
        if (existing?.callback_id !== undefined) {
          this.#callback_map.delete(existing.callback_id);
        }
        callback_id = this.#next_callback_id++;
        this.#callback_map.set(callback_id, rhs);
      }

      this.#registry.set(registry_key(mode, sequence), {
        mode,
        sequence,
        lhs,
        rhs,
        description: opts?.description,
        retain_key_display,
        buffer,
        deleted: false,
        callback_id,
      });

      if (this.#custom_mode_names.has(mode)) {
        // Custom-mode keymaps are now resolved by the Rust engine (via
        // `setCustomKeymap` / `resolveKeyNotation`).  The `mode` field in
        // `KeymapDefinition` is a sentinel (Normal); the actual mode name is
        // carried in `customMode`.
        this.#bridge.setCustomKeymap(
          new RustGlideModal.KeymapDefinition({
            mode: RustGlideModal.GlideMode.Normal, // sentinel, ignored by Rust
            sequence,
            command: this.#to_engine_command(rhs, callback_id),
            retainKeyDisplay: retain_key_display,
            buffer,
            description: opts?.description ?? null,
            customMode: mode,
          }),
        );
        continue;
      }

      this.#bridge.setKeymap(
        new RustGlideModal.KeymapDefinition({
          mode: rust_mode(mode),
          sequence,
          command: this.#to_engine_command(rhs, callback_id),
          retainKeyDisplay: retain_key_display,
          buffer,
          description: opts?.description ?? null,
        }),
      );
    }
  }

  del(
    modes: GlideMode | GlideMode[],
    lhs: string,
    opts?: glide.KeymapDeleteOpts,
  ) {
    const sequence = Keys.split(lhs).map((keyn: string) => this.#resolve_leader(Keys.normalize(keyn)));
    const buffer = opts?.buffer ?? false;

    for (const mode of typeof modes === "string" ? [modes] : modes) {
      const entry = this.#registry.get(registry_key(mode, sequence));
      if (entry?.callback_id !== undefined) {
        this.#callback_map.delete(entry.callback_id);
      }
      this.#registry.delete(registry_key(mode, sequence));
      if (this.#custom_mode_names.has(mode)) {
        this.#bridge.delCustomKeymap(mode, sequence);
        continue;
      }
      this.#bridge.delKeymap(rust_mode(mode), sequence, buffer);
    }
  }

  list(modes?: GlideMode | GlideMode[]): glide.Keymap[] {
    const output: glide.Keymap[] = [];

    for (const entry of this.#registry.values()) {
      if (entry.deleted) {
        continue;
      }
      if (typeof modes === "string" && entry.mode !== modes) {
        continue;
      }
      if (Array.isArray(modes) && !modes.includes(entry.mode)) {
        continue;
      }

      output.push({
        sequence: entry.sequence,
        lhs: entry.sequence.join(""),
        description: entry.description,
        rhs: entry.rhs,
        mode: entry.mode,
      });
    }

    return output;
  }

  // ----- key resolution --------------------------------------------------

  /** Process a raw key event through the Rust engine and return a fully-resolved
   *  {@link ProcessedKeyDisposition}. Returns `null` for modifier-only / dead keys. */
  process_key(event: KeyboardEvent): ProcessedKeyDisposition | null {
    const disp = this.#bridge.processKey({
      key: event.key,
      code: event.code,
      ctrl: event.ctrlKey,
      alt: event.altKey,
      shift: event.shiftKey,
      meta: event.metaKey,
    });
    if (!disp) return null;

    return {
      preventDefault: disp.preventDefault,
      sequenceDisplay: disp.sequenceDisplay,
      modeTransition: disp.modeTransition ? this.#rust_mode_name(disp.modeTransition.nextMode) : null,
      matchedMapping: disp.matchedMapping,
      hasPartialMatch: disp.hasPartialMatch,
      instructions: disp.matchedMapping ? this.#translate_instructions(disp) : [],
    };
  }

  /**
   * Map the engine's flat {@link RustGlideModalT.Instruction} records onto the
   * browser-facing {@link ProcessedInstruction}s. Rust has already made every
   * decision (operator resolved incl. `change` recovery, command bar expressed
   * as its excmd), so this is a near-passthrough `switch` — the only JS-side
   * step is resolving a callback id back to its closure.
   */
  #translate_instructions(disp: RustGlideModalT.KeyDisposition): ProcessedInstruction[] {
    // A pure vim mode change (i/v/<Esc>) emits no instructions, just a transition.
    if (disp.instructions.length === 0 && disp.modeTransition) {
      return [{ kind: "mode-change" }];
    }

    const out: ProcessedInstruction[] = [];
    for (const instr of disp.instructions) {
      switch (instr.kind) {
        case "callback": {
          const cb = this.#callback_map.get(Number(instr.callbackId));
          if (cb != null) out.push({ kind: "callback", cb, sequence: instr.sequence });
          break;
        }
        case "editing-action": {
          const action = instr.action!;
          out.push({
            kind: "excmd",
            command: "motion",
            editing_action: {
              operation: action.operation as GlideEditingAction["operation"],
              character: action.character ?? undefined,
              target: action.target as GlideEditingAction["target"],
            },
          });
          break;
        }
        case "insert-sequence":
          out.push({
            kind: "excmd",
            command: "motion",
            editing_action: {
              operation: "insert",
              // InsertOp records are already in `GlideInsertOp` shape.
              ops: instr.insertOps as unknown as GlideInsertOp[],
              entersInsert: instr.entersInsert,
              target: { kind: "current-position", count: 0 },
            },
          });
          break;
        case "excmd":
          out.push({
            kind: "excmd",
            command: instr.arguments.length ? `${instr.command} ${instr.arguments.join(" ")}` : instr.command!,
          });
          break;
      }
    }
    return out;
  }

  // ----- Phase 6: excmd registry & dot-repeat ----------------------------

  /**
   * Return the full built-in excmd registry from Rust.
   *
   * Each entry has `name`, `description`, `contentFlag`, and `repeatable`.
   * Useful for which-key display and command-line completion.
   */
  excmd_registry(): RustGlideModalT.ExcmdInfo[] {
    return this.#bridge.excmdRegistry();
  }

  /**
   * Parse `input` (e.g. `"tab_next"` or `"mode_change normal"`) into a
   * `ParsedExcmd` containing the command name and positional arguments.
   *
   * Throws when the command name is not in the built-in registry.
   * User-defined excmds are not validated here.
   */
  parse_excmd(input: string): RustGlideModalT.ParsedExcmd {
    return this.#bridge.parseExcmd(input);
  }

  /**
   * Notify Rust that a repeatable excmd was dispatched so it can be returned
   * by {@link repeat_last} for the next dot-repeat invocation.
   *
   * Non-repeatable commands are silently ignored.
   */
  note_executed(parsed: RustGlideModalT.ParsedExcmd): void {
    this.#bridge.noteExecuted(parsed);
  }

  /**
   * Return the last repeatable excmd recorded by {@link note_executed}, or
   * `null` when no repeatable excmd has been dispatched yet.
   */
  repeat_last(): RustGlideModalT.ParsedExcmd | null {
    return this.#bridge.repeatLast();
  }

  // ----- internals -------------------------------------------------------

  #resolve_leader(keyn: string): string {
    if (keyn !== "<leader>") {
      return keyn;
    }

    const mapleader = GlideBrowser?.api?.g?.mapleader;
    return mapleader ? Keys.normalize(mapleader) : keyn;
  }

  #to_engine_command(
    rhs: glide.ExcmdValue,
    callback_id?: number,
  ): RustGlideModalT.EngineCommand {
    if (typeof rhs === "string") {
      // `.` (`repeat`) is dispatched like any other excmd; the actual replay is
      // handled JS-side by the `repeat` excmd against `#last_command` (modalkit
      // can't see custom/async excmds like `r`, so it can't own dot-repeat).
      const mode_change = parse_mode_change(rhs);
      if (mode_change) {
        return new RustGlideModal.EngineCommand.ChangeMode({
          request: new RustGlideModal.ModeChangeRequest({
            targetMode: rust_mode(mode_change.target),
            automaticMoveDirection: mode_change.automove === "left"
              ? RustGlideModal.AutomaticMoveDirection.Left
              : mode_change.automove === "endline"
              ? RustGlideModal.AutomaticMoveDirection.EndOfLine
              : null,
          }),
        });
      }

      return new RustGlideModal.EngineCommand.DispatchBrowserCommand({
        commandName: rhs,
        arguments: [],
      });
    }

    // Non-string excmd values (JS callbacks / structured excmds) can't be held
    // by Rust, so the callback_id allocated in `set()` is stored with the
    // mapping. Rust returns it in `Instruction::Callback.callbackId` after a
    // match, and `process_key` looks it up in `#callback_map`.
    return new RustGlideModal.EngineCommand.Callback({
      callbackId: BigInt(callback_id!),
    });
  }

  #rust_mode_name(mode: RustGlideModalT.GlideMode): GlideMode {
    for (const name of BUILTIN_GLIDE_MODES) {
      if (rust_mode(name) === mode) {
        return name;
      }
    }
    return "normal";
  }

  #register_builtin_keymaps() {
    // Builtins are registered through the same `set` path as user mappings, so
    // the Rust engine owns matching/mode for them too. `mode_change …` excmds
    // become typed mode transitions in Rust; everything else dispatches the
    // excmd back through the browser layer.
    this.set("normal", "<leader>r", "reload");
    this.set("normal", "<leader>R", "reload_hard");
    this.set("normal", ":", "commandline_show");
    this.set("normal", "<leader><leader>", "commandline_show tab ");
    this.set("normal", "gg", "scroll_top");
    this.set("normal", "G", "scroll_bottom");
    this.set(["insert", "normal"], "<C-d>", "scroll_half_page_down");
    this.set(["insert", "normal"], "<C-u>", "scroll_half_page_up");

    this.set(["normal", "insert", "visual"], "<S-Esc>", "mode_change ignore");
    this.set("ignore", "<S-Esc>", "mode_change normal");

    if (AppConstants.platform === "macosx") {
      this.set(["normal", "insert"], "<C-h>", "back");
      this.set(["normal", "insert"], "<C-l>", "forward");
    } else {
      this.set(["normal", "insert"], "<A-h>", "back");
      this.set(["normal", "insert"], "<A-l>", "forward");
    }

    this.set("normal", "f", "hint");
    this.set("normal", "F", "hint --action=newtab-click");
    this.set("normal", "<leader>f", "hint --location=browser-ui");
    this.set("hint", "<Esc>", "hints_remove");

    this.set("normal", "gi", "focusinput last");
    this.set("normal", "[[", "go_previous");
    this.set("normal", "]]", "go_next");
    this.set(["normal", "insert"], "<C-,>", "blur");
    this.set("normal", "gu", "go_up");
    this.set("normal", "gU", "go_to_root");

    this.set("command", "<Esc>", "commandline_toggle");
    this.set("command", "<Tab>", "commandline_focus_next");
    this.set("command", "<S-Tab>", "commandline_focus_back");
    this.set("command", "<Down>", "commandline_focus_next");
    this.set("command", "<Up>", "commandline_focus_back");
    this.set("command", "<Enter>", "commandline_accept");
    this.set("command", "<C-d>", "commandline_delete");

    this.set("normal", "<leader>d", "tab_close");
    this.set(["normal", "insert"], "<C-j>", "tab_next");
    this.set(["normal", "insert"], "<C-k>", "tab_prev");
    this.set("normal", "<A-p>", "tab_pin_toggle");
    this.set("normal", "yt", "tab_duplicate");

    // `.` is left to modalkit's native dot-repeat (edits only). Repeating the
    // last non-edit excmd is a separate, explicitly-bound `repeat_command`.
    this.set("insert", "jj", "mode_change normal");
    this.set(["insert", "visual", "op-pending"], "<Esc>", "mode_change normal");
    this.set(["insert", "visual", "op-pending"], "<C-[>", "mode_change normal");

    this.set("normal", "i", "mode_change insert --automove=left");
    this.set("normal", "a", "mode_change insert");
    this.set("normal", "A", "mode_change insert --automove=endline");

    this.set("normal", "u", "undo");
    // Operators (`d`, `c`, `y`), the simple vim motions (`w`, `e`, `b`, `$`,
    // `0`, `^`, `{`, `}`), and text objects (`iw`, `i(`, …) are all handled by
    // modalkit's built-in keybindings — no JS registration needed. The Rust
    // engine resolves the operator from its own context and emits a typed
    // `ExecuteEditingAction` intent (operation + target) which
    // `editing-actions.mts` applies. `c` arrives as a `Delete` plus an
    // insert-mode transition; `dd`/`cc` arrive as `Range(Line)`.

    // `x`/`X` (delete char), `s` (substitute char), `r` (replace char), and
    // `o`/`O` (open line + insert) are native modalkit edits handled by the
    // descriptor path (`r` via `CharReplaceSuffix`; `o`/`O` via the `insert` op
    // sequence, so `.o` re-types the text). `i`/`a`/`A`/`I` insert-entries stay
    // JS-registered (their block-cursor automove semantics still need porting).
    this.set(["normal", "visual"], "I", "motion I");
    this.set("normal", "h", "caret_move left");
    this.set("normal", "l", "caret_move right");
    this.set("normal", "j", "caret_move down");
    this.set("normal", "k", "caret_move up");
    this.set("normal", "yy", "url_yank");
    this.set("normal", "v", "motion v");
    this.set("visual", "h", "motion vh");
    this.set("visual", "l", "motion vl");
    this.set("visual", "d", "motion vd");
    this.set("visual", "c", "motion vc");
    this.set("visual", "y", "visual_selection_copy");

    this.set(["normal", "insert"], "<C-o>", "jumplist_back");
    this.set(["normal", "insert"], "<C-i>", "jumplist_forward");
  }
}
