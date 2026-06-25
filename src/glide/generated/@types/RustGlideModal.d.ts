/**
 * Hand-written type contract for the uniffi-generated `RustGlideModal.sys.mjs`
 * module (produced from `src/glide/rust/glide-modal` by uniffi-bindgen-gecko-js).
 *
 * The real module is generated at build time into
 * `toolkit/components/uniffi-bindgen-gecko-js/components/generated/RustGlideModal.sys.mjs`;
 * this declaration mirrors its public surface so the TypeScript sources can be
 * type-checked without the generated artifact present.
 *
 * Conventions encoded here follow uniffi-bindgen-gecko-js:
 *   - interface methods and record fields are lowerCamelCase
 *   - records are classes constructed from an object literal of their fields
 *   - fieldless enums are exported as frozen objects (member -> integer)
 *   - data-carrying enums are exported as a namespace of variant classes; an
 *     instance is one of those classes, discriminated via `instanceof`
 *
 * If a future uniffi version changes member casing or the enum representation,
 * this single file (and the thin adapter in `modal-engine.mts`) is where it is
 * reconciled.
 */

// ---------------------------------------------------------------------------
// Fieldless enums
// ---------------------------------------------------------------------------

export enum GlideMode {
  Normal,
  Insert,
  Visual,
  OperatorPending,
  Ignore,
  Command,
  Hint,
}

export enum CommandBarKind {
  Search,
  Command,
}

export enum PendingOperator {
  Delete,
  Change,
  Replace,
}

export enum AutomaticMoveDirection {
  Left,
  EndOfLine,
}

export enum MotionDirection {
  Previous,
  Next,
}

export enum WordStyleName {
  Little,
  Big,
  Keyword,
  NonAlphanumeric,
}

// ---------------------------------------------------------------------------
// Data-carrying enums (variant classes discriminated via `instanceof`)
// ---------------------------------------------------------------------------

export namespace MotionIntent {
  export class Column {
    direction: MotionDirection;
    wrap: boolean;
    constructor(fields: { direction: MotionDirection; wrap: boolean });
  }
  export class LineStart {}
  export class LineEnd {}
  export class FirstWord {
    direction: MotionDirection;
    constructor(fields: { direction: MotionDirection });
  }
  export class Line {
    direction: MotionDirection;
    constructor(fields: { direction: MotionDirection });
  }
  export class WordBegin {
    direction: MotionDirection;
    wordStyle: WordStyleName;
    constructor(fields: { direction: MotionDirection; wordStyle: WordStyleName });
  }
  export class WordEnd {
    direction: MotionDirection;
    wordStyle: WordStyleName;
    constructor(fields: { direction: MotionDirection; wordStyle: WordStyleName });
  }
  export class ParagraphBegin {
    direction: MotionDirection;
    constructor(fields: { direction: MotionDirection });
  }
  export class RawDescription {
    description: string;
    constructor(fields: { description: string });
  }
}
export type MotionIntent =
  | MotionIntent.Column
  | MotionIntent.LineStart
  | MotionIntent.LineEnd
  | MotionIntent.FirstWord
  | MotionIntent.Line
  | MotionIntent.WordBegin
  | MotionIntent.WordEnd
  | MotionIntent.ParagraphBegin
  | MotionIntent.RawDescription;

export namespace EditorOperationIntent {
  export class Motion {}
  export class Delete {}
  export class Change {}
  export class Yank {}
  export class Replace {}
  export class RawDescription {
    description: string;
    constructor(fields: { description: string });
  }
}
export type EditorOperationIntent =
  | EditorOperationIntent.Motion
  | EditorOperationIntent.Delete
  | EditorOperationIntent.Change
  | EditorOperationIntent.Yank
  | EditorOperationIntent.Replace
  | EditorOperationIntent.RawDescription;

export namespace EditTargetIntent {
  export class CurrentPosition {}
  export class CurrentSelection {}
  export class LineRange {
    count: number;
    includeLineBreak: boolean;
    constructor(fields: { count: number; includeLineBreak: boolean });
  }
  export class Motion {
    motion: MotionIntent;
    count: number;
    constructor(fields: { motion: MotionIntent; count: number });
  }
  export class RawDescription {
    description: string;
    constructor(fields: { description: string });
  }
}
export type EditTargetIntent =
  | EditTargetIntent.CurrentPosition
  | EditTargetIntent.CurrentSelection
  | EditTargetIntent.LineRange
  | EditTargetIntent.Motion
  | EditTargetIntent.RawDescription;

