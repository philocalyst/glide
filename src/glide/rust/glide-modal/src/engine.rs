use modalkit::actions::{
    Action, CommandBarAction, CursorAction, EditAction, EditorAction, HistoryAction,
    InsertTextAction,
};
use modalkit::env::vim::keybindings::VimMachine;
use modalkit::key::TerminalKey;
use modalkit::keybindings::BindingMachine;
use modalkit::prelude::{
    Char, Count, EditTarget, MoveDir1D, MoveType, RangeType, Specifier, WordStyle,
};

use crate::actions::{
    BrowserCommandIntent, CommandBarKind, EditTargetIntent, EditingActionIntent,
    EditorOperationIntent, EngineCommand, GlideApplicationAction, GlideApplicationInfo, GlideMode,
    KeySequence, KeymapDefinition, ModeTransition, MotionDirection, MotionIntent,
    PendingSequenceDisplay, ResolvedKeyResult, WordStyleName,
};
use crate::bindings::{
    build_modal_machine, default_keymaps, from_modalkit_mode, is_displayable_partial_match,
};

pub struct GlideModalEngine {
    modal_machine: VimMachine<TerminalKey, GlideApplicationInfo>,
    current_mode: GlideMode,
    custom_keymaps: Vec<KeymapDefinition>,
    pending_sequence: KeySequence,
}

impl Default for GlideModalEngine {
    fn default() -> Self {
        Self {
            modal_machine: build_modal_machine(&[]),
            current_mode: GlideMode::Normal,
            custom_keymaps: Vec::new(),
            pending_sequence: KeySequence::new(),
        }
    }
}

impl GlideModalEngine {
    pub fn current_mode(&self) -> GlideMode {
        self.current_mode
    }

    pub fn current_sequence(&self) -> &[String] {
        &self.pending_sequence
    }

    pub fn set_mapping(&mut self, keymap_definition: KeymapDefinition) {
        // Replace any existing mapping for the same mode + sequence so that
        // re-binding a key updates rather than accumulating duplicates.
        self.custom_keymaps.retain(|existing| {
            !(existing.mode == keymap_definition.mode
                && existing.sequence == keymap_definition.sequence)
        });
        self.custom_keymaps.push(keymap_definition);
        self.rebuild_modal_machine();
    }

    pub fn del_mapping(&mut self, mode: GlideMode, sequence: &[String], buffer: bool) {
        self.custom_keymaps.retain(|existing| {
            !(existing.mode == mode
                && existing.sequence.as_slice() == sequence
                && existing.buffer == buffer)
        });
        self.rebuild_modal_machine();
    }

    pub fn clear_buffer(&mut self) {
        self.custom_keymaps.retain(|existing| !existing.buffer);
        self.rebuild_modal_machine();
    }

    pub fn list_mappings(&self, mode: GlideMode) -> Vec<KeymapDefinition> {
        default_keymaps()
            .into_iter()
            .chain(self.custom_keymaps.iter().cloned())
            .filter(|keymap_definition| keymap_definition.mode == mode)
            .collect()
    }

    pub fn reset_sequence(&mut self) {
        self.pending_sequence.clear();
    }

    /// Force the active mode, e.g. when the command bar opens or hints engage.
    ///
    /// The modalkit vim machine only models the vim-native modes, so the
    /// Glide-managed modes (command/hint/ignore) are tracked here and resolved
    /// outside of modalkit by [`Self::resolve_in_managed_mode`].
    pub fn set_mode(&mut self, mode: GlideMode) {
        self.current_mode = mode;
        self.pending_sequence.clear();
    }

