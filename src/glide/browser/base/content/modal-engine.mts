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
export interface GlideEditingAction {
  operation: "motion" | "delete" | "change" | "yank" | "replace" | "raw";
  target: {
    kind: "current-position" | "current-selection" | "line-range" | "motion" | "raw";
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
    count: number;
    direction?: "previous" | "next";
    wordStyle?: "little" | "big" | "keyword" | "non-alphanumeric";
    wrap?: boolean;
    includeLineBreak?: boolean;
    description?: string;
  };
}

/**
 * A minimal trie-node-shaped result returned by {@link GlideModalEngine.handle_key_event}.
 *
 * `browser.mts` was written against the previous trie-based `KeyManager` and
 * branches on these fields; we synthesize the same shape from the Rust
 * resolution result so that consumer remains unchanged.
 */
export interface ResolvedMappingNode {
  /** A longer mapping starts with the current sequence; wait for more keys. */
  has_children: boolean;
  value:
    | {
      sequence: string[];
      command: glide.ExcmdValue;
      retain_key_display: boolean;
      deleted: boolean;
      description: string | undefined;
      /**
       * Typed editing action descriptor built from the Rust
       * `ExecuteEditingAction` intent. Forwarded to the content process so the
       * descriptor-driven executor (`editing-actions.mts`) can apply counts and
       * operator×motion composition. Absent for commands that have no typed
       * descriptor (e.g. plain excmds); the legacy per-key path then runs.
       */
      editing_action?: GlideEditingAction;
    }
    | null;
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
}

function registry_key(mode: GlideMode, sequence: string[]): string {
  return `${mode}\u0000${sequence.join("\u0001")}`;
}

interface ParsedModeChange {
  target: GlideMode;
  automove: "left" | "endline" | null;
  operator: "d" | "c" | null;
}

function rust_motion_direction(dir: RustGlideModalT.MotionDirection): "previous" | "next" {
  return dir === RustGlideModal.MotionDirection.Next ? "next" : "previous";
}

function rust_word_style(style: RustGlideModalT.WordStyleName): GlideEditingAction["target"]["wordStyle"] {
  switch (style) {
    case RustGlideModal.WordStyleName.Little:
      return "little";
    case RustGlideModal.WordStyleName.Big:
      return "big";
    case RustGlideModal.WordStyleName.Keyword:
      return "keyword";
    case RustGlideModal.WordStyleName.NonAlphanumeric:
      return "non-alphanumeric";
    default:
      return "little";
  }
}

/**
 * Convert a Rust `MotionIntent` into the plain `motion` kind string used by
 * `GlideEditingAction`. Returns `undefined` for variants the content executor
 * doesn't handle yet (Stage A covers the common vim motions).
 */
function motion_kind_from_intent(motion: RustGlideModalT.MotionIntent): GlideEditingAction["target"]["motion"] | undefined {
  if (motion instanceof RustGlideModal.MotionIntent.Column) return "column";
  if (motion instanceof RustGlideModal.MotionIntent.LineStart) return "line-start";
  if (motion instanceof RustGlideModal.MotionIntent.LineEnd) return "line-end";
  if (motion instanceof RustGlideModal.MotionIntent.FirstWord) return "first-word";
  if (motion instanceof RustGlideModal.MotionIntent.Line) return "line";
  if (motion instanceof RustGlideModal.MotionIntent.WordBegin) return "word-begin";
  if (motion instanceof RustGlideModal.MotionIntent.WordEnd) return "word-end";
  if (motion instanceof RustGlideModal.MotionIntent.ParagraphBegin) return "paragraph-begin";
  if (motion instanceof RustGlideModal.MotionIntent.RawDescription) return "raw";
  return undefined;
}

/**
 * Convert a Rust `EditingActionIntent` into a plain, structured-clone-safe
 * `GlideEditingAction`. Returns `undefined` when the intent doesn't carry a
 * typed descriptor the content executor can consume (Stage A: motions only,
 * `Motion`/`Delete`/`Change`/`Yank` operations); the caller then falls back to
 * the legacy per-key path.
 */