export namespace BrowserCommandIntent {
  export class ExecuteBrowserCommand {
    commandName: string;
    arguments: string[];
    constructor(fields: { commandName: string; arguments: string[] });
  }
  export class ExecuteEditingAction {
    editingAction: EditingActionIntent;
    constructor(fields: { editingAction: EditingActionIntent });
  }
  export class OpenCommandBar {
    promptPrefix: string;
    commandBarKind: CommandBarKind;
    constructor(fields: { promptPrefix: string; commandBarKind: CommandBarKind });
  }
  export class InsertText {
    text: string;
    constructor(fields: { text: string });
  }
  export class ApplyAutomaticMove {
    automaticMoveDirection: AutomaticMoveDirection;
    constructor(fields: { automaticMoveDirection: AutomaticMoveDirection });
  }
}
export type BrowserCommandIntent =
  | BrowserCommandIntent.ExecuteBrowserCommand
  | BrowserCommandIntent.ExecuteEditingAction
  | BrowserCommandIntent.OpenCommandBar
  | BrowserCommandIntent.InsertText
  | BrowserCommandIntent.ApplyAutomaticMove;

export namespace EngineCommand {
  export class RepeatLastAction {}
  export class ChangeMode {
    request: ModeChangeRequest;
    constructor(fields: { request: ModeChangeRequest });
  }
  export class DispatchBrowserCommand {
    commandName: string;
    arguments: string[];
    isRepeatable: boolean;
    constructor(fields: { commandName: string; arguments: string[]; isRepeatable: boolean });
  }
}
export type EngineCommand =
  | EngineCommand.RepeatLastAction
  | EngineCommand.ChangeMode
  | EngineCommand.DispatchBrowserCommand;

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

export class EditingActionIntent {
  operation: EditorOperationIntent;
  target: EditTargetIntent;
  constructor(fields: { operation: EditorOperationIntent; target: EditTargetIntent });
}

export class ModeChangeRequest {
  targetMode: GlideMode;
  pendingOperator: PendingOperator | null;
  automaticMoveDirection: AutomaticMoveDirection | null;
  constructor(fields: {
    targetMode: GlideMode;
    pendingOperator?: PendingOperator | null;
    automaticMoveDirection?: AutomaticMoveDirection | null;
  });
}

export class ModeTransition {
  previousMode: GlideMode;
  nextMode: GlideMode;
  constructor(fields: { previousMode: GlideMode; nextMode: GlideMode });
}

export class PendingSequenceDisplay {
  keyNotations: string[];
  constructor(fields: { keyNotations: string[] });
}

export class ResolvedKeyResult {
  defaultPrevented: boolean;
  modeTransition: ModeTransition | null;
  browserCommandIntents: BrowserCommandIntent[];
  pendingSequenceDisplay: PendingSequenceDisplay;
  matchedMapping: boolean;
  hasPartialMatch: boolean;
  operator: PendingOperator | null;
}

export class KeymapDefinition {
  mode: GlideMode;
  sequence: string[];
  command: EngineCommand;
  retainKeyDisplay: boolean;
  buffer: boolean;
  description: string | null;
  constructor(fields: {
    mode: GlideMode;
    sequence: string[];
    command: EngineCommand;
    retainKeyDisplay?: boolean;
    buffer?: boolean;
    description?: string | null;
  });
}

// ---------------------------------------------------------------------------
// The interface object — the single authority over Glide's modal state.
// ---------------------------------------------------------------------------

export class GlideModalBridge {
  constructor();

  currentModeName(): string;
  setMode(mode: GlideMode): void;

  currentSequence(): string[];
  resetSequence(): void;

  modeCaretStyle(mode: GlideMode): number;
  modeNames(): string[];

  listKeymaps(mode: GlideMode): KeymapDefinition[];
  setKeymap(keymapDefinition: KeymapDefinition): void;
  delKeymap(mode: GlideMode, sequence: string[], buffer: boolean): void;
  clearBuffer(): void;

  resolveKeyNotation(keyNotation: string): ResolvedKeyResult;
}
