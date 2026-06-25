use modalkit::actions::Action;
use modalkit::env::vim::keybindings::{default_vim_keys, InputStep, VimMachine};
use modalkit::env::vim::VimMode;
use modalkit::env::CommonKeyClass;
use modalkit::key::TerminalKey;
use modalkit::keybindings::{EdgeEvent, EdgeRepeat};

use crate::actions::{
    AutomaticMoveDirection, EngineCommand, GlideApplicationAction, GlideApplicationInfo, GlideMode,
    KeySequence, KeymapDefinition, ModeChangeRequest,
};

/// Build a [`KeymapDefinition`] for a built-in/default mapping (never buffer-local).
fn builtin_keymap(
    mode: GlideMode,
    sequence: Vec<String>,
    command: EngineCommand,
    retain_key_display: bool,
) -> KeymapDefinition {
    KeymapDefinition {
        mode,
        sequence,
        command,
        retain_key_display,
        buffer: false,
        description: None,
    }
}

pub fn builtin_mode_names() -> Vec<&'static str> {
    vec![
        "normal",
        "insert",
        "visual",
        "op-pending",
        "ignore",
        "command",
        "hint",
    ]
}

pub fn to_modalkit_mode(mode: GlideMode) -> VimMode {
    match mode {
        GlideMode::Normal | GlideMode::Ignore | GlideMode::Hint => VimMode::Normal,
        GlideMode::Insert => VimMode::Insert,
        GlideMode::Visual => VimMode::Visual,
        GlideMode::OperatorPending => VimMode::OperationPending,
        GlideMode::Command => VimMode::Command,
    }
}

pub fn from_modalkit_mode(mode: VimMode) -> GlideMode {
    match mode {
        VimMode::Normal => GlideMode::Normal,
        VimMode::Insert => GlideMode::Insert,
        VimMode::Visual | VimMode::Select => GlideMode::Visual,
        VimMode::OperationPending => GlideMode::OperatorPending,
        VimMode::Command => GlideMode::Command,
        VimMode::LangArg | VimMode::CharSearchSuffix | VimMode::CharReplaceSuffix => {
            GlideMode::Normal
        }
    }
}

fn change_mode_keymap(
    mode: GlideMode,
    sequence: &[&str],
    target_mode: GlideMode,
    automatic_move_direction: Option<AutomaticMoveDirection>,
    retain_key_display: bool,
) -> KeymapDefinition {
    builtin_keymap(
        mode,
        sequence.iter().map(|key| key.to_string()).collect(),
        EngineCommand::ChangeMode {
            request: ModeChangeRequest {
                target_mode,
                pending_operator: None,
                automatic_move_direction,
            },
        },
        retain_key_display,
    )
}

fn dispatch_keymap(mode: GlideMode, sequence: &[&str], command_name: &str) -> KeymapDefinition {
    builtin_keymap(
        mode,
        sequence.iter().map(|key| key.to_string()).collect(),
        EngineCommand::DispatchBrowserCommand {
            command_name: command_name.to_string(),
            arguments: Vec::new(),
            is_repeatable: false,
        },
        false,
    )
}

pub fn default_keymaps() -> Vec<KeymapDefinition> {
    vec![
        change_mode_keymap(
            GlideMode::Normal,
            &["i"],
            GlideMode::Insert,
            Some(AutomaticMoveDirection::Left),
            false,
        ),
        change_mode_keymap(GlideMode::Normal, &["a"], GlideMode::Insert, None, false),
        change_mode_keymap(
            GlideMode::Normal,
            &["A"],
            GlideMode::Insert,
            Some(AutomaticMoveDirection::EndOfLine),
            false,
        ),
        change_mode_keymap(GlideMode::Insert, &["j", "j"], GlideMode::Normal, None, false),
        dispatch_keymap(GlideMode::Normal, &[":"], "commandline_show"),
        builtin_keymap(
            GlideMode::Normal,
            vec![".".into()],
            EngineCommand::RepeatLastAction,
            false,
        ),
        dispatch_keymap(GlideMode::Normal, &["g", "g"], "scroll_top"),
        dispatch_keymap(GlideMode::Normal, &["G"], "scroll_bottom"),
        dispatch_keymap(GlideMode::Normal, &["<C-d>"], "scroll_half_page_down"),
        dispatch_keymap(GlideMode::Insert, &["<C-d>"], "scroll_half_page_down"),
        dispatch_keymap(GlideMode::Normal, &["<C-u>"], "scroll_half_page_up"),
        dispatch_keymap(GlideMode::Insert, &["<C-u>"], "scroll_half_page_up"),
        change_mode_keymap(
            GlideMode::Normal,
            &["d"],
            GlideMode::OperatorPending,
            None,
            true,
        ),
        change_mode_keymap(GlideMode::Normal, &["v"], GlideMode::Visual, None, false),
    ]
}