function editing_action_from_intent(
  intent: RustGlideModalT.EditingActionIntent,
): GlideEditingAction | undefined {
  let operation: GlideEditingAction["operation"];
  const op = intent.operation;
  if (op instanceof RustGlideModal.EditorOperationIntent.Motion) {
    operation = "motion";
  } else if (op instanceof RustGlideModal.EditorOperationIntent.Delete) {
    operation = "delete";
  } else if (op instanceof RustGlideModal.EditorOperationIntent.Change) {
    operation = "change";
  } else if (op instanceof RustGlideModal.EditorOperationIntent.Yank) {
    operation = "yank";
  } else if (op instanceof RustGlideModal.EditorOperationIntent.Replace) {
    operation = "replace";
  } else if (op instanceof RustGlideModal.EditorOperationIntent.RawDescription) {
    operation = "raw";
  } else {
    return undefined;
  }

  if (operation === "raw" || operation === "replace") {
    // Replace isn't routed through the descriptor executor yet — Stage A.
    return undefined;
  }

  const target = intent.target;
  if (target instanceof RustGlideModal.EditTargetIntent.Motion) {
    const motion_kind = motion_kind_from_intent(target.motion);
    if (motion_kind === undefined || motion_kind === "raw") {
      return undefined;
    }

    const action: GlideEditingAction = {
      operation,
      target: {
        kind: "motion",
        motion: motion_kind,
        count: target.count,
      },
    };

    // Attach variant-specific fields so the executor can pick the right offset
    // primitive.
    const m = target.motion;
    if (m instanceof RustGlideModal.MotionIntent.Column) {
      action.target.direction = rust_motion_direction(m.direction);
      action.target.wrap = m.wrap;
    } else if (m instanceof RustGlideModal.MotionIntent.FirstWord) {
      action.target.direction = rust_motion_direction(m.direction);
    } else if (m instanceof RustGlideModal.MotionIntent.Line) {
      action.target.direction = rust_motion_direction(m.direction);
    } else if (m instanceof RustGlideModal.MotionIntent.WordBegin) {
      action.target.direction = rust_motion_direction(m.direction);
      action.target.wordStyle = rust_word_style(m.wordStyle);
    } else if (m instanceof RustGlideModal.MotionIntent.WordEnd) {
      action.target.direction = rust_motion_direction(m.direction);
      action.target.wordStyle = rust_word_style(m.wordStyle);
    } else if (m instanceof RustGlideModal.MotionIntent.ParagraphBegin) {
      action.target.direction = rust_motion_direction(m.direction);
    }

    return action;
  } else if (target instanceof RustGlideModal.EditTargetIntent.LineRange) {
    return {
      operation,
      target: {
        kind: "line-range",
        count: target.count,
        includeLineBreak: target.includeLineBreak,
      },
    };
  }

  // `CurrentPosition` / `CurrentSelection` / `RawDescription` — Stage A skips
  // these; the legacy per-key path handles them where applicable.
  return undefined;
}

