use std::collections::HashMap;

use modalkit::actions::{
    Action, CommandBarAction, CursorAction, EditAction, EditorAction, InsertTextAction,
};
use modalkit::editing::context::{EditContext, Resolve};
use modalkit::env::vim::keybindings::VimMachine;
use modalkit::key::TerminalKey;
use modalkit::keybindings::BindingMachine;
use modalkit::prelude::{
    Char, Count, EditTarget, MoveDir1D, MovePosition, MoveType, RangeType, Specifier, WordStyle,
};

use crate::actions::{
    AutomaticMoveDirection, BrowserCommandIntent, CommandBarKind, EngineCommand, ExcmdInfo,
    ExcmdParseError, GlideApplicationAction, GlideApplicationInfo, GlideMode, InsertOp,
    Instruction, KeyDisposition, KeyEventInfo, KeySequence, KeymapDefinition, ModeTransition,
    ParsedExcmd, PendingSequenceDisplay, ResolvedKeyResult, WireEditingAction, WireEditingTarget,
};
use crate::bindings::{build_modal_machine, from_modalkit_mode, is_displayable_partial_match};

// ── Phase 6: built-in excmd registry ─────────────────────────────────────────

/// Private static metadata for a single excmd, using `&'static str` to avoid
/// heap allocation. Converted to [`ExcmdInfo`] (owned `String`s) on demand by
/// [`GlideModalEngine::excmd_registry`].
struct ExcmdMeta {
    name: &'static str,
    description: &'static str,
    content_flag: bool,
    repeatable: bool,
}

