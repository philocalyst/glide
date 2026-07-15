// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/**
 * TypeScript type declarations for the uniffi-bindgen-gecko-js generated module
 * `RustGlideModal.sys.mjs` (from the `glide_modal` Rust crate).
 *
 * uniffi conventions used here:
 *   - C-like enums  → class with static readonly variant properties
 *   - Data enums    → namespace containing one class per variant + union type
 *   - Records       → class with a constructor that accepts a plain object
 *   - Objects       → class (the bridge itself)
 *
 * Rust snake_case field names become camelCase in the generated JS.
 */

// ── C-like enums ─────────────────────────────────────────────────────────────

export declare class GlideMode {
  private constructor();
  static readonly Normal: GlideMode;
  static readonly Insert: GlideMode;
  static readonly Visual: GlideMode;
  static readonly OperatorPending: GlideMode;
  static readonly Ignore: GlideMode;
  static readonly Command: GlideMode;
  static readonly Hint: GlideMode;
}

export declare class AutomaticMoveDirection {
  private constructor();
  static readonly Left: AutomaticMoveDirection;
  static readonly EndOfLine: AutomaticMoveDirection;
}

// ── Records (plain structured-clone-safe objects) ─────────────────────────────

export declare class ModeChangeRequest {
  readonly targetMode: GlideMode;
  readonly automaticMoveDirection: AutomaticMoveDirection | null;
  constructor(props: {
    targetMode: GlideMode;
    automaticMoveDirection: AutomaticMoveDirection | null;
  });
}

export declare class ModeTransition {
  readonly previousMode: GlideMode;
  readonly nextMode: GlideMode;
  constructor(props: { previousMode: GlideMode; nextMode: GlideMode });
}

export declare class WireEditingTarget {
  readonly kind: string;
  readonly motion: string | null;
  readonly range: string | null;
  readonly count: number;
  readonly direction: string | null;
  readonly wordStyle: string | null;
  readonly wrap: boolean | null;
  readonly includeLineBreak: boolean | null;
  readonly inclusive: boolean | null;
  readonly left: string | null;
  readonly right: string | null;
  readonly quote: string | null;
  readonly description: string | null;
  constructor(props: {
    kind: string;
    motion?: string | null;
    range?: string | null;
    count: number;
    direction?: string | null;
    wordStyle?: string | null;
    wrap?: boolean | null;
    includeLineBreak?: boolean | null;
    inclusive?: boolean | null;
    left?: string | null;
    right?: string | null;
    quote?: string | null;
    description?: string | null;
  });
}

export declare class WireEditingAction {
  readonly operation: string;
  readonly character: string | null;
  readonly target: WireEditingTarget;
  constructor(props: {
    operation: string;
    character?: string | null;
    target: WireEditingTarget;
  });
}

export declare class KeymapDefinition {
  readonly mode: GlideMode;
  readonly sequence: string[];
  readonly command: EngineCommand;
  readonly retainKeyDisplay: boolean;
  readonly buffer: boolean;
  readonly description: string | null;
  readonly customMode: string | null;
  constructor(props: {
    mode: GlideMode;
    sequence: string[];
    command: EngineCommand;
    retainKeyDisplay?: boolean;
    buffer?: boolean;
    description?: string | null;
    customMode?: string | null;
  });
}

export declare class KeyEventInfo {
  readonly key: string;
  readonly code: string;
  readonly ctrl: boolean;
  readonly alt: boolean;
  readonly shift: boolean;
  readonly meta: boolean;
  constructor(props: {
    key: string;
    code: string;
    ctrl: boolean;
    alt: boolean;
    shift: boolean;
    meta: boolean;
  });
}

export declare class KeyDisposition {
  readonly preventDefault: boolean;
  readonly sequenceDisplay: string[];
  readonly modeTransition: ModeTransition | null;
  readonly instructions: Instruction[];
  readonly matchedMapping: boolean;
  readonly hasPartialMatch: boolean;
}

export declare class ExcmdInfo {
  readonly name: string;
  readonly description: string;
  readonly contentFlag: boolean;
  readonly repeatable: boolean;
}

export declare class ParsedExcmd {
  readonly name: string;
  readonly arguments: string[];
  constructor(props: { name: string; arguments: string[] });
}

// ── Flat instruction records (kind-discriminated, structured-clone-safe) ──────

/**
 * One step of an insert-entry / dot-replayed insert session. Flat record with a
 * `kind` discriminant (`open_line` | `automove` | `insert_text` | `move`); only
 * the fields relevant to that `kind` are populated. Matches the browser-side
 * `GlideInsertOp` shape 1:1.
 */
export declare class InsertOp {
  readonly kind: string;
  readonly above: boolean | null;
  readonly direction: string | null;
  readonly text: string | null;
  readonly target: WireEditingTarget | null;
}

/**
 * A single executable step from the engine. Flat record with a `kind`
 * discriminant (`excmd` | `callback` | `editing-action` | `insert-sequence`);
 * only the fields relevant to that `kind` are populated.
 */
export declare class Instruction {
  readonly kind: string;
  readonly command: string | null;
  readonly arguments: string[];
  readonly callbackId: bigint | null;
  readonly sequence: string[];
  readonly action: WireEditingAction | null;
  readonly insertOps: InsertOp[];
  readonly entersInsert: boolean;
}

// ── Data enums (namespace + variant classes) ──────────────────────────────────

export declare namespace EngineCommand {
  export declare class ChangeMode {
    readonly request: ModeChangeRequest;
    constructor(props: { request: ModeChangeRequest });
  }
  export declare class DispatchBrowserCommand {
    readonly commandName: string;
    readonly arguments: string[];
    constructor(props: { commandName: string; arguments: string[] });
  }
  export declare class Callback {
    readonly callbackId: bigint;
    constructor(props: { callbackId: bigint });
  }
}
export type EngineCommand =
  | EngineCommand.ChangeMode
  | EngineCommand.DispatchBrowserCommand
  | EngineCommand.Callback;

export declare namespace ExcmdParseError {
  export declare class EmptyInput {
    constructor();
  }
  export declare class UnknownCommand {
    readonly name: string;
    constructor(props: { name: string });
  }
}
export type ExcmdParseError =
  | ExcmdParseError.EmptyInput
  | ExcmdParseError.UnknownCommand;

// ── The bridge object ─────────────────────────────────────────────────────────

export declare class GlideModalBridge {
  constructor();

  // Mode
  currentModeName(): string;
  setMode(mode: GlideMode): void;
  modeCaretStyle(mode: GlideMode): number;

  // Sequence
  currentSequence(): string[];
  resetSequence(): void;

  // Key resolution: raw DOM event → full execution plan.
  processKey(event: KeyEventInfo): KeyDisposition | null;

  // Keymaps
  setKeymap(keymapDefinition: KeymapDefinition): void;
  delKeymap(mode: GlideMode, sequence: string[], buffer: boolean): void;
  clearBuffer(): void;

  // Custom modes (Phase 4)
  registerCustomMode(modeName: string, caretStyle: number): void;
  setCustomMode(modeName: string): void;
  currentCustomMode(): string | null;
  customModeCaretStyle(modeName: string): number | null;
  setCustomKeymap(keymapDefinition: KeymapDefinition): void;
  delCustomKeymap(modeName: string, sequence: string[]): void;

  // Excmd registry + dot-repeat (Phase 6)
  excmdRegistry(): ExcmdInfo[];
  parseExcmd(input: string): ParsedExcmd;
  noteExecuted(parsed: ParsedExcmd): void;
  repeatLast(): ParsedExcmd | null;
}
