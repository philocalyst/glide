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

    /// Parse the wire-format mode name used by the JavaScript modal engine.
    pub fn from_wire(value: &str) -> Option<Self> {
        match value {
            "normal" => Some(GlideMode::Normal),
            "insert" => Some(GlideMode::Insert),
            "visual" => Some(GlideMode::Visual),
            "op-pending" => Some(GlideMode::OperatorPending),
            "ignore" => Some(GlideMode::Ignore),
            "command" => Some(GlideMode::Command),
            "hint" => Some(GlideMode::Hint),
            _ => None,
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
pub enum PendingOperator {
    Delete,
    Change,
    Replace,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum AutomaticMoveDirection {
    Left,
    EndOfLine,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum MotionDirection {
    Previous,
    Next,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum WordStyleName {
    Little,
    Big,
    Keyword,
    NonAlphanumeric,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum EditorOperationIntent {
    Motion,
    Delete,
    Change,
    Yank,
    Replace,
    RawDescription { description: String },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
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
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
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

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct EditingActionIntent {
    pub operation: EditorOperationIntent,
    pub target: EditTargetIntent,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct ModeChangeRequest {
    pub target_mode: GlideMode,
    pub pending_operator: Option<PendingOperator>,
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
        editing_action: EditingActionIntent,
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
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct ResolvedKeyResult {
    pub default_prevented: bool,
    pub mode_transition: Option<ModeTransition>,
    pub browser_command_intents: Vec<BrowserCommandIntent>,
    pub pending_sequence_display: PendingSequenceDisplay,
    pub matched_mapping: bool,
    pub has_partial_match: bool,
    pub operator: Option<PendingOperator>,
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
            operator: None,
        }
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum EngineCommand {
    RepeatLastAction,
    ChangeMode {
        request: ModeChangeRequest,
    },
    DispatchBrowserCommand {
        command_name: String,
        arguments: Vec<String>,
        is_repeatable: bool,
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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum GlideApplicationAction {
    ChangeMode(ModeChangeRequest),
    DispatchBrowserCommand {
        command_name: String,
        arguments: Vec<String>,
        is_repeatable: bool,
    },
}

impl ApplicationAction for GlideApplicationAction {
    fn is_edit_sequence(&self, _: &EditContext) -> SequenceStatus {
        match self {
            GlideApplicationAction::ChangeMode(_) => SequenceStatus::Ignore,
            GlideApplicationAction::DispatchBrowserCommand { is_repeatable, .. } => {
                if *is_repeatable {
                    SequenceStatus::Atom
                } else {
                    SequenceStatus::Break
                }
            }
        }
    }

    fn is_last_action(&self, _: &EditContext) -> SequenceStatus {
        match self {
            GlideApplicationAction::ChangeMode(_) => SequenceStatus::Ignore,
            GlideApplicationAction::DispatchBrowserCommand { is_repeatable, .. } => {
                if *is_repeatable {
                    SequenceStatus::Atom
                } else {
                    SequenceStatus::Break
                }
            }
        }
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