/// Static table of all built-in Glide excmds, ported from
/// `browser-excmds-registry.mts` → `GLIDE_EXCOMMANDS`.
const GLIDE_EXCMDS: &[ExcmdMeta] = &[
    // ----- navigation -------------------------------------------------------
    ExcmdMeta {
        name: "back",
        description: "Go back one page in history",
        content_flag: false,
        repeatable: true,
    },
    ExcmdMeta {
        name: "forward",
        description: "Go forward one page in history",
        content_flag: false,
        repeatable: true,
    },
    ExcmdMeta {
        name: "reload",
        description: "Reload the current page",
        content_flag: false,
        repeatable: true,
    },
    ExcmdMeta {
        name: "reload_hard",
        description: "Reload the current page, bypassing the cache",
        content_flag: false,
        repeatable: true,
    },
    // ----- application ------------------------------------------------------
    ExcmdMeta {
        name: "quit",
        description: "Close all windows",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "clear",
        description: "Clear all notifications",
        content_flag: false,
        repeatable: false,
    },
    // ----- options ----------------------------------------------------------
    ExcmdMeta {
        name: "set",
        description: "Set an option",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "profile_dir",
        description: "Show the current profile directory",
        content_flag: false,
        repeatable: false,
    },
    // ----- config -----------------------------------------------------------
    ExcmdMeta {
        name: "config_edit",
        description: "Open the config file in the default editor",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "config_path",
        description: "Show the config file path",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "config_reload",
        description: "Reload the config file",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "config_init",
        description: "Initialise a config dir with all the necessary setup",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "css_edit",
        description: "Open the userChrome.css file in the default editor",
        content_flag: false,
        repeatable: false,
    },
    // ----- dot-repeat -------------------------------------------------------
    ExcmdMeta {
        name: "repeat_command",
        description: "Re-run the last invoked non-edit excmd",
        content_flag: false,
        repeatable: false,
    },
    // ----- keymaps ----------------------------------------------------------
    ExcmdMeta {
        name: "map",
        description: "Show all mappings",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "unmap",
        description: "Remove a mapping from normal mode",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "nunmap",
        description: "Remove a mapping from normal mode",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "iunmap",
        description: "Remove a mapping from insert mode",
        content_flag: false,
        repeatable: false,
    },
    // ----- tabs -------------------------------------------------------------
    ExcmdMeta {
        name: "tab",
        description: "Switch to the given tab index",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "tab_new",
        description:
            "Create a new tab with the given URL or the default new tab page if not provided",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "tab_close",
        description: "Close the current tab",
        content_flag: false,
        repeatable: true,
    },
    ExcmdMeta {
        name: "tab_next",
        description: "Switch to the next tab, wrapping around if applicable",
        content_flag: false,
        repeatable: true,
    },
    ExcmdMeta {
        name: "tab_prev",
        description: "Switch to the previous tab, wrapping around if applicable",
        content_flag: false,
        repeatable: true,
    },
    ExcmdMeta {
        name: "tab_pin",
        description: "Pin the current tab, or the tab with the given ID",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "tab_unpin",
        description: "Unpin the current tab, or the tab with the given ID",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "tab_pin_toggle",
        description: "Pin or unpin the current tab",
        content_flag: false,
        repeatable: true,
    },
    ExcmdMeta {
        name: "tab_reopen",
        description: "Open the last closed tab",
        content_flag: false,
        repeatable: true,
    },
    ExcmdMeta {
        name: "tab_duplicate",
        description: "Duplicate the current tab",
        content_flag: false,
        repeatable: true,
    },
    // ----- command line -----------------------------------------------------
    ExcmdMeta {
        name: "commandline_show",
        description: "Show the commandline UI",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "commandline_toggle",
        description: "Toggle the commandline UI",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "commandline_focus_next",
        description: "Focus the next completion in the commandline",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "commandline_focus_back",
        description: "Focus the previous completion in the commandline",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "commandline_delete",
        description: "Delete the focused commandline completion",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "commandline_accept",
        description: "Accept the focused commandline completion",
        content_flag: false,
        repeatable: false,
    },
    // ----- clipboard --------------------------------------------------------
    ExcmdMeta {
        name: "url_yank",
        description: "Yank the URL of the current tab to the clipboard",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "echo",
        description: "Log the given arguments to the console",
        content_flag: false,
        repeatable: false,
    },
    // ----- URL navigation ---------------------------------------------------
    ExcmdMeta {
        name: "go_up",
        description: "Go up the URL hierarchy",
        content_flag: false,
        repeatable: true,
    },
    ExcmdMeta {
        name: "go_to_root",
        description: "Go to the root of the current URL",
        content_flag: false,
        repeatable: true,
    },
    ExcmdMeta {
        name: "go_next",
        description: "Follow the link labeled next or >",
        content_flag: false,
        repeatable: true,
    },
    ExcmdMeta {
        name: "go_previous",
        description: "Follow the link labeled previous or <",
        content_flag: false,
        repeatable: true,
    },
    // ----- jump list (extended; not yet in TS registry) ---------------------
    ExcmdMeta {
        name: "jumplist_back",
        description: "Jump back in the jump list",
        content_flag: false,
        repeatable: true,
    },
    ExcmdMeta {
        name: "jumplist_forward",
        description: "Jump forward in the jump list",
        content_flag: false,
        repeatable: true,
    },
    // ----- mode / caret -----------------------------------------------------
    ExcmdMeta {
        name: "mode_change",
        description: "Change the current mode",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "caret_move",
        description: "Move the text caret",
        content_flag: false,
        repeatable: false,
    },
    // ----- visual / selection -----------------------------------------------
    ExcmdMeta {
        name: "copy",
        description: "Copy text from an applicable context to the clipboard",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "visual_selection_copy",
        description: "Copy the currently selected text to the clipboard & change to normal mode",
        content_flag: false,
        repeatable: false,
    },
    // ----- help / REPL ------------------------------------------------------
    ExcmdMeta {
        name: "help",
        description: "Open the docs",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "tutor",
        description: "Open the Glide tutorial",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "repl",
        description: "Start the config REPL",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "keys",
        description: "Synthesize the given key sequence as if they were actually pressed",
        content_flag: false,
        repeatable: false,
    },
    // ----- scrolling --------------------------------------------------------
    ExcmdMeta {
        name: "scroll_top",
        description: "Scroll to the top of the window",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "scroll_bottom",
        description: "Scroll to the bottom of the window",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "scroll_page_down",
        description: "Scroll down by 1 page (the size of the viewport)",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "scroll_page_up",
        description: "Scroll up by 1 page (the size of the viewport)",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "scroll_half_page_down",
        description: "Scroll down by half a page (0.5 * the size of the viewport)",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "scroll_half_page_up",
        description: "Scroll up by half a page (0.5 * the size of the viewport)",
        content_flag: false,
        repeatable: false,
    },
    // ----- content-process commands -----------------------------------------
    ExcmdMeta {
        name: "blur",
        description: "Blur the active element",
        content_flag: true,
        repeatable: false,
    },
    ExcmdMeta {
        name: "focusinput",
        description: "Focus an input element based on the given filter",
        content_flag: true,
        repeatable: false,
    },
    ExcmdMeta {
        name: "hint",
        description: "Show hint labels for jumping to clickable elements",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "hints_remove",
        description: "Remove all hint labels and exit hint mode",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "motion",
        description: "Execute a given motion (internal)",
        content_flag: true,
        repeatable: false,
    },
    // ----- edit history -----------------------------------------------------
    ExcmdMeta {
        name: "undo",
        description: "Undo the most recent edit",
        content_flag: false,
        repeatable: false,
    },
    ExcmdMeta {
        name: "redo",
        description: "Redo the most recent undo",
        content_flag: false,
        repeatable: false,
    },
];