function parse_mode_change(excmd: string): ParsedModeChange | null {
  const match = /^mode_change\s+(\S+)(.*)$/.exec(excmd.trim());
  if (!match) {
    return null;
  }

  const rest = match[2] ?? "";
  const automove_raw = /--automove=(\S+)/.exec(rest)?.[1] ?? null;
  const operator_raw = /--operator=(\S+)/.exec(rest)?.[1] ?? null;

  return {
    target: match[1] as GlideMode,
    automove: automove_raw === "left" || automove_raw === "endline" ? automove_raw : null,
    operator: operator_raw === "d" || operator_raw === "c" ? operator_raw : null,
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

  /** Keys accumulated for the current in-progress sequence (browser-facing). */
  #sequence: string[] = [];

  /**
   * Custom modes registered via `glide.modes.register`. The Rust engine has a
   * fixed `GlideMode` enum and can't represent these, so they're owned here:
   * tracked for listing/caret styling and resolved JS-side (see
   * {@link GlideModalEngine.handle_key_event}).
   */
  #custom_modes: Map<GlideMode, { caret: "block" | "underline" | "line" }> = new Map();

  /**
   * When a custom mode is active, the authoritative current mode (the Rust
   * engine only knows built-in modes). `null` when a built-in mode is active.
   */
  #js_mode: GlideMode | null = null;

  #log: ConsoleInstance = console.createInstance
    ? console.createInstance({ prefix: "Glide[Modal]", maxLogLevelPref: "glide.logging.loglevel" })
    : (console as any);

  constructor() {
    this.#bridge = new RustGlideModal.GlideModalBridge();
    this.#register_builtin_keymaps();
  }

  // ----- sequence / mode state -------------------------------------------

  get current_sequence(): string[] {
    return this.#sequence;
  }

  get has_partial_mapping(): boolean {
    return this.#sequence.length !== 0;
  }

  reset_sequence() {
    this.#sequence = [];
    this.#bridge.resetSequence();
  }

  clear_buffer() {
    for (const [key, entry] of [...this.#registry]) {
      if (entry.buffer) {
        this.#registry.delete(key);
      }
    }
    this.#bridge.clearBuffer();
  }

  get mode_names(): GlideMode[] {
    return [...BUILTIN_GLIDE_MODES, ...this.#custom_modes.keys()];
  }

  get current_mode(): GlideMode {
    // A custom mode (if active) is authoritative — the Rust engine only tracks
    // built-in modes.
    return this.#js_mode ?? (this.#bridge.currentModeName() as GlideMode);
  }

  /**
   * Register a custom mode. Mirrors the historical `glide.modes.register`:
   * a mode can only be registered once (built-in or custom).
   */
  register_mode(mode: GlideMode, opts: { caret: "block" | "underline" | "line" }) {
    if (is_builtin_mode(mode) || this.#custom_modes.has(mode)) {
      throw new Error(
        `The \`${mode}\` mode has already been registered. Modes can only be registered once`,
      );
    }
    this.#custom_modes.set(mode, { caret: opts.caret });
  }

  /**
   * Keep the engine authoritative when the mode changes outside the key
   * pipeline (command bar, hints, sandbox `glide.ctx.mode = …`, custom modes).
   */
  set_mode(mode: GlideMode) {
    this.#sequence = [];
    if (this.#custom_modes.has(mode)) {
      // Rust can't represent a custom mode; track it JS-side and park the Rust
      // engine in normal so it stops matching built-in mappings until we return.
      this.#js_mode = mode;
      this.#bridge.setMode(rust_mode("normal"));
      return;
    }

    this.#js_mode = null;
    this.#bridge.setMode(rust_mode(mode));
  }

  mode_to_style_enum(mode: GlideMode): number {
    const custom = this.#custom_modes.get(mode);
    if (custom) {
      return CARET_STYLE_TO_ENUM[custom.caret];
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
      this.#registry.set(registry_key(mode, sequence), {
        mode,
        sequence,
        lhs,
        rhs,
        description: opts?.description,
        retain_key_display,
        buffer,
        deleted: false,
      });

      // Custom modes live only in the JS registry (Rust can't represent them);
      // they're resolved by `handle_key_event` directly.
      if (this.#custom_modes.has(mode)) {
        continue;
      }

      this.#bridge.setKeymap(
        new RustGlideModal.KeymapDefinition({
          mode: rust_mode(mode),
          sequence,
          command: this.#to_engine_command(mode, sequence, rhs),
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
      this.#registry.delete(registry_key(mode, sequence));
      if (this.#custom_modes.has(mode)) {
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

  handle_key_event(
    event: KeyboardEvent,
    _current_mode: GlideMode,
  ): ResolvedMappingNode | undefined {
    const keyn = Keys.event_to_key_notation(event);
    const mode_before = this.current_mode;
    this.#sequence.push(keyn);

    // Custom modes have no Rust representation, so resolve them against the JS
    // registry directly.
    if (this.#custom_modes.has(mode_before)) {
      return this.#resolve_in_custom_mode(mode_before);
    }

    const result = this.#bridge.resolveKeyNotation(keyn);

    if (result.hasPartialMatch) {
      this.#log.debug(`${keyn} -> partial mapping`);
      return { has_children: true, value: null };
    }

    if (!result.matchedMapping) {
      this.#log.debug(`${keyn} -> did not match`);
      return undefined;
    }

    const matched_sequence = [...this.#sequence];
    const entry = this.#registry.get(registry_key(mode_before, matched_sequence));

    const command = entry?.rhs ?? this.#synthesize_command(result);
    if (command == null) {
      // e.g. plain typing in insert mode — let the browser handle the key.
      this.#log.debug(`${keyn} -> matched with no executable command (pass-through)`);
      return undefined;
    }

    // Extract a typed editing-action descriptor (if any) from the Rust
    // `ExecuteEditingAction` intent. The content process uses it to apply
    // counts and operator×motion composition; when absent the legacy per-key
    // path runs unchanged.
    const editing_action = this.#editing_action_from_result(result);

    return {
      has_children: false,
      value: {
        sequence: matched_sequence,
        command,
        retain_key_display: entry?.retain_key_display ?? this.#synthesize_retain(result),
        deleted: false,
        description: entry?.description,
        editing_action,
      },
    };
  }

  /**
   * Resolve a key in a custom (JS-registered) mode using a small prefix matcher
   * over the registry, mirroring the Rust engine's managed-mode resolver.
   */
  #resolve_in_custom_mode(mode: GlideMode): ResolvedMappingNode | undefined {
    const pending = this.#sequence;

    const entry = this.#registry.get(registry_key(mode, pending));
    if (entry && !entry.deleted) {
      const matched_sequence = [...pending];
      this.#sequence = [];
      return {
        has_children: false,
        value: {
          sequence: matched_sequence,
          command: entry.rhs,
          retain_key_display: entry.retain_key_display,
          deleted: false,
          description: entry.description,
        },
      };
    }

    const has_partial = [...this.#registry.values()].some(candidate =>
      !candidate.deleted
      && candidate.mode === mode
      && candidate.sequence.length > pending.length
      && pending.every((key, index) => candidate.sequence[index] === key)
    );
    if (has_partial) {
      this.#log.debug(`${pending.join("")} -> partial mapping (custom mode)`);
      return { has_children: true, value: null };
    }

    this.#log.debug(`${pending.join("")} -> did not match (custom mode)`);
    this.#sequence = [];
    return undefined;
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
    _mode: GlideMode,
    sequence: string[],
    rhs: glide.ExcmdValue,
  ): RustGlideModalT.EngineCommand {
    if (typeof rhs === "string") {
      if (rhs.trim() === "repeat") {
        return new RustGlideModal.EngineCommand.RepeatLastAction();
      }

      const mode_change = parse_mode_change(rhs);
      if (mode_change) {
        return new RustGlideModal.EngineCommand.ChangeMode({
          request: new RustGlideModal.ModeChangeRequest({
            targetMode: rust_mode(mode_change.target),
            pendingOperator: mode_change.operator === "d"
              ? RustGlideModal.PendingOperator.Delete
              : mode_change.operator === "c"
              ? RustGlideModal.PendingOperator.Change
              : null,
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
        isRepeatable: true,
      });
    }

    // Non-string excmd values (JS callbacks / structured excmds) can't be held
    // by Rust, so the registry is the source of truth for execution. We still
    // register the sequence so Rust can match it; a synthetic token is used as
    // the command name (it is never executed via this path — `handle_key_event`
    // resolves the real value from the registry).
    return new RustGlideModal.EngineCommand.DispatchBrowserCommand({
      commandName: `__glide_callback:${sequence.join("")}`,
      arguments: [],
      isRepeatable: true,
    });
  }

  /**
   * Build a `glide.ExcmdValue` for a mapping that matched a built-in Rust
   * default (i.e. one not present in the TS registry).
   */
  #synthesize_command(result: RustGlideModalT.ResolvedKeyResult): glide.ExcmdValue | null {
    if (result.modeTransition) {
      const target = result.modeTransition.nextMode;
      const parts = [`mode_change ${this.#rust_mode_name(target)}`];

      for (const intent of result.browserCommandIntents) {
        if (intent instanceof RustGlideModal.BrowserCommandIntent.ApplyAutomaticMove) {
          parts.push(
            intent.automaticMoveDirection === RustGlideModal.AutomaticMoveDirection.EndOfLine
              ? "--automove=endline"
              : "--automove=left",
          );
        }
      }

      if (result.operator === RustGlideModal.PendingOperator.Delete) {
        parts.push("--operator=d");
      } else if (result.operator === RustGlideModal.PendingOperator.Change) {
        parts.push("--operator=c");
      }

      return parts.join(" ") as glide.ExcmdString;
    }

    for (const intent of result.browserCommandIntents) {
      if (intent instanceof RustGlideModal.BrowserCommandIntent.ExecuteBrowserCommand) {
        return (intent.arguments.length
          ? `${intent.commandName} ${intent.arguments.join(" ")}`
          : intent.commandName) as glide.ExcmdString;
      }
      if (intent instanceof RustGlideModal.BrowserCommandIntent.OpenCommandBar) {
        return "commandline_show";
      }
      // `InsertText` / `ExecuteEditingAction` intents are handled content-side
      // (or are plain insert-mode typing) and have no parent excmd.
    }

    return null;
  }

  #synthesize_retain(result: RustGlideModalT.ResolvedKeyResult): boolean {
    return result.modeTransition?.nextMode === RustGlideModal.GlideMode.OperatorPending;
  }

  /**
   * Extract the first `ExecuteEditingAction` intent from a resolved key result
   * and convert it to a plain `GlideEditingAction`. Returns `undefined` when
   * there's no typed descriptor or it isn't handled by Stage A's executor.
   */
  #editing_action_from_result(result: RustGlideModalT.ResolvedKeyResult): GlideEditingAction | undefined {
    for (const intent of result.browserCommandIntents) {
      if (intent instanceof RustGlideModal.BrowserCommandIntent.ExecuteEditingAction) {
        return editing_action_from_intent(intent.editingAction);
      }
    }
    return undefined;
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

    this.set("normal", ".", "repeat");
    this.set("insert", "jj", "mode_change normal");
    this.set(["insert", "visual", "op-pending"], "<Esc>", "mode_change normal");
    this.set(["insert", "visual", "op-pending"], "<C-[>", "mode_change normal");

    this.set("normal", "i", "mode_change insert --automove=left");
    this.set("normal", "a", "mode_change insert");
    this.set("normal", "A", "mode_change insert --automove=endline");

    this.set("normal", "u", "undo");
    this.set("normal", "d", "mode_change op-pending --operator=d", { retain_key_display: true });
    this.set("normal", "c", "mode_change op-pending --operator=c", { retain_key_display: true });
    this.set("op-pending", "iw", "execute_motion");
    this.set("op-pending", "h", "execute_motion");
    this.set("op-pending", "j", "execute_motion");
    this.set("op-pending", "k", "execute_motion");
    this.set("op-pending", "l", "execute_motion");
    this.set("op-pending", "d", "execute_motion");

    this.set(["normal", "visual"], "w", "motion w");
    this.set(["normal", "visual"], "W", "motion W");
    this.set("normal", "e", "motion e");
    this.set("normal", "b", "motion b");
    this.set("normal", "B", "motion B");
    this.set("normal", "x", "motion x");
    this.set("normal", "X", "motion X");
    this.set("normal", "o", "motion o");
    this.set("normal", "{", "motion {");
    this.set("normal", "}", "motion }");
    this.set("normal", "r", "r");
    this.set("normal", "s", "motion s");
    this.set(["normal", "visual"], "I", "motion I");
    this.set("normal", "0", "motion 0");
    this.set("normal", "^", "motion ^");
    this.set("normal", "$", "motion $");
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
