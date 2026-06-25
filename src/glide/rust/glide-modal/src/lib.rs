pub mod actions;
pub mod bindings;
pub mod bridge;
pub mod editing;
pub mod engine;

uniffi::setup_scaffolding!();

#[cfg(test)]
mod tests {
    use crate::actions::{
        AutomaticMoveDirection, BrowserCommandIntent, EditTargetIntent, EditingActionIntent,
        EditorOperationIntent, EngineCommand, GlideMode, KeymapDefinition, MotionDirection,
        MotionIntent, WordStyleName,
    };
    use crate::bridge::GlideModalBridge;
    use crate::editing::{
        scalar_offset_to_utf16_offset, utf16_offset_to_scalar_offset, EditInstruction,
        EditPlanBehavior, EditorSelectionSnapshot, EditorSnapshot, MotionKind,
    };

    #[test]
    fn utf16_offsets_round_trip_for_unicode_text() {
        let text = "a😀z";
        let scalar_offset = utf16_offset_to_scalar_offset(text, 3);
        assert_eq!(scalar_offset, 2);
        assert_eq!(scalar_offset_to_utf16_offset(text, scalar_offset), 3);
    }

    #[test]
    fn inner_word_selects_unicode_word_boundaries() {
        let bridge = GlideModalBridge::default();
        let snapshot = EditorSnapshot {
            text: "hello 世界 there".into(),
            selection: EditorSelectionSnapshot {
                anchor_scalar_offset: 7,
                focus_scalar_offset: 7,
                is_collapsed: true,
            },
        };

        let plan = bridge
            .make_edit_plan(
                snapshot,
                MotionKind::InnerWord,
                EditPlanBehavior::MoveCaret,
            )
            .unwrap();
        assert_eq!(
            plan.instructions,
            vec![EditInstruction::SelectRange {
                anchor_scalar_offset: 7,
                focus_scalar_offset: 8,
            }]
        );
    }

    #[test]
    fn delete_line_selects_trailing_newline_when_present() {
        let bridge = GlideModalBridge::default();
        let snapshot = EditorSnapshot {
            text: "hello\nworld".into(),
            selection: EditorSelectionSnapshot {
                anchor_scalar_offset: 1,
                focus_scalar_offset: 1,
                is_collapsed: true,
            },
        };

        let plan = bridge
            .make_edit_plan(
                snapshot,
                MotionKind::DeleteLine,
                EditPlanBehavior::ExtendSelectionFromFocus,
            )
            .unwrap();
        assert_eq!(
            plan.instructions,
            vec![EditInstruction::SelectRange {
                anchor_scalar_offset: 1,
                focus_scalar_offset: 6,
            }]
        );
    }

    #[test]
    fn downward_motion_preserves_visual_column() {
        let bridge = GlideModalBridge::default();
        let snapshot = EditorSnapshot {
            text: "hello\nw\nworld".into(),
            selection: EditorSelectionSnapshot {
                anchor_scalar_offset: 4,
                focus_scalar_offset: 4,
                is_collapsed: true,
            },
        };

        let plan = bridge
            .make_edit_plan(snapshot, MotionKind::Down, EditPlanBehavior::MoveCaret)
            .unwrap();
        assert_eq!(
            plan.instructions,
            vec![EditInstruction::SelectRange {
                anchor_scalar_offset: 7,
                focus_scalar_offset: 7,
            }]
        );
    }

    #[test]
    fn operator_motion_extends_selection_from_focus() {
        let bridge = GlideModalBridge::default();
        let snapshot = EditorSnapshot {
            text: "alpha beta".into(),
            selection: EditorSelectionSnapshot {
                anchor_scalar_offset: 2,
                focus_scalar_offset: 2,
                is_collapsed: true,
            },
        };

        let plan = bridge
            .make_edit_plan(
                snapshot,
                MotionKind::WordForward,
                EditPlanBehavior::ExtendSelectionFromFocus,
            )
            .unwrap();
        assert_eq!(
            plan.instructions,
            vec![EditInstruction::SelectRange {
                anchor_scalar_offset: 2,
                focus_scalar_offset: 6,
            }]
        );
    }

    #[test]
    fn open_line_below_produces_insert_instruction() {
        let bridge = GlideModalBridge::default();
        let snapshot = EditorSnapshot {
            text: "alpha\nbeta".into(),
            selection: EditorSelectionSnapshot {
                anchor_scalar_offset: 1,
                focus_scalar_offset: 1,
                is_collapsed: true,
            },
        };

        let plan = bridge
            .make_edit_plan(
                snapshot,
                MotionKind::OpenLineBelow,
                EditPlanBehavior::MoveCaret,
            )
            .unwrap();
        assert_eq!(
            plan.instructions,
            vec![EditInstruction::InsertText {
                scalar_offset: 5,
                text: "\n".into(),
            }]
        );
    }

    #[test]
    fn default_engine_supports_insert_escape_mapping() {
        let bridge = GlideModalBridge::default();
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
                    operation: EditorOperationIntent::RawDescription {
                        description: "contextual".into(),
                    },
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
    fn repeat_replays_last_modalkit_edit_sequence() {
        let bridge = GlideModalBridge::default();
        let _ = bridge.resolve_key_notation("d".into());
        let _ = bridge.resolve_key_notation("w".into());

        let repeated = bridge.resolve_key_notation(".".into());
        assert_eq!(
            repeated.browser_command_intents,
            vec![BrowserCommandIntent::ExecuteEditingAction {
                editing_action: EditingActionIntent {
                    operation: EditorOperationIntent::RawDescription {
                        description: "contextual".into(),
                    },
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
    fn list_keymaps_includes_default_mappings() {
        let bridge = GlideModalBridge::default();
        let normal_mappings = bridge.list_keymaps(GlideMode::Normal);

        assert!(normal_mappings
            .iter()
            .any(|keymap_definition| keymap_definition
                .sequence
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice()
                == ["i"]));
        assert!(normal_mappings
            .iter()
            .any(|keymap_definition| keymap_definition
                .sequence
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                .as_slice()
                == ["."]));
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
    fn command_bar_entry_opens_with_colon_prefix() {
        let bridge = GlideModalBridge::default();
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
    fn builtin_scroll_overlay_dispatches_scroll_command() {
        let bridge = GlideModalBridge::default();
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