pub struct GlideModalEngine {
    modal_machine: VimMachine<TerminalKey, GlideApplicationInfo>,
    current_mode: GlideMode,
    custom_keymaps: Vec<KeymapDefinition>,
    pending_sequence: KeySequence,
    /// Map from custom mode name → caret-style u8 (mirrors `GlideCaretStyle`).
    custom_modes_caret: HashMap<String, u8>,
    /// When `Some`, a custom mode is active and Rust is parked in `Normal`.
    current_custom_mode: Option<String>,
    /// The last excmd that was marked repeatable and passed to
    /// [`Self::note_executed`]. Used by [`Self::repeat_last`] for dot-repeat.
    last_excmd: Option<ParsedExcmd>,
}

impl Default for GlideModalEngine {
    fn default() -> Self {
        Self {
            modal_machine: build_modal_machine(&[]),
            current_mode: GlideMode::Normal,
            custom_keymaps: Vec::new(),
            pending_sequence: KeySequence::new(),
            custom_modes_caret: HashMap::new(),
            current_custom_mode: None,
            last_excmd: None,
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

    // ----- custom mode management ------------------------------------------

    /// Register a custom mode with its caret style so the engine can resolve
    /// keys in it and report the caret style to the bridge.
    pub fn register_custom_mode(&mut self, mode_name: String, caret_style: u8) {
        self.custom_modes_caret.insert(mode_name, caret_style);
    }

    /// Activate a custom mode.  Rust parks itself in `Normal` so the modalkit
    /// machine stops matching built-in mode bindings; the custom mode name is
    /// recorded here so [`Self::resolve_key_notation`] routes keys correctly.
    pub fn set_custom_mode(&mut self, name: String) {
        self.current_custom_mode = Some(name);
        self.current_mode = GlideMode::Normal;
        self.pending_sequence.clear();
    }

    /// The currently active custom mode name, or `None` when a built-in mode is active.
    pub fn current_custom_mode(&self) -> Option<String> {
        self.current_custom_mode.clone()
    }

    /// Caret style for a custom mode, or `None` if the mode is unknown.
    pub fn custom_mode_caret_style(&self, mode_name: &str) -> Option<u8> {
        self.custom_modes_caret.get(mode_name).copied()
    }

    /// Store a custom-mode keymap.  Unlike [`Self::set_mapping`] this does
    /// **not** rebuild the modalkit machine — custom-mode keymaps live outside
    /// modalkit and are matched by [`Self::resolve_in_custom_mode`].
    pub fn set_custom_mapping(&mut self, keymap_definition: KeymapDefinition) {
        let custom_mode = keymap_definition
            .custom_mode
            .as_deref()
            .unwrap_or("")
            .to_owned();
        self.custom_keymaps.retain(|existing| {
            !(existing.custom_mode.as_deref() == Some(&custom_mode)
                && existing.sequence == keymap_definition.sequence)
        });
        self.custom_keymaps.push(keymap_definition);
    }

    /// Remove a custom-mode keymap by mode name + sequence.
    pub fn del_custom_mapping(&mut self, custom_mode_name: &str, sequence: &[String]) {
        self.custom_keymaps.retain(|existing| {
            !(existing.custom_mode.as_deref() == Some(custom_mode_name)
                && existing.sequence.as_slice() == sequence)
        });
    }

    // ----- built-in mapping management ------------------------------------

    pub fn set_mapping(&mut self, keymap_definition: KeymapDefinition) {
        // Replace any existing mapping for the same mode + sequence so that
        // re-binding a key updates rather than accumulating duplicates.
        self.custom_keymaps.retain(|existing| {
            !(existing.custom_mode.is_none()
                && existing.mode == keymap_definition.mode
                && existing.sequence == keymap_definition.sequence)
        });
        self.custom_keymaps.push(keymap_definition);
        self.rebuild_modal_machine();
    }

    pub fn del_mapping(&mut self, mode: GlideMode, sequence: &[String], buffer: bool) {
        self.custom_keymaps.retain(|existing| {
            !(existing.custom_mode.is_none()
                && existing.mode == mode
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
        self.custom_keymaps
            .iter()
            .cloned()
            .filter(|kd| kd.custom_mode.is_none() && kd.mode == mode)
            .collect()
    }

    pub fn reset_sequence(&mut self) {
        self.pending_sequence.clear();
    }

    // ----- Phase 6: excmd registry & dot-repeat tracking ------------------

    /// Return the full built-in excmd registry as owned records.
    ///
    /// Suitable for which-key introspection, `glide.keymaps.list`, and the
    /// command-line completions that need `content_flag` / `repeatable` info.
    pub fn excmd_registry(&self) -> Vec<ExcmdInfo> {
        GLIDE_EXCMDS
            .iter()
            .map(|m| ExcmdInfo {
                name: m.name.to_string(),
                description: m.description.to_string(),
                content_flag: m.content_flag,
                repeatable: m.repeatable,
            })
            .collect()
    }

    /// Tokenize `input` (e.g. `"tab_next"` or `"mode_change normal"`) into a
    /// [`ParsedExcmd`] and validate that the command name is in the built-in
    /// registry.
    ///
    /// Tokenization splits on ASCII whitespace; quoted strings are not
    /// special-cased (the JS layer handles rich shell-quoting via `Args`).
    pub fn parse_excmd(&self, input: &str) -> Result<ParsedExcmd, ExcmdParseError> {
        let mut parts = input.split_ascii_whitespace();
        let name = parts.next().ok_or(ExcmdParseError::EmptyInput)?.to_string();

        if !GLIDE_EXCMDS.iter().any(|m| m.name == name) {
            return Err(ExcmdParseError::UnknownCommand { name });
        }

        let arguments: Vec<String> = parts.map(|s| s.to_string()).collect();
        Ok(ParsedExcmd { name, arguments })
    }

    /// Record `parsed` as the most-recently executed excmd.
    ///
    /// Only stores the command when it is flagged as `repeatable` in the
    /// built-in registry; non-repeatable commands leave the previous value
    /// unchanged so that `repeat_last` keeps returning the last repeatable one.
    pub fn note_executed(&mut self, parsed: ParsedExcmd) {
        let repeatable = GLIDE_EXCMDS
            .iter()
            .find(|m| m.name == parsed.name)
            .map_or(false, |m| m.repeatable);
        if repeatable {
            self.last_excmd = Some(parsed);
        }
    }

    /// Return the last repeatable excmd that was passed to
    /// [`Self::note_executed`], or `None` when no repeatable excmd has been
    /// executed yet.
    pub fn repeat_last(&self) -> Option<ParsedExcmd> {
        self.last_excmd.clone()
    }

    /// Force the active mode, e.g. when the command bar opens or hints engage.
    ///
    /// The modalkit vim machine only models the vim-native modes, so the
    /// Glide-managed modes (command/hint/ignore) are tracked here and resolved
    /// outside of modalkit by [`Self::resolve_in_managed_mode`].
    /// Activating a built-in mode also clears any active custom mode.
    pub fn set_mode(&mut self, mode: GlideMode) {
        self.current_custom_mode = None;
        self.current_mode = mode;
        self.pending_sequence.clear();
    }

    pub fn resolve_key_notation(&mut self, key_notation: &str) -> ResolvedKeyResult {
        let previous_mode = self.current_mode;

        // Custom modes take priority: they are resolved with the same prefix
        // matcher used for Glide-managed modes, but keyed by the custom mode
        // name stored in `custom_mode` rather than the `GlideMode` enum value.
        if let Some(custom_mode_name) = self.current_custom_mode.clone() {
            return self.resolve_in_custom_mode(&custom_mode_name, key_notation);
        }

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
                // `.` — modalkit owns dot-repeat for edits: replaying the last
                // edit sequence re-emits its underlying actions, which the loop
                // then translates just like the original edit. (Non-edit excmds
                // are repeated separately by the JS `repeat_command`.)
                Action::Repeat(repeat_type) => {
                    self.modal_machine.repeat(repeat_type, Some(edit_context));
                }
                other_action => {
                    emitted_any_action = true;
                    self.translate_action(
                        previous_mode,
                        &other_action,
                        &edit_context,
                        &mut resolved_key_result,
                        &mut requested_mode,
                    );
                }
            }
        }

        let next_mode = if let Some(requested) = requested_mode {
            // An explicit `ChangeMode` mapping is authoritative, even when the
            // target is a Glide-managed mode that modalkit folds into Normal.
            requested
        } else if previous_mode == GlideMode::Normal
            && !emitted_any_action
            && is_operator_pending_prefix(self.pending_sequence.as_slice())
        {
            // modalkit doesn't surface operator-pending via `mode()` until the
            // motion completes, so detect a bare operator prefix (`d`/`c`/`y`)
            // ourselves to drive the op-pending caret/display immediately.
            GlideMode::OperatorPending
        } else {
            from_modalkit_mode(self.modal_machine.mode())
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

    // ----- Phase 5: process_key_event ------------------------------------

    /// Process a raw DOM key event end-to-end, returning a [`KeyDisposition`]
    /// that the JS layer executes verbatim.
    ///
    /// Returns `None` for modifier-only or dead keys that produce no typeable
    /// notation (same contract as [`crate::key::key_notation_from_event`]).
    pub fn process_key_event(&mut self, event: &KeyEventInfo) -> Option<KeyDisposition> {
        let notation = crate::key::key_notation_from_event(event)?;

        // Snapshot the pre-push sequence so we can reconstruct the full matched
        // sequence for `Instruction::Callback` (resolve_key_notation pushes and
        // potentially clears `pending_sequence` internally).
        let mode_before = self.current_mode;
        let mut matched_sequence = self.pending_sequence.clone();
        matched_sequence.push(notation.clone());

        let result = self.resolve_key_notation(&notation);

        Some(key_disposition_from_result(
            result,
            mode_before,
            matched_sequence,
        ))
    }

    fn rebuild_modal_machine(&mut self) {
        // Only feed built-in mode keymaps to modalkit; custom-mode keymaps are
        // resolved outside the state machine by `resolve_in_custom_mode`.
        let builtin_keymaps: Vec<KeymapDefinition> = self
            .custom_keymaps
            .iter()
            .cloned()
            .filter(|kd| kd.custom_mode.is_none())
            .collect();
        self.modal_machine = build_modal_machine(&builtin_keymaps);
        self.current_mode = GlideMode::Normal;
        self.pending_sequence.clear();
    }

    /// All built-in mode mappings that apply in `mode`.  Custom-mode keymaps
    /// (where `custom_mode.is_some()`) are excluded since they are not fed to
    /// the modalkit machine and have a separate resolution path.
    fn mode_keymaps(&self, mode: GlideMode) -> Vec<KeymapDefinition> {
        self.custom_keymaps
            .iter()
            .cloned()
            .filter(|kd| kd.custom_mode.is_none() && kd.mode == mode)
            .collect()
    }

    /// Resolve a key while a custom mode is active.  Uses the same prefix
    /// matcher as [`Self::resolve_in_managed_mode`] but filters keymaps by the
    /// `custom_mode` field rather than the `mode` enum value.
    fn resolve_in_custom_mode(
        &mut self,
        custom_mode_name: &str,
        key_notation: &str,
    ) -> ResolvedKeyResult {
        self.pending_sequence.push(key_notation.to_string());
        let pending = self.pending_sequence.clone();
        let keymaps: Vec<KeymapDefinition> = self
            .custom_keymaps
            .iter()
            .cloned()
            .filter(|kd| kd.custom_mode.as_deref() == Some(custom_mode_name))
            .collect();

        if let Some(keymap_definition) = keymaps.iter().find(|kd| kd.sequence == pending) {
            let mut resolved = ResolvedKeyResult::default();
            let previous_mode = self.current_mode;
            let command = keymap_definition.command.clone();
            self.apply_engine_command(previous_mode, &command, &mut resolved);
            self.pending_sequence.clear();
            resolved.default_prevented = true;
            resolved.matched_mapping = true;
            return resolved;
        }

        let has_partial = keymaps.iter().any(|kd| {
            kd.sequence.len() > pending.len() && kd.sequence.starts_with(pending.as_slice())
        });

        let mut resolved = ResolvedKeyResult::default();
        if has_partial {
            resolved.default_prevented = true;
            resolved.has_partial_match = true;
            resolved.pending_sequence_display = PendingSequenceDisplay {
                key_notations: pending,
            };
        } else {
            self.pending_sequence.clear();
        }
        resolved
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
            EngineCommand::ChangeMode { request } => {
                let next_mode = request.target_mode;
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
            } => {
                resolved_key_result.browser_command_intents.push(
                    BrowserCommandIntent::ExecuteBrowserCommand {
                        command_name: command_name.clone(),
                        arguments: arguments.clone(),
                    },
                );
            }
            EngineCommand::Callback { callback_id } => {
                resolved_key_result.matched_callback_id = Some(*callback_id);
            }
        }
    }

    fn translate_action(
        &self,
        previous_mode: GlideMode,
        action: &Action<GlideApplicationInfo>,
        edit_context: &EditContext,
        resolved_key_result: &mut ResolvedKeyResult,
        requested_mode: &mut Option<GlideMode>,
    ) {
        match action {
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
                        action: wire_editing_action(specifier, edit_target, edit_context),
                    },
                );
            }
            Action::Editor(EditorAction::Cursor(CursorAction::Split(_))) => {
                resolved_key_result.browser_command_intents.push(
                    BrowserCommandIntent::ApplyAutomaticMove {
                        automatic_move_direction: crate::actions::AutomaticMoveDirection::Left,
                    },
                );
            }
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
            Action::Editor(EditorAction::InsertText(InsertTextAction::OpenLine(
                _shape,
                direction,
                _count,
            ))) => {
                resolved_key_result
                    .browser_command_intents
                    .push(BrowserCommandIntent::OpenLine {
                        above: matches!(direction, MoveDir1D::Previous),
                    });
            }
            // Every other modalkit action (history, scroll, search, jumps,
            // tabs/windows, macros, prompts, repeat, …) has no Glide-side
            // effect — Glide only consumes edits, command-bar focus, insert
            // typing and the insert-entry cursor split.
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
            } => {
                resolved_key_result.browser_command_intents.push(
                    BrowserCommandIntent::ExecuteBrowserCommand {
                        command_name: command_name.clone(),
                        arguments: arguments.clone(),
                    },
                );
            }
            GlideApplicationAction::Callback { callback_id } => {
                resolved_key_result.matched_callback_id = Some(*callback_id);
            }
        }
    }
}