pub fn build_modal_machine(
    custom_keymaps: &[KeymapDefinition],
) -> VimMachine<TerminalKey, GlideApplicationInfo> {
    let mut modal_machine = default_vim_keys::<GlideApplicationInfo>();

    for default_keymap in default_overlay_keymaps() {
        add_keymap_definition(&mut modal_machine, &default_keymap);
    }

    for custom_keymap in custom_keymaps {
        add_keymap_definition(&mut modal_machine, custom_keymap);
    }

    modal_machine
}

pub fn is_displayable_partial_match(
    mode: GlideMode,
    pending_sequence: &[String],
    custom_keymaps: &[KeymapDefinition],
) -> bool {
    if pending_sequence.is_empty() {
        return false;
    }

    default_overlay_keymaps()
        .into_iter()
        .chain(custom_keymaps.iter().cloned())
        .any(|keymap_definition| {
            keymap_definition.mode == mode
                && keymap_definition
                    .sequence
                    .as_slice()
                    .starts_with(pending_sequence)
                && keymap_definition.sequence.len() > pending_sequence.len()
        })
}

fn default_overlay_keymaps() -> Vec<KeymapDefinition> {
    default_keymaps()
        .into_iter()
        .filter(|keymap_definition| {
            matches!(
                keymap_definition
                    .sequence
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>()
                    .as_slice(),
                ["j", "j"] | [":"] | ["g", "g"]
            )
        })
        .collect()
}

fn add_keymap_definition(
    modal_machine: &mut VimMachine<TerminalKey, GlideApplicationInfo>,
    keymap_definition: &KeymapDefinition,
) {
    // Glide's key notation is a superset of what modalkit's `TerminalKey` can
    // parse (e.g. the `<D-…>` super modifier). If any key in the sequence can't
    // be represented, skip registering rather than panicking — the modal layer
    // still tracks the mapping for introspection, it just won't match natively.
    let Some(edge_path) = key_sequence_to_edge_path(&keymap_definition.sequence) else {
        return;
    };
    let input_step = keymap_definition_to_input_step(keymap_definition);

    modal_machine.add_mapping(
        to_modalkit_mode(keymap_definition.mode),
        edge_path.as_slice(),
        &input_step,
    );
}

fn key_sequence_to_edge_path(
    key_sequence: &KeySequence,
) -> Option<Vec<(EdgeRepeat, EdgeEvent<TerminalKey, CommonKeyClass>)>> {
    key_sequence
        .iter()
        .map(|key_notation| {
            let terminal_key = key_notation.parse::<TerminalKey>().ok()?;
            Some((EdgeRepeat::Once, EdgeEvent::Key(terminal_key)))
        })
        .collect()
}

fn keymap_definition_to_input_step(
    keymap_definition: &KeymapDefinition,
) -> InputStep<GlideApplicationInfo> {
    match &keymap_definition.command {
        EngineCommand::RepeatLastAction => InputStep::new().actions(vec![Action::Repeat(
            modalkit::prelude::RepeatType::EditSequence,
        )]),
        EngineCommand::ChangeMode { request } => InputStep::new()
            .actions(vec![Action::Application(
                GlideApplicationAction::ChangeMode(*request),
            )])
            .goto(to_modalkit_mode(request.target_mode)),
        EngineCommand::DispatchBrowserCommand {
            command_name,
            arguments,
            is_repeatable,
        } => InputStep::new().actions(vec![Action::Application(
            GlideApplicationAction::DispatchBrowserCommand {
                command_name: command_name.clone(),
                arguments: arguments.clone(),
                is_repeatable: *is_repeatable,
            },
        )]),
    }
}