    pub fn resolve_key_notation(&mut self, key_notation: &str) -> ResolvedKeyResult {
        let previous_mode = self.current_mode;

        // Glide-managed modes don't have a faithful modalkit representation, so
        // we resolve their mappings with a small prefix matcher while still
        // keeping Rust the single authority over modal state.
        if is_glide_managed_mode(previous_mode) {
            return self.resolve_in_managed_mode(previous_mode, key_notation);
        }

        self.pending_sequence.push(key_notation.to_string());

        let Ok(terminal_key) = key_notation.parse::<TerminalKey>() else {
            self.pending_sequence.clear();
            return ResolvedKeyResult::default();
        };

        self.modal_machine.input_key(terminal_key);

        let mut resolved_key_result = ResolvedKeyResult::default();
        let mut emitted_any_action = false;
        let mut requested_mode: Option<GlideMode> = None;

        while let Some((action, edit_context)) = self.modal_machine.pop() {
            match action {
                Action::Repeat(repeat_type) => {
                    self.modal_machine.repeat(repeat_type, Some(edit_context));
                }
                other_action => {
                    emitted_any_action = true;
                    self.translate_action(
                        previous_mode,
                        &other_action,
                        &mut resolved_key_result,
                        &mut requested_mode,
                    );
                }
            }
        }

        let modal_machine_mode = from_modalkit_mode(self.modal_machine.mode());
        let next_mode = if let Some(requested) = requested_mode {
            // An explicit `ChangeMode` mapping is authoritative, even when the
            // target is a Glide-managed mode that modalkit folds into Normal.
            requested
        } else if previous_mode == GlideMode::Normal
            && !emitted_any_action
            && is_operator_pending_prefix(self.pending_sequence.as_slice())
        {
            GlideMode::OperatorPending
        } else {
            modal_machine_mode
        };

        self.current_mode = next_mode;
        if previous_mode != next_mode {
            resolved_key_result.mode_transition = Some(ModeTransition {
                previous_mode,
                next_mode,
            });
        }

        let has_displayable_partial_match = is_displayable_partial_match(
            next_mode,
            self.pending_sequence.as_slice(),
            &self.custom_keymaps,
        );

        let should_preserve_pending_sequence =
            has_displayable_partial_match || next_mode == GlideMode::OperatorPending;

        if should_preserve_pending_sequence {
            resolved_key_result.default_prevented = true;
            resolved_key_result.has_partial_match = has_displayable_partial_match;
            resolved_key_result.pending_sequence_display = PendingSequenceDisplay {
                key_notations: self.pending_sequence.clone(),
            };
        } else {
            self.pending_sequence.clear();
            resolved_key_result.pending_sequence_display = PendingSequenceDisplay {
                key_notations: KeySequence::new(),
            };
        }

        if emitted_any_action || resolved_key_result.mode_transition.is_some() {
            resolved_key_result.default_prevented = true;
            resolved_key_result.matched_mapping = true;
        }

        resolved_key_result
    }

    fn rebuild_modal_machine(&mut self) {
        self.modal_machine = build_modal_machine(&self.custom_keymaps);
        self.current_mode = GlideMode::Normal;
        self.pending_sequence.clear();
    }

    /// All mappings that apply in `mode`, custom mappings taking precedence over
    /// built-ins so that user overrides win.
    fn mode_keymaps(&self, mode: GlideMode) -> Vec<KeymapDefinition> {
        self.custom_keymaps
            .iter()
            .cloned()
            .chain(default_keymaps())
            .filter(|keymap_definition| keymap_definition.mode == mode)
            .collect()
    }

    /// Resolve a key in a Glide-managed mode (command/hint/ignore) using a small
    /// prefix matcher, since these modes have no modalkit representation.
    fn resolve_in_managed_mode(
        &mut self,
        previous_mode: GlideMode,
        key_notation: &str,
    ) -> ResolvedKeyResult {
        self.pending_sequence.push(key_notation.to_string());
        let pending = self.pending_sequence.clone();
        let keymaps = self.mode_keymaps(previous_mode);

        if let Some(keymap_definition) = keymaps
            .iter()
            .find(|keymap_definition| keymap_definition.sequence == pending)
        {
            let mut resolved_key_result = ResolvedKeyResult::default();
            self.apply_engine_command(
                previous_mode,
                &keymap_definition.command,
                &mut resolved_key_result,
            );
            self.pending_sequence.clear();
            resolved_key_result.default_prevented = true;
            resolved_key_result.matched_mapping = true;
            return resolved_key_result;
        }

        let has_partial_match = keymaps.iter().any(|keymap_definition| {
            keymap_definition.sequence.len() > pending.len()
                && keymap_definition.sequence.starts_with(pending.as_slice())
        });

        let mut resolved_key_result = ResolvedKeyResult::default();
        if has_partial_match {
            resolved_key_result.default_prevented = true;
            resolved_key_result.has_partial_match = true;
            resolved_key_result.pending_sequence_display = PendingSequenceDisplay {
                key_notations: pending,
            };
        } else {
            self.pending_sequence.clear();
        }

        resolved_key_result
    }