// ── modalkit action → flat wire target ───────────────────────────────────────

/// Build a [`WireEditingAction`] from a modalkit edit specifier + target.
///
/// The operator is resolved through the [`EditContext`] (so `dw` arrives as
/// `Delete`, `yw` as `Yank`); `c` (change) is a `Delete` plus an insert-mode
/// transition, so there is no distinct `change` operation. `r{char}` recovers
/// its replacement char from modalkit's `CharReplaceSuffix` submode.
fn wire_editing_action(
    specifier: &Specifier<EditAction>,
    edit_target: &EditTarget,
    edit_context: &EditContext,
) -> WireEditingAction {
    let (operation, character) = match edit_context.resolve(specifier) {
        EditAction::Motion => ("motion", None),
        EditAction::Delete => ("delete", None),
        EditAction::Yank => ("yank", None),
        EditAction::Replace(_) => match edit_context.get_replace_char() {
            Some(Char::Single(character)) => ("replace", Some(character.to_string())),
            // Digraphs / control sequences / a missing char aren't handled by
            // the content executor; fall back to the raw path.
            _ => ("raw", None),
        },
        _ => ("raw", None),
    };
    WireEditingAction {
        operation: operation.into(),
        character,
        target: wire_target(edit_target, edit_context),
    }
}

