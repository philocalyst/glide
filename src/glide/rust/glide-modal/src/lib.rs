pub mod actions;
pub mod bindings;
pub mod bridge;
pub mod engine;

uniffi::setup_scaffolding!();

#[cfg(test)]
mod tests {
    use crate::actions::{
        AutomaticMoveDirection, BrowserCommandIntent, EditTargetIntent, EditingActionIntent,
        EditorOperationIntent, EngineCommand, GlideMode, KeymapDefinition, ModeChangeRequest,
        MotionDirection, MotionIntent, RangeTargetIntent, WordStyleName,
    };
    use crate::bridge::GlideModalBridge;

    /// Build a `DispatchBrowserCommand` keymap, mirroring how the JS layer
    /// registers Glide's default bindings via `glide.keymaps.set`.
    fn dispatch_keymap(mode: GlideMode, sequence: &[&str], command_name: &str) -> KeymapDefinition {
        KeymapDefinition {
            mode,
            sequence: sequence.iter().map(|key| key.to_string()).collect(),
            command: EngineCommand::DispatchBrowserCommand {
                command_name: command_name.into(),
                arguments: vec![],
                is_repeatable: false,
            },
            retain_key_display: false,
            buffer: false,
            description: None,
        }
    }

    /// Build a `ChangeMode` keymap, mirroring a JS-registered `mode_change …`.
    fn change_mode_keymap(
        mode: GlideMode,
        sequence: &[&str],
        target_mode: GlideMode,
    ) -> KeymapDefinition {
        KeymapDefinition {
            mode,
            sequence: sequence.iter().map(|key| key.to_string()).collect(),
            command: EngineCommand::ChangeMode {
                request: ModeChangeRequest {
                    target_mode,
                    pending_operator: None,
                    automatic_move_direction: None,
                },
            },
            retain_key_display: false,
            buffer: false,
            description: None,
        }
    }

    #[test]
    fn registered_insert_escape_mapping_resolves() {
        let bridge = GlideModalBridge::default();
        bridge.set_keymap(change_mode_keymap(
            GlideMode::Insert,
            &["j", "j"],
            GlideMode::Normal,
        ));

        let _ = bridge.resolve_key_notation("i".into());
        let first = bridge.resolve_key_notation("j".into());
        assert!(first.has_partial_match);
        let second = bridge.resolve_key_notation("j".into());
        assert!(second.matched_mapping);
        assert_eq!(bridge.current_mode_name(), GlideMode::Normal.as_str());
    }