    fn apply_engine_command(
        &mut self,
        previous_mode: GlideMode,
        command: &EngineCommand,
        resolved_key_result: &mut ResolvedKeyResult,
    ) {
        match command {
            EngineCommand::RepeatLastAction => {}
            EngineCommand::ChangeMode { request } => {
                let next_mode = request.target_mode;
                resolved_key_result.operator = request.pending_operator;
                if let Some(automatic_move_direction) = request.automatic_move_direction {
                    resolved_key_result.browser_command_intents.push(
                        BrowserCommandIntent::ApplyAutomaticMove {
                            automatic_move_direction,
                        },
                    );
                }

                self.current_mode = next_mode;
                resolved_key_result.mode_transition = Some(ModeTransition {
                    previous_mode,
                    next_mode,
                });
            }
            EngineCommand::DispatchBrowserCommand {
                command_name,
                arguments,
                ..
            } => {
                resolved_key_result.browser_command_intents.push(
                    BrowserCommandIntent::ExecuteBrowserCommand {
                        command_name: command_name.clone(),
                        arguments: arguments.clone(),
                    },
                );
            }
        }
    }

    fn translate_action(
        &self,
        previous_mode: GlideMode,
        action: &Action<GlideApplicationInfo>,
        resolved_key_result: &mut ResolvedKeyResult,
        requested_mode: &mut Option<GlideMode>,
    ) {
        match action {
            Action::NoOp => {}
            Action::Application(application_action) => {
                self.translate_application_action(
                    application_action,
                    resolved_key_result,
                    requested_mode,
                );
            }
            Action::CommandBar(CommandBarAction::Focus(prompt_prefix, command_type, _)) => {
                resolved_key_result.browser_command_intents.push(
                    BrowserCommandIntent::OpenCommandBar {
                        prompt_prefix: prompt_prefix.clone(),
                        command_bar_kind: CommandBarKind::from(*command_type),
                    },
                );
            }
            Action::CommandBar(CommandBarAction::Unfocus) => {}
            Action::Editor(EditorAction::Edit(specifier, edit_target)) => {
                if previous_mode == GlideMode::Insert
                    && matches!(
                        edit_target,
                        EditTarget::Motion(
                            MoveType::Column(MoveDir1D::Previous, false),
                            Count::Exact(1)
                        )
                    )
                {
                    resolved_key_result.browser_command_intents.push(
                        BrowserCommandIntent::ApplyAutomaticMove {
                            automatic_move_direction: crate::actions::AutomaticMoveDirection::Left,
                        },
                    );
                    return;
                }

                resolved_key_result.browser_command_intents.push(
                    BrowserCommandIntent::ExecuteEditingAction {
                        editing_action: EditingActionIntent {
                            operation: editor_operation_intent(specifier),
                            target: edit_target_intent(edit_target),
                        },
                    },
                );
            }
            Action::Editor(EditorAction::History(HistoryAction::Checkpoint)) => {}
            Action::Editor(EditorAction::History(_)) => {}
            Action::Editor(EditorAction::Cursor(CursorAction::Split(_))) => {
                resolved_key_result.browser_command_intents.push(
                    BrowserCommandIntent::ApplyAutomaticMove {
                        automatic_move_direction: crate::actions::AutomaticMoveDirection::Left,
                    },
                );
            }
            Action::Editor(EditorAction::Cursor(_)) => {}
            Action::Editor(EditorAction::Selection(_)) => {}
            Action::Editor(EditorAction::InsertText(InsertTextAction::Type(
                specifier,
                _,
                Count::Exact(1),
            ))) => {
                if let Specifier::Exact(Char::Single(character)) = specifier {
                    resolved_key_result.browser_command_intents.push(
                        BrowserCommandIntent::InsertText {
                            text: character.to_string(),
                        },
                    );
                }
            }
            Action::Editor(EditorAction::InsertText(_)) => {}
            Action::Prompt(_) => {}
            Action::Command(_) => {}
            Action::Jump(_, _, _) => {}
            Action::KeywordLookup(_) => {}
            Action::Macro(_) => {}
            Action::RedrawScreen => {}
            Action::Scroll(_) => {}
            Action::Search(_, _) => {}
            Action::ShowInfoMessage(_) => {}
            Action::Suspend => {}
            Action::Tab(_) => {}
            Action::Window(_) => {}
            Action::Repeat(_) => {}
            _ => {}
        }
    }