/// A bare [`WireEditingTarget`] carrying only `kind` + `count`; the caller fills
/// in the variant-specific fields via struct-update syntax.
fn target(kind: &str, count: u32) -> WireEditingTarget {
    WireEditingTarget {
        kind: kind.into(),
        count,
        ..Default::default()
    }
}

fn wire_target(edit_target: &EditTarget, ctx: &EditContext) -> WireEditingTarget {
    match edit_target {
        EditTarget::CurrentPosition => target("current-position", 0),
        EditTarget::Selection => target("current-selection", 0),
        EditTarget::Range(RangeType::Line, include_line_break, count) => WireEditingTarget {
            include_line_break: Some(*include_line_break),
            ..target("line-range", resolve_count(ctx, count))
        },
        EditTarget::Range(range_type, inclusive, count)
        | EditTarget::Boundary(range_type, inclusive, _, count) => {
            wire_range(range_type, *inclusive, resolve_count(ctx, count))
        }
        EditTarget::Motion(move_type, count) => wire_motion(move_type, resolve_count(ctx, count)),
        other => WireEditingTarget {
            description: Some(format!("{other:?}")),
            ..target("raw", 0)
        },
    }
}

fn wire_motion(move_type: &MoveType, count: u32) -> WireEditingTarget {
    let motion = |name: &str| WireEditingTarget {
        motion: Some(name.into()),
        ..target("motion", count)
    };
    match move_type {
        MoveType::Column(direction, wrap) => WireEditingTarget {
            direction: Some(dir_str(*direction)),
            wrap: Some(*wrap),
            ..motion("column")
        },
        MoveType::LinePos(MovePosition::Beginning) => motion("line-start"),
        MoveType::LinePos(MovePosition::End) => motion("line-end"),
        MoveType::FirstWord(direction) => WireEditingTarget {
            direction: Some(dir_str(*direction)),
            ..motion("first-word")
        },
        MoveType::Line(direction) => WireEditingTarget {
            direction: Some(dir_str(*direction)),
            ..motion("line")
        },
        MoveType::WordBegin(word_style, direction) => WireEditingTarget {
            direction: Some(dir_str(*direction)),
            word_style: Some(word_style_str(word_style)),
            ..motion("word-begin")
        },
        MoveType::WordEnd(word_style, direction) => WireEditingTarget {
            direction: Some(dir_str(*direction)),
            word_style: Some(word_style_str(word_style)),
            ..motion("word-end")
        },
        MoveType::ParagraphBegin(direction) => WireEditingTarget {
            direction: Some(dir_str(*direction)),
            ..motion("paragraph-begin")
        },
        other => WireEditingTarget {
            description: Some(format!("{other:?}")),
            ..motion("raw")
        },
    }
}

