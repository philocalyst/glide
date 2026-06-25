use modalkit::actions::Action;
use modalkit::env::vim::keybindings::{default_vim_keys, InputStep, VimMachine};
use modalkit::env::vim::VimMode;
use modalkit::env::CommonKeyClass;
use modalkit::key::TerminalKey;
use modalkit::keybindings::{EdgeEvent, EdgeRepeat};

use crate::actions::{
    EngineCommand, GlideApplicationAction, GlideApplicationInfo, GlideMode, KeySequence,
    KeymapDefinition,
};

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

/// Build the modalkit machine from modalkit's native vim defaults plus the
/// caller-supplied (JS-registered) keymaps.
///
/// The engine deliberately carries **no** Glide-specific default bindings:
/// modalkit's `default_vim_keys` supplies the state-machine behaviour
/// (operator-pending, counts, motions, repeat), and every Glide-specific
/// binding is registered from JavaScript via `glide.keymaps.set` so that the
/// default keymap set stays hot-reloadable configuration rather than compiled-in
/// Rust.
pub fn build_modal_machine(
    custom_keymaps: &[KeymapDefinition],
) -> VimMachine<TerminalKey, GlideApplicationInfo> {
    let mut modal_machine = default_vim_keys::<GlideApplicationInfo>();

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

    custom_keymaps.iter().any(|keymap_definition| {
        keymap_definition.mode == mode
            && keymap_definition
                .sequence
                .as_slice()
                .starts_with(pending_sequence)
            && keymap_definition.sequence.len() > pending_sequence.len()
    })
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