    fn translate_application_action(
        &self,
        application_action: &GlideApplicationAction,
        resolved_key_result: &mut ResolvedKeyResult,
        requested_mode: &mut Option<GlideMode>,
    ) {
        match application_action {
            GlideApplicationAction::ChangeMode(mode_change_request) => {
                *requested_mode = Some(mode_change_request.target_mode);
                resolved_key_result.operator = mode_change_request.pending_operator;
                if let Some(automatic_move_direction) = mode_change_request.automatic_move_direction
                {
                    resolved_key_result.browser_command_intents.push(
                        BrowserCommandIntent::ApplyAutomaticMove {
                            automatic_move_direction,
                        },
                    );
                }
            }
            GlideApplicationAction::DispatchBrowserCommand {
                command_name,
                arguments,
                ..
            } => {
                resolved_key_result.browser_command_intents.push(
                    BrowserCommandIntent::ExecuteBrowserCommand {
                        command_name: command_name.clone(),
                        arguments: arguments.clone(),
                    },
                );
            }
        }
    }
}

fn editor_operation_intent(specifier: &Specifier<EditAction>) -> EditorOperationIntent {
    match specifier {
        Specifier::Contextual => EditorOperationIntent::RawDescription {
            description: "contextual".into(),
        },
        Specifier::Exact(edit_action) => match edit_action {
            EditAction::Motion => EditorOperationIntent::Motion,
            EditAction::Delete => EditorOperationIntent::Delete,
            EditAction::Yank => EditorOperationIntent::Yank,
            EditAction::Replace(_) => EditorOperationIntent::Replace,
            other_action => EditorOperationIntent::RawDescription {
                description: format!("{other_action:?}"),
            },
        },
    }
}

fn edit_target_intent(edit_target: &EditTarget) -> EditTargetIntent {
    match edit_target {
        EditTarget::CurrentPosition => EditTargetIntent::CurrentPosition,
        EditTarget::Selection => EditTargetIntent::CurrentSelection,
        EditTarget::Range(RangeType::Line, include_line_break, count) => {
            EditTargetIntent::LineRange {
                count: count_to_u32(count),
                include_line_break: *include_line_break,
            }
        }
        EditTarget::Motion(move_type, count) => EditTargetIntent::Motion {
            motion: motion_intent(move_type),
            count: count_to_u32(count),
        },
        other_target => EditTargetIntent::RawDescription {
            description: format!("{other_target:?}"),
        },
    }
}

fn motion_intent(move_type: &MoveType) -> MotionIntent {
    match move_type {
        MoveType::Column(direction, wrap) => MotionIntent::Column {
            direction: motion_direction(*direction),
            wrap: *wrap,
        },
        MoveType::WordBegin(word_style, direction) => MotionIntent::WordBegin {
            direction: motion_direction(*direction),
            word_style: word_style_name(word_style.clone()),
        },
        other_motion => MotionIntent::RawDescription {
            description: format!("{other_motion:?}"),
        },
    }
}

fn motion_direction(direction: MoveDir1D) -> MotionDirection {
    match direction {
        MoveDir1D::Previous => MotionDirection::Previous,
        MoveDir1D::Next => MotionDirection::Next,
    }
}

fn word_style_name(word_style: WordStyle) -> WordStyleName {
    match word_style {
        WordStyle::Little => WordStyleName::Little,
        WordStyle::Big => WordStyleName::Big,
        WordStyle::AlphaNum => WordStyleName::Keyword,
        _ => WordStyleName::NonAlphanumeric,
    }
}

fn count_to_u32(count: &Count) -> u32 {
    match count {
        Count::Contextual => 1,
        Count::Exact(exact_count) => *exact_count as u32,
        Count::MinusOne => 1,
    }
}

fn is_glide_managed_mode(mode: GlideMode) -> bool {
    matches!(
        mode,
        GlideMode::Command | GlideMode::Hint | GlideMode::Ignore
    )
}

fn is_operator_pending_prefix(pending_sequence: &[String]) -> bool {
    matches!(
        pending_sequence,
        [single_key]
            if single_key == "d" || single_key == "c" || single_key == "y"
    )
}