    #[test]
    fn operator_pending_resolves_modalkit_motion_action() {
        let bridge = GlideModalBridge::default();
        let operator_result = bridge.resolve_key_notation("d".into());
        assert_eq!(
            operator_result.mode_transition.unwrap().next_mode,
            GlideMode::OperatorPending
        );
        assert_eq!(
            operator_result
                .pending_sequence_display
                .key_notations
                .as_slice(),
            ["d".to_string()]
        );

        let motion_result = bridge.resolve_key_notation("w".into());
        assert!(motion_result.matched_mapping);
        assert_eq!(
            motion_result.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteEditingAction {
                editing_action: EditingActionIntent {
                    // `d` set the pending operator in modalkit's context, so the
                    // contextual operator resolves to a concrete `Delete`.
                    operation: EditorOperationIntent::Delete,
                    target: EditTargetIntent::Motion {
                        motion: MotionIntent::WordBegin {
                            direction: MotionDirection::Next,
                            word_style: WordStyleName::Little,
                        },
                        count: 1,
                    },
                },
            }]
        );
        assert_eq!(bridge.current_mode_name(), GlideMode::Normal.as_str());
    }

    #[test]
    fn change_operator_emits_delete_and_enters_insert() {
        // `cw` is modelled as a `Delete` edit plus a transition into insert mode
        // — there is no distinct `Change` edit action. The JS layer recovers the
        // "change" intent from the insert-mode transition. Note modalkit also
        // implements the vim `cw`-acts-like-`ce` special case natively, emitting
        // `WordEnd` (operate to the end of the word) rather than `WordBegin`.
        let bridge = GlideModalBridge::default();
        let _ = bridge.resolve_key_notation("c".into());
        let result = bridge.resolve_key_notation("w".into());

        assert_eq!(
            result.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteEditingAction {
                editing_action: EditingActionIntent {
                    operation: EditorOperationIntent::Delete,
                    target: EditTargetIntent::Motion {
                        motion: MotionIntent::WordEnd {
                            direction: MotionDirection::Next,
                            word_style: WordStyleName::Little,
                        },
                        count: 1,
                    },
                },
            }]
        );
        assert_eq!(
            result.mode_transition.unwrap().next_mode,
            GlideMode::Insert
        );
    }

    #[test]
    fn yank_operator_resolves_to_typed_yank() {
        let bridge = GlideModalBridge::default();
        let _ = bridge.resolve_key_notation("y".into());
        let result = bridge.resolve_key_notation("w".into());

        assert_eq!(
            result.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteEditingAction {
                editing_action: EditingActionIntent {
                    operation: EditorOperationIntent::Yank,
                    target: EditTargetIntent::Motion {
                        motion: MotionIntent::WordBegin {
                            direction: MotionDirection::Next,
                            word_style: WordStyleName::Little,
                        },
                        count: 1,
                    },
                },
            }]
        );
        assert_eq!(bridge.current_mode_name(), GlideMode::Normal.as_str());
    }

    #[test]
    fn count_prefix_flows_into_motion_target() {
        let bridge = GlideModalBridge::default();
        let _ = bridge.resolve_key_notation("3".into());
        let result = bridge.resolve_key_notation("w".into());

        assert_eq!(
            result.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteEditingAction {
                editing_action: EditingActionIntent {
                    // No operator pending, so the contextual operator resolves to
                    // the default `Motion` (a bare `3w` caret move).
                    operation: EditorOperationIntent::Motion,
                    target: EditTargetIntent::Motion {
                        motion: MotionIntent::WordBegin {
                            direction: MotionDirection::Next,
                            word_style: WordStyleName::Little,
                        },
                        count: 3,
                    },
                },
            }]
        );
    }

    #[test]
    fn operator_with_line_end_motion_translates_to_typed_intent() {
        let bridge = GlideModalBridge::default();
        let _ = bridge.resolve_key_notation("d".into());
        let result = bridge.resolve_key_notation("$".into());

        // `$` is `Count::MinusOne` in modalkit (vim: end of line, count-1 lines
        // down), so a bare `$` resolves to 0 — i.e. the current line's end.
        assert_eq!(
            result.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteEditingAction {
                editing_action: EditingActionIntent {
                    operation: EditorOperationIntent::Delete,
                    target: EditTargetIntent::Motion {
                        motion: MotionIntent::LineEnd,
                        count: 0,
                    },
                },
            }]
        );
    }

    #[test]
    fn text_object_inside_word_translates_to_typed_range() {
        // `diw` should produce a typed Range(Word(Little)) target. The JS
        // op-pending `iw` registration preempts modalkit's built-in, so clear
        // the buffer first to exercise the modalkit-native path.
        let bridge = GlideModalBridge::default();
        bridge.clear_buffer();
        let _ = bridge.resolve_key_notation("d".into());
        let _ = bridge.resolve_key_notation("i".into());
        let result = bridge.resolve_key_notation("w".into());

        assert_eq!(
            result.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteEditingAction {
                editing_action: EditingActionIntent {
                    operation: EditorOperationIntent::Delete,
                    target: EditTargetIntent::Range {
                        range: RangeTargetIntent::Word {
                            word_style: WordStyleName::Little,
                        },
                        inclusive: true,
                        count: 1,
                    },
                },
            }]
        );
    }

    #[test]
    fn text_object_inside_parens_translates_to_typed_range() {
        let bridge = GlideModalBridge::default();
        bridge.clear_buffer();
        let _ = bridge.resolve_key_notation("d".into());
        let _ = bridge.resolve_key_notation("i".into());
        let result = bridge.resolve_key_notation("(".into());

        assert_eq!(
            result.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteEditingAction {
                editing_action: EditingActionIntent {
                    operation: EditorOperationIntent::Delete,
                    target: EditTargetIntent::Range {
                        range: RangeTargetIntent::Bracketed {
                            left: "(".into(),
                            right: ")".into(),
                        },
                        inclusive: false,
                        count: 1,
                    },
                },
            }]
        );
    }

    #[test]
    fn text_object_around_quotes_translates_to_typed_range() {
        let bridge = GlideModalBridge::default();
        bridge.clear_buffer();
        let _ = bridge.resolve_key_notation("d".into());
        let _ = bridge.resolve_key_notation("a".into());
        let result = bridge.resolve_key_notation("\"".into());

        assert_eq!(
            result.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteEditingAction {
                editing_action: EditingActionIntent {
                    operation: EditorOperationIntent::Delete,
                    target: EditTargetIntent::Range {
                        range: RangeTargetIntent::Quote { quote: "\"".into() },
                        inclusive: true,
                        count: 1,
                    },
                },
            }]
        );
    }

    #[test]
    fn repeat_replays_last_modalkit_edit_sequence() {
        let bridge = GlideModalBridge::default();
        let _ = bridge.resolve_key_notation("d".into());
        let _ = bridge.resolve_key_notation("w".into());

        let repeated = bridge.resolve_key_notation(".".into());
        assert_eq!(
            repeated.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteEditingAction {
                editing_action: EditingActionIntent {
                    operation: EditorOperationIntent::Delete,
                    target: EditTargetIntent::Motion {
                        motion: MotionIntent::WordBegin {
                            direction: MotionDirection::Next,
                            word_style: WordStyleName::Little,
                        },
                        count: 1,
                    },
                },
            }]
        );
    }

    #[test]
    fn insert_entry_emits_typed_automatic_move_intent() {
        let bridge = GlideModalBridge::default();
        let resolved = bridge.resolve_key_notation("i".into());

        assert_eq!(
            resolved.browser_command_intents,
            vec![BrowserCommandIntent::ApplyAutomaticMove {
                automatic_move_direction: AutomaticMoveDirection::Left,
            }]
        );
        assert_eq!(bridge.current_mode_name(), GlideMode::Insert.as_str());
    }

    #[test]
    fn unmatched_key_replays_pending_insert_text_and_clears_partial_sequence_display() {
        let bridge = GlideModalBridge::default();
        bridge.set_keymap(change_mode_keymap(
            GlideMode::Insert,
            &["j", "j"],
            GlideMode::Normal,
        ));

        let _ = bridge.resolve_key_notation("i".into());
        let first = bridge.resolve_key_notation("j".into());
        assert!(first.has_partial_match);

        let second = bridge.resolve_key_notation("x".into());
        assert!(second.matched_mapping);
        assert!(!second.has_partial_match);
        assert!(second.pending_sequence_display.key_notations.is_empty());
        assert_eq!(
            second.browser_command_intents,
            vec![
                BrowserCommandIntent::InsertText { text: "j".into() },
                BrowserCommandIntent::InsertText { text: "x".into() },
            ]
        );
    }

    #[test]
    fn list_keymaps_returns_registered_mappings() {
        let bridge = GlideModalBridge::default();
        bridge.set_keymap(dispatch_keymap(GlideMode::Normal, &["g", "g"], "scroll_top"));
        bridge.set_keymap(dispatch_keymap(GlideMode::Normal, &["G"], "scroll_bottom"));

        let normal_mappings = bridge.list_keymaps(GlideMode::Normal);

        assert!(normal_mappings
            .iter()
            .any(|keymap_definition| keymap_definition
                .sequence
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice()
                == ["g", "g"]));
        assert!(normal_mappings
            .iter()
            .any(|keymap_definition| keymap_definition
                .sequence
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice()
                == ["G"]));
    }

    #[test]
    fn custom_mapping_uses_typed_browser_command_dispatch() {
        let bridge = GlideModalBridge::default();
        bridge.set_keymap(KeymapDefinition {
            mode: GlideMode::Normal,
            sequence: vec!["g".into(), "t".into()],
            command: EngineCommand::DispatchBrowserCommand {
                command_name: "tab_next".into(),
                arguments: vec![],
                is_repeatable: true,
            },
            retain_key_display: false,
            buffer: false,
            description: None,
        });

        let first = bridge.resolve_key_notation("g".into());
        assert!(first.has_partial_match);

        let second = bridge.resolve_key_notation("t".into());
        assert_eq!(
            second.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteBrowserCommand {
                command_name: "tab_next".into(),
                arguments: vec![],
            }]
        );

        let repeated = bridge.resolve_key_notation(".".into());
        assert_eq!(
            repeated.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteBrowserCommand {
                command_name: "tab_next".into(),
                arguments: vec![],
            }]
        );
    }

    #[test]
    fn default_vim_visual_entry_comes_from_modalkit_machine() {
        let bridge = GlideModalBridge::default();
        let resolved = bridge.resolve_key_notation("v".into());

        assert!(resolved.matched_mapping);
        assert_eq!(
            resolved.mode_transition.unwrap().next_mode,
            GlideMode::Visual
        );
        assert_eq!(bridge.current_mode_name(), GlideMode::Visual.as_str());
    }

    #[test]
    fn registered_colon_mapping_overrides_native_command_bar() {
        let bridge = GlideModalBridge::default();
        bridge.set_keymap(dispatch_keymap(GlideMode::Normal, &[":"], "commandline_show"));

        let resolved = bridge.resolve_key_notation(":".into());

        assert_eq!(
            resolved.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteBrowserCommand {
                command_name: "commandline_show".into(),
                arguments: vec![],
            }]
        );
    }

    #[test]
    fn registered_scroll_mapping_dispatches_scroll_command() {
        let bridge = GlideModalBridge::default();
        bridge.set_keymap(dispatch_keymap(GlideMode::Normal, &["g", "g"], "scroll_top"));

        let first = bridge.resolve_key_notation("g".into());
        assert!(first.has_partial_match);

        let second = bridge.resolve_key_notation("g".into());
        assert_eq!(
            second.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteBrowserCommand {
                command_name: "scroll_top".into(),
                arguments: vec![],
            }]
        );
    }
}
