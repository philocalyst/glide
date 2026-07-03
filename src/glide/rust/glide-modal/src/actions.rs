use std::fmt::Debug;

use modalkit::editing::application::{ApplicationAction, ApplicationInfo};
use modalkit::editing::context::EditContext;
use modalkit::keybindings::SequenceStatus;
use modalkit::prelude::CommandType;
use serde::{Deserialize, Serialize};

/// A single key in Vim-style notation, e.g. `a`, `<C-a>`, `<leader>`.
pub type KeyNotation = String;
/// An ordered sequence of [`KeyNotation`]s, e.g. `["g", "g"]`.
pub type KeySequence = Vec<KeyNotation>;
/// Positional arguments forwarded alongside a dispatched browser command.
pub type BrowserCommandArguments = Vec<String>;

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum CommandBarKind {
    Search,
    Command,
}

impl From<CommandType> for CommandBarKind {
    fn from(value: CommandType) -> Self {
        match value {
            CommandType::Search => CommandBarKind::Search,
            CommandType::Command => CommandBarKind::Command,
        }
    }
}

#[derive(
    Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize, uniffi::Enum,
)]
pub enum GlideMode {
    Normal,
    Insert,
    Visual,
    OperatorPending,
    Ignore,
    Command,
    Hint,
}

impl GlideMode {
    pub fn as_str(self) -> &'static str {
        match self {
            GlideMode::Normal => "normal",
            GlideMode::Insert => "insert",
            GlideMode::Visual => "visual",
            GlideMode::OperatorPending => "op-pending",
            GlideMode::Ignore => "ignore",
            GlideMode::Command => "command",
            GlideMode::Hint => "hint",
        }
    }

    /// The caret style enum value that the C++ layer expects for this mode.
    ///
    /// Must correspond exactly with `src/glide/cpp/Glide.h::GlideCaretStyle`.
    pub fn caret_style(self) -> u8 {
        match self {
            GlideMode::Normal | GlideMode::Visual | GlideMode::Hint => 0, // block
            GlideMode::OperatorPending => 1,                              // underline
            GlideMode::Insert | GlideMode::Ignore | GlideMode::Command => 2, // line
        }
    }

    /// All built-in modes, in declaration order.
    pub fn all() -> Vec<GlideMode> {
        vec![
            GlideMode::Normal,
            GlideMode::Insert,
            GlideMode::Visual,
            GlideMode::OperatorPending,
            GlideMode::Ignore,
            GlideMode::Command,
            GlideMode::Hint,
        ]
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum AutomaticMoveDirection {
    Left,
    EndOfLine,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MotionDirection {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WordStyleName {
    Little,
    Big,
    Keyword,
    NonAlphanumeric,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MotionIntent {
    Column {
        direction: MotionDirection,
        wrap: bool,
    },
    /// First column of the current line (`0`).
    LineStart,
    /// End of the current line (`$`).
    LineEnd,
    /// First non-blank character of a line (`^`).
    FirstWord {
        direction: MotionDirection,
    },
    /// Whole-line vertical motion (`j` / `k`).
    Line {
        direction: MotionDirection,
    },
    WordBegin {
        direction: MotionDirection,
        word_style: WordStyleName,
    },
    /// End of a word (`e` / `ge`).
    WordEnd {
        direction: MotionDirection,
        word_style: WordStyleName,
    },
    /// Paragraph boundary (`{` / `}`).
    ParagraphBegin {
        direction: MotionDirection,
    },
    RawDescription {
        description: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditorOperationIntent {
    Motion,
    Delete,
    Yank,
    /// Replace the target with `character` (`r{char}`). The char is resolved
    /// from modalkit's `CharReplaceSuffix` submode into the edit context.
    Replace { character: String },
    RawDescription { description: String },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum EditTargetIntent {
    CurrentPosition,
    CurrentSelection,
    LineRange {
        count: u32,
        include_line_break: bool,
    },
    Motion {
        motion: MotionIntent,
        count: u32,
    },
    /// Text object around the cursor (Stage B): `iw`, `di(`, `ci"`, …
    Range {
        range: RangeTargetIntent,
        inclusive: bool,
        count: u32,
    },
    RawDescription {
        description: String,
    },
}

/// A typed text-object kind, mirroring modalkit's `RangeType`.
///
/// Only the variants modalkit actually resolves are surfaced here; `Paragraph`,
/// `Sentence`, and `XmlTag` are `XXX: implement` in modalkit 0.0.24 but still
/// carried through so callers can fall back when they arrive.
///
/// `char` fields are `String`s because uniffi doesn't support `char` directly;
/// each holds a single UTF-8 character.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RangeTargetIntent {
    /// `iw` / `aw` — a word (with optional whitespace when inclusive).
    Word { word_style: WordStyleName },
    /// `i(` / `a(` — text between matching bracket characters.
    Bracketed { left: String, right: String },
    /// `i"` / `a"` — text between matching quote characters.
    Quote { quote: String },
    /// `it` / `at` — XML tag block (modalkit stub).
    XmlTag,
    /// `ip` / `ap` — paragraph (modalkit stub).
    Paragraph,
    /// `is` / `as` — sentence (modalkit stub).
    Sentence,
    /// `Line` range (handled by `LineRange`, kept for completeness).
    Line,
    /// `Buffer` range.
    Buffer,
    /// `Item` range (`ib`-style matching).
    Item,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditingActionIntent {
    pub operation: EditorOperationIntent,
    pub target: EditTargetIntent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct ModeChangeRequest {
    pub target_mode: GlideMode,
    pub automatic_move_direction: Option<AutomaticMoveDirection>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct ModeTransition {
    pub previous_mode: GlideMode,
    pub next_mode: GlideMode,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct PendingSequenceDisplay {
    pub key_notations: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum BrowserCommandIntent {
    ExecuteBrowserCommand {
        command_name: String,
        arguments: Vec<String>,
    },
    ExecuteEditingAction {
        action: WireEditingAction,
    },
    OpenCommandBar {
        prompt_prefix: String,
        command_bar_kind: CommandBarKind,
    },
    InsertText {
        text: String,
    },
    ApplyAutomaticMove {
        automatic_move_direction: AutomaticMoveDirection,
    },
    /// Open a new line below (`o`) or above (`O`) the cursor and enter insert.
    OpenLine {
        above: bool,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct ResolvedKeyResult {
    pub default_prevented: bool,
    pub mode_transition: Option<ModeTransition>,
    pub browser_command_intents: Vec<BrowserCommandIntent>,
    pub pending_sequence_display: PendingSequenceDisplay,
    pub matched_mapping: bool,
    pub has_partial_match: bool,
    /// The excmd string when the matched command was a `DispatchBrowserCommand`.
    /// JS can use this directly instead of calling `#synthesize_command`.
    pub matched_excmd: Option<String>,
    /// The opaque callback id when the matched command was a `Callback`.
    /// JS looks this up in its `#callback_map` to invoke the original closure.
    pub matched_callback_id: Option<u64>,
}

impl Default for ResolvedKeyResult {
    fn default() -> Self {
        Self {
            default_prevented: false,
            mode_transition: None,
            browser_command_intents: Vec::new(),
            pending_sequence_display: PendingSequenceDisplay {
                key_notations: Vec::new(),
            },
            matched_mapping: false,
            has_partial_match: false,
            matched_excmd: None,
            matched_callback_id: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum EngineCommand {
    ChangeMode {
        request: ModeChangeRequest,
    },
    DispatchBrowserCommand {
        command_name: String,
        arguments: Vec<String>,
    },
    /// A JS closure registered for this sequence.  Rust stores the opaque id
    /// so that `ResolvedKeyResult::matched_callback_id` can carry it back to
    /// JS without any separate registry look-up.
    Callback {
        callback_id: u64,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct KeymapDefinition {
    pub mode: GlideMode,
    pub sequence: Vec<String>,
    pub command: EngineCommand,
    #[uniffi(default = false)]
    pub retain_key_display: bool,
    /// Buffer-local mappings are cleared on navigation; global mappings persist.
    #[uniffi(default = false)]
    pub buffer: bool,
    /// Optional human-readable description, surfaced in docs/introspection.
    #[uniffi(default = None)]
    pub description: Option<String>,
    /// When `Some`, this keymap belongs to the named custom mode rather than the
    /// built-in `mode` field.  The `mode` field acts as an ignored sentinel
    /// (`GlideMode::Normal`) for custom-mode keymaps and is never fed to the
    /// modalkit state machine.
    #[uniffi(default = None)]
    pub custom_mode: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GlideApplicationAction {
    ChangeMode(ModeChangeRequest),
    DispatchBrowserCommand {
        command_name: String,
        arguments: Vec<String>,
    },
    Callback {
        callback_id: u64,
    },
}

impl ApplicationAction for GlideApplicationAction {
    // Glide owns dot-repeat via the JS `#last_command` system (modalkit can't
    // see custom/async excmds like `r`), so application actions never feed
    // modalkit's edit-sequence / repeat tracking.
    fn is_edit_sequence(&self, _: &EditContext) -> SequenceStatus {
        SequenceStatus::Ignore
    }

    fn is_last_action(&self, _: &EditContext) -> SequenceStatus {
        SequenceStatus::Ignore
    }

    fn is_last_selection(&self, _: &EditContext) -> SequenceStatus {
        SequenceStatus::Ignore
    }

    fn is_switchable(&self, _: &EditContext) -> bool {
        false
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GlideApplicationInfo {}

impl ApplicationInfo for GlideApplicationInfo {
    type Error = String;
    type Action = GlideApplicationAction;
    type Store = ();
    type WindowId = String;
    type ContentId = String;

    fn content_of_command(command_type: CommandType) -> Self::ContentId {
        match command_type {
            CommandType::Search => "*glide-search*".into(),
            CommandType::Command => "*glide-command*".into(),
        }
    }
}

/// Raw DOM keyboard-event fields forwarded from JavaScript so Rust can convert
/// them to Vim-style key notation without a TS round-trip.
///
/// Field names deliberately mirror the DOM `KeyboardEvent` properties; the JS
/// layer constructs this record directly from `event.*` before passing it over
/// the uniffi boundary.
#[derive(uniffi::Record, Debug, Clone)]
pub struct KeyEventInfo {
    /// `event.key` — the logical key value (e.g. `"a"`, `"Enter"`, `"ArrowUp"`).
    pub key: String,
    /// `event.code` — the physical key identifier (e.g. `"KeyA"`, `"Space"`).
    /// Reserved for future physical-layout translation; currently unused in Rust.
    pub code: String,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    /// `event.metaKey` (Command on macOS, Windows key elsewhere).
    pub meta: bool,
}

/// A flat, structured-clone-safe wire representation of an edit target.
///
/// All variant-specific fields are optional; only the fields relevant to the
/// `kind` are populated. This survives Firefox IPC (parent → content) unchanged
/// because it contains only primitive types that the structured-clone algorithm
/// handles natively.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct WireEditingTarget {
    pub kind: String,
    pub motion: Option<String>,
    pub range: Option<String>,
    pub count: u32,
    pub direction: Option<String>,
    pub word_style: Option<String>,
    pub wrap: Option<bool>,
    pub include_line_break: Option<bool>,
    pub inclusive: Option<bool>,
    pub left: Option<String>,
    pub right: Option<String>,
    pub quote: Option<String>,
    pub description: Option<String>,
}

/// A flat, structured-clone-safe wire representation of an editing action.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct WireEditingAction {
    pub operation: String,
    pub character: Option<String>,
    pub target: WireEditingTarget,
}

// ── Phase 5: KeyDisposition ──────────────────────────────────────────────────

/// A rich instruction packet returned by [`crate::bridge::GlideModalBridge::process_key`]
/// that the JS layer executes verbatim, replacing the old
/// `ResolvedKeyResult` → synthesize → dispatch pipeline.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct KeyDisposition {
    /// Whether the browser's default action for this key event should be suppressed.
    pub prevent_default: bool,
    /// The current pending key sequence to display (e.g. `["d", "w"]` while composing).
    pub sequence_display: Vec<String>,
    /// A mode transition emitted by this key, if any.
    pub mode_transition: Option<ModeTransition>,
    /// When `Some`, the JS layer should arm a key-sequence timeout after this many ms.
    /// (Reserved for future insert-mode timeout; always `None` in Phase 5.)
    pub arm_timeout_ms: Option<u64>,
    /// Notifications to forward to the content process (e.g. partial-match display).
    pub notify_content: Vec<ContentNotification>,
    /// Ordered list of instructions for the JS layer to execute.
    pub instructions: Vec<Instruction>,
    /// True when the key fully matched a registered mapping.
    #[uniffi(default = false)]
    pub matched_mapping: bool,
    /// True when the key extended a partial sequence (more keys expected).
    #[uniffi(default = false)]
    pub has_partial_match: bool,
}

/// A primitive step within an insert-entry or dot-replayed insert session.
/// These are applied in order by the content executor after `Instruction::InsertSequence`.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum InsertOp {
    /// Open a new line (from `o` / `O`).
    OpenLine { above: bool },
    /// Move the caret automatically before entering insert mode (from `a`/`A`/`I`).
    AutoMove { direction: AutomaticMoveDirection },
    /// Insert literal text (from `.`-replay of a typed insert session).
    InsertText { text: String },
    /// Entry motion for `a`/`A`/`I` that carries a column-move editing action.
    MoveToColumn { action: WireEditingAction },
}

/// A single executable step produced by the modal engine for a resolved key event.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum Instruction {
    /// Execute a browser excmd string with optional positional arguments.
    Excmd { command: String, arguments: Vec<String> },
    /// Invoke the JS closure stored under `callback_id` in the engine's callback map.
    Callback { callback_id: u64, sequence: Vec<String> },
    /// Apply a typed editing action in the content process.
    EditingAction { action: WireEditingAction },
    /// Open the command bar with the given prompt prefix.
    OpenCommandBar { prefix: String },
    /// Filter visible hint labels by the typed character (hint mode).
    HintFilter { label: String },
    /// Execute the hint with the given numeric id (hint mode).
    HintExecute { id: u64 },
    /// Exit hint mode without executing anything.
    HintExit,
    /// Apply an insert-entry or dot-replayed insert session (o/O/i/a/A/I and `.`).
    InsertSequence { ops: Vec<InsertOp>, enters_insert: bool },
}

// ── Phase 6: excmd registry ──────────────────────────────────────────────────

/// Parse error returned by [`crate::bridge::GlideModalBridge::parse_excmd`].
#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum ExcmdParseError {
    #[error("Empty command string")]
    EmptyInput,
    #[error("Unknown excmd: `{name}`")]
    UnknownCommand { name: String },
}

/// Metadata for a single built-in Glide excmd, mirroring the
/// `GlideExcmdInfo` interface in `browser-excmds-registry.mts`.
#[derive(uniffi::Record, Debug, Clone)]
pub struct ExcmdInfo {
    /// The excmd name as it appears in a command string (e.g. `"tab_next"`).
    pub name: String,
    /// Human-readable description surfaced in docs / which-key.
    pub description: String,
    /// When `true` the command must be executed in the content process
    /// (mirrors `GlideExcmdInfo.content`).
    pub content_flag: bool,
    /// When `true` the command can be re-issued with dot-repeat.
    pub repeatable: bool,
}

/// A parsed excmd string: command name + positional arguments.
///
/// Produced by [`crate::bridge::GlideModalBridge::parse_excmd`] and consumed
/// by [`crate::bridge::GlideModalBridge::note_executed`] /
/// [`crate::bridge::GlideModalBridge::repeat_last`].
#[derive(uniffi::Record, Debug, Clone)]
pub struct ParsedExcmd {
    pub name: String,
    pub arguments: Vec<String>,
}

/// A notification to forward from the parent process to the content process
/// after key resolution so the content layer can update its display state.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum ContentNotification {
    /// A key sequence is in progress; show the given notations in the status bar.
    KeyMappingPartial { sequence: Vec<String> },
    /// A key sequence fully matched; clear any partial display.
    KeyMappingComplete,
    /// The in-progress sequence was cancelled (no match found, no partial left).
    Cancel,
    /// The active mode has changed; the content process should update the caret / status bar.
    ModeChanged { mode: String },
}
