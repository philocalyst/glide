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

/// Internal only: which command bar a modalkit `CommandBar(Focus)` targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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

/// Internal only: the pending key notations to display while a sequence is
/// composing. Reached JS through `KeyDisposition::sequence_display`.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PendingSequenceDisplay {
    pub key_notations: Vec<String>,
}

/// Internal intermediate between the modalkit action stream and the flat
/// [`Instruction`]s handed to JS. Never crosses the FFI boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
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

/// Internal result of resolving one key against the machine. Folded into a
/// [`KeyDisposition`] before crossing to JS; never an FFI type itself.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ResolvedKeyResult {
    pub default_prevented: bool,
    pub mode_transition: Option<ModeTransition>,
    pub browser_command_intents: Vec<BrowserCommandIntent>,
    pub pending_sequence_display: PendingSequenceDisplay,
    pub matched_mapping: bool,
    pub has_partial_match: bool,
    /// The opaque callback id when the matched command was a `Callback`.
    pub matched_callback_id: Option<u64>,
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
    /// A JS closure registered for this sequence. Rust stores the opaque id and
    /// hands it back in `Instruction::callback_id` so JS can invoke the closure.
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
/// `kind` are populated (the rest stay `Default::default()`). This survives
/// Firefox IPC (parent → content) unchanged because it contains only primitive
/// types that the structured-clone algorithm handles natively.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
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

/// The complete instruction packet returned by
/// [`crate::bridge::GlideModalBridge::process_key`]. The JS layer executes it
/// verbatim: prevent the default, update the sequence display, then run each
/// [`Instruction`] in order.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct KeyDisposition {
    /// Whether the browser's default action for this key event should be suppressed.
    pub prevent_default: bool,
    /// The current pending key sequence to display (e.g. `["d", "w"]` while composing).
    pub sequence_display: Vec<String>,
    /// A mode transition emitted by this key, if any.
    pub mode_transition: Option<ModeTransition>,
    /// Ordered list of instructions for the JS layer to execute.
    pub instructions: Vec<Instruction>,
    /// True when the key fully matched a registered mapping.
    pub matched_mapping: bool,
    /// True when the key extended a partial sequence (more keys expected).
    pub has_partial_match: bool,
}

/// A primitive step within an insert-entry or dot-replayed insert session,
/// applied in order by the content executor. This is a flat record (rather than
/// a data enum) so it structured-clones into the content process untouched and
/// maps 1:1 onto the JS `GlideInsertOp` shape — no translation required.
///
/// `kind` is one of `open_line` / `automove` / `insert_text` / `move`; only the
/// fields relevant to that `kind` are populated.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct InsertOp {
    pub kind: String,
    /// `open_line`: open below (`false`) or above (`true`).
    pub above: Option<bool>,
    /// `automove`: `"left"` or `"endline"`.
    pub direction: Option<String>,
    /// `insert_text`: the literal text to type.
    pub text: Option<String>,
    /// `move`: the entry-motion target for `a` / `A` / `I`.
    pub target: Option<WireEditingTarget>,
}

impl InsertOp {
    pub fn open_line(above: bool) -> Self {
        Self { kind: "open_line".into(), above: Some(above), ..Default::default() }
    }

    /// `direction` is `"left"` or `"endline"`.
    pub fn automove(direction: impl Into<String>) -> Self {
        Self { kind: "automove".into(), direction: Some(direction.into()), ..Default::default() }
    }

    pub fn insert_text(text: impl Into<String>) -> Self {
        Self { kind: "insert_text".into(), text: Some(text.into()), ..Default::default() }
    }

    pub fn move_to(target: WireEditingTarget) -> Self {
        Self { kind: "move".into(), target: Some(target), ..Default::default() }
    }
}

/// A single executable step produced by the engine for a resolved key event.
///
/// Flat record with a `kind` discriminant so JS dispatches with `switch (kind)`
/// instead of uniffi `instanceof` checks. `kind` is one of:
///   - `excmd`           → run `command` + `arguments`
///   - `callback`        → invoke the JS closure `callback_id` (matched `sequence`)
///   - `editing-action`  → apply `action` in the content process
///   - `insert-sequence` → apply `insert_ops` then settle into insert if `enters_insert`
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct Instruction {
    pub kind: String,
    pub command: Option<String>,
    pub arguments: Vec<String>,
    pub callback_id: Option<u64>,
    pub sequence: Vec<String>,
    pub action: Option<WireEditingAction>,
    pub insert_ops: Vec<InsertOp>,
    pub enters_insert: bool,
}

impl Instruction {
    pub fn excmd(command: impl Into<String>, arguments: Vec<String>) -> Self {
        Self { kind: "excmd".into(), command: Some(command.into()), arguments, ..Default::default() }
    }

    pub fn callback(callback_id: u64, sequence: Vec<String>) -> Self {
        Self { kind: "callback".into(), callback_id: Some(callback_id), sequence, ..Default::default() }
    }

    pub fn editing_action(action: WireEditingAction) -> Self {
        Self { kind: "editing-action".into(), action: Some(action), ..Default::default() }
    }

    pub fn insert_sequence(insert_ops: Vec<InsertOp>, enters_insert: bool) -> Self {
        Self { kind: "insert-sequence".into(), insert_ops, enters_insert, ..Default::default() }
    }
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