fn wire_range(range_type: &RangeType, inclusive: bool, count: u32) -> WireEditingTarget {
    let mut wire = WireEditingTarget {
        inclusive: Some(inclusive),
        ..target("range", count)
    };
    wire.range = Some(
        match range_type {
            RangeType::Word(word_style) => {
                wire.word_style = Some(word_style_str(word_style));
                "word"
            }
            RangeType::Bracketed(left, right) => {
                wire.left = Some(left.to_string());
                wire.right = Some(right.to_string());
                "bracketed"
            }
            RangeType::Quote(quote) => {
                wire.quote = Some(quote.to_string());
                "quote"
            }
            RangeType::XmlTag => "xml-tag",
            RangeType::Paragraph => "paragraph",
            RangeType::Sentence => "sentence",
            RangeType::Line => "line",
            RangeType::Buffer => "buffer",
            RangeType::Item => "item",
            _ => "sentence", // unreachable: RangeType is non_exhaustive
        }
        .into(),
    );
    wire
}

fn dir_str(direction: MoveDir1D) -> String {
    match direction {
        MoveDir1D::Previous => "previous",
        MoveDir1D::Next => "next",
    }
    .into()
}

fn word_style_str(word_style: &WordStyle) -> String {
    match word_style {
        WordStyle::Little => "little",
        WordStyle::Big => "big",
        WordStyle::AlphaNum => "keyword",
        _ => "non-alphanumeric",
    }
    .into()
}

/// Resolve a modalkit [`Count`] against the active [`EditContext`].
///
/// motion/operator counts are usually `Count::Contextual`, with the actual
/// numeric count (e.g. the `3` in `3w`) living on the context — so resolving
/// through the context is what makes counts work.
fn resolve_count(edit_context: &EditContext, count: &Count) -> u32 {
    let resolved: usize = edit_context.resolve(count);
    resolved as u32
}

fn is_glide_managed_mode(mode: GlideMode) -> bool {
    matches!(
        mode,
        GlideMode::Command | GlideMode::Hint | GlideMode::Ignore
    )
}

/// Whether a bare pending key puts modalkit into a "waiting for more input"
/// submode that its `mode()` doesn't surface: the operators `d`/`c`/`y` (await a
/// motion) and `r` (awaits the replacement char, via `CharReplaceSuffix`). Glide
/// drives the op-pending caret/display + swallows the key for all of them.
fn is_operator_pending_prefix(pending_sequence: &[String]) -> bool {
    matches!(
        pending_sequence,
        [single_key]
            if single_key == "d"
                || single_key == "c"
                || single_key == "y"
                || single_key == "r"
    )
}

// ── Phase 5 helpers ──────────────────────────────────────────────────────────

/// Convert an internal [`ResolvedKeyResult`] into the flat [`KeyDisposition`]
/// handed to JS.
fn key_disposition_from_result(
    result: ResolvedKeyResult,
    mode_before: GlideMode,
    matched_sequence: Vec<String>,
) -> KeyDisposition {
    KeyDisposition {
        prevent_default: result.default_prevented,
        instructions: build_instructions(&result, mode_before, &matched_sequence),
        sequence_display: result.pending_sequence_display.key_notations,
        mode_transition: result.mode_transition,
        matched_mapping: result.matched_mapping,
        has_partial_match: result.has_partial_match,
    }
}

/// Whether the resolved intents represent an insert-entry or dot-replay of an
/// insert session (`o`/`O`/`i`/`a`/`A`/`I`) — as opposed to plain typing while
/// already in insert, or a `cw`-style change that only transitions into insert.
fn is_insert_sequence(
    intents: &[BrowserCommandIntent],
    mode_before: GlideMode,
    enters_insert: bool,
) -> bool {
    if mode_before == GlideMode::Insert {
        // Live insert-mode typing also emits `InsertText`; only entries and
        // `.`-replays (which start from a non-insert mode) are bundled.
        return false;
    }
    let has = |f: fn(&BrowserCommandIntent) -> bool| intents.iter().any(f);
    has(|i| matches!(i, BrowserCommandIntent::OpenLine { .. }))
        || has(|i| matches!(i, BrowserCommandIntent::InsertText { .. }))
        || (enters_insert && has(|i| matches!(i, BrowserCommandIntent::ApplyAutomaticMove { .. })))
}

fn automove_str(direction: AutomaticMoveDirection) -> &'static str {
    match direction {
        AutomaticMoveDirection::Left => "left",
        AutomaticMoveDirection::EndOfLine => "endline",
    }
}

/// Build the ordered [`Instruction`] list for a resolved key result. The engine
/// makes every decision here so JS executes the result verbatim: the operator is
/// already resolved (incl. `change` recovery), the command bar is expressed as
/// its excmd, and insert-entries are bundled into one `insert-sequence`.
fn build_instructions(
    result: &ResolvedKeyResult,
    mode_before: GlideMode,
    matched_sequence: &[String],
) -> Vec<Instruction> {
    // A matched JS closure is dispatched by id; nothing else runs.
    if let Some(callback_id) = result.matched_callback_id {
        return vec![Instruction::callback(
            callback_id,
            matched_sequence.to_vec(),
        )];
    }

    let intents = &result.browser_command_intents;
    let enters_insert = result
        .mode_transition
        .as_ref()
        .map_or(false, |t| t.next_mode == GlideMode::Insert);

    if is_insert_sequence(intents, mode_before, enters_insert) {
        let ops = intents
            .iter()
            .filter_map(|intent| match intent {
                BrowserCommandIntent::OpenLine { above } => Some(InsertOp::open_line(*above)),
                BrowserCommandIntent::ApplyAutomaticMove {
                    automatic_move_direction,
                } => Some(InsertOp::automove(automove_str(*automatic_move_direction))),
                BrowserCommandIntent::InsertText { text } => {
                    Some(InsertOp::insert_text(text.clone()))
                }
                // Entry motion for `a`/`A`/`I` (Column / LineEnd / FirstWord).
                BrowserCommandIntent::ExecuteEditingAction { action }
                    if action.target.kind == "motion" =>
                {
                    Some(InsertOp::move_to(action.target.clone()))
                }
                _ => None,
            })
            .collect();
        return vec![Instruction::insert_sequence(ops, enters_insert)];
    }

    intents
        .iter()
        .filter_map(|intent| match intent {
            BrowserCommandIntent::ExecuteEditingAction { action } => {
                let mut action = action.clone();
                // `c` (change) reaches us as `Delete` + an insert transition;
                // recover the change intent so the executor drops into insert.
                if action.operation == "delete" && enters_insert {
                    action.operation = "change".into();
                }
                Some(Instruction::editing_action(action))
            }
            BrowserCommandIntent::ExecuteBrowserCommand {
                command_name,
                arguments,
            } => Some(Instruction::excmd(command_name.clone(), arguments.clone())),
            BrowserCommandIntent::OpenCommandBar { .. } => {
                Some(Instruction::excmd("commandline_show", vec![]))
            }
            // InsertText / ApplyAutomaticMove / OpenLine outside an insert
            // sequence are live-typing pass-throughs with no parent instruction.
            _ => None,
        })
        .collect()
}
