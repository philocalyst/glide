use std::sync::Mutex;

use crate::actions::{
    ExcmdInfo, ExcmdParseError, GlideMode, KeyDisposition, KeyEventInfo, KeymapDefinition,
    ParsedExcmd, ResolvedKeyResult,
};
use crate::engine::GlideModalEngine;

/// The single authority over Glide's modal state.
///
/// All mode tracking, key-sequence matching and edit planning lives here; the
/// JavaScript layer is a thin adapter that forwards key notations and applies
/// the resulting intents. The engine is wrapped in a [`Mutex`] so the object can
/// be shared across the FFI boundary while still mutating internal state.
#[derive(Default, uniffi::Object)]
pub struct GlideModalBridge {
    engine: Mutex<GlideModalEngine>,
}

#[uniffi::export]
impl GlideModalBridge {
    #[uniffi::constructor]
    pub fn new() -> Self {
        Self::default()
    }

    /// The wire-format name of the currently active mode (e.g. `"normal"`).
    pub fn current_mode_name(&self) -> String {
        self.engine.lock().unwrap().current_mode().as_str().into()
    }

    /// Force the engine into `mode`, keeping Rust authoritative when a mode
    /// change originates outside the key pipeline (command bar, hints, sandbox).
    pub fn set_mode(&self, mode: GlideMode) {
        self.engine.lock().unwrap().set_mode(mode);
    }

    /// The pending (partially matched) key sequence, in notation form.
    pub fn current_sequence(&self) -> Vec<String> {
        self.engine.lock().unwrap().current_sequence().to_vec()
    }

    /// Clear any in-progress key sequence.
    pub fn reset_sequence(&self) {
        self.engine.lock().unwrap().reset_sequence();
    }

    /// Caret style enum value for `mode` (see `GlideCaretStyle`).
    pub fn mode_caret_style(&self, mode: GlideMode) -> u8 {
        mode.caret_style()
    }

    /// All built-in mode names, in declaration order.
    pub fn mode_names(&self) -> Vec<String> {
        GlideMode::all().into_iter().map(|m| m.as_str().into()).collect()
    }

    pub fn list_keymaps(&self, mode: GlideMode) -> Vec<KeymapDefinition> {
        self.engine.lock().unwrap().list_mappings(mode)
    }

    pub fn set_keymap(&self, keymap_definition: KeymapDefinition) {
        self.engine.lock().unwrap().set_mapping(keymap_definition);
    }

    pub fn del_keymap(&self, mode: GlideMode, sequence: Vec<String>, buffer: bool) {
        self.engine.lock().unwrap().del_mapping(mode, &sequence, buffer);
    }

    /// Remove all buffer-local mappings (e.g. on navigation).
    pub fn clear_buffer(&self) {
        self.engine.lock().unwrap().clear_buffer();
    }

    pub fn resolve_key_notation(&self, key_notation: String) -> ResolvedKeyResult {
        self.engine.lock().unwrap().resolve_key_notation(&key_notation)
    }

    /// Convert a raw DOM keyboard event into Vim-style key notation.
    ///
    /// This is the Rust port of `Keys.event_to_key_notation` from
    /// `utils/keys.mts`.  Returns `None` for modifier-only keypresses
    /// (`Shift`, `Control`, …) and dead keys that Glide cannot handle.
    ///
    /// The JS layer should call this instead of `Keys.event_to_key_notation`
    /// and skip processing when `None` is returned.
    pub fn key_notation_from_event(&self, event: KeyEventInfo) -> Option<String> {
        crate::key::key_notation_from_event(&event)
    }

    // ----- custom mode API -------------------------------------------------

    /// Register a custom mode so the engine can resolve keys in it.
    /// `caret_style` must match a `GlideCaretStyle` value (0=block, 1=underline,
    /// 2=line).
    pub fn register_custom_mode(&self, mode_name: String, caret_style: u8) {
        self.engine
            .lock()
            .unwrap()
            .register_custom_mode(mode_name, caret_style);
    }

    /// Activate a custom mode.  Rust parks itself in `Normal` internally; the
    /// custom mode name governs key resolution until a built-in mode is set.
    pub fn set_custom_mode(&self, mode_name: String) {
        self.engine.lock().unwrap().set_custom_mode(mode_name);
    }

    /// The currently active custom mode name, or `None` when a built-in mode is
    /// active.
    pub fn current_custom_mode(&self) -> Option<String> {
        self.engine.lock().unwrap().current_custom_mode()
    }

    /// Caret style for a custom mode (see `GlideCaretStyle`), or `None` if the
    /// mode has not been registered.
    pub fn custom_mode_caret_style(&self, mode_name: String) -> Option<u8> {
        self.engine
            .lock()
            .unwrap()
            .custom_mode_caret_style(&mode_name)
    }

    /// Register a keymap for a custom mode.  The `KeymapDefinition.custom_mode`
    /// field must be `Some(mode_name)` and the `mode` field is treated as an
    /// ignored sentinel.  The mapping is resolved by the engine's prefix matcher
    /// rather than the modalkit state machine.
    pub fn set_custom_keymap(&self, keymap_definition: KeymapDefinition) {
        self.engine
            .lock()
            .unwrap()
            .set_custom_mapping(keymap_definition);
    }

    /// Remove a custom-mode keymap by mode name and sequence.
    pub fn del_custom_keymap(&self, mode_name: String, sequence: Vec<String>) {
        self.engine
            .lock()
            .unwrap()
            .del_custom_mapping(&mode_name, &sequence);
    }

    // ----- Phase 5: process_key -------------------------------------------

    /// Process a raw DOM key event through the modal engine, returning a rich
    /// [`KeyDisposition`] that the JS layer executes verbatim.
    ///
    /// This replaces the two-step `key_notation_from_event` →
    /// `resolve_key_notation` pipeline: the JS side now hands the raw event
    /// fields directly and receives a complete execution plan.
    ///
    /// Returns `None` for modifier-only or dead keys (same cases where
    /// `key_notation_from_event` would return `None`), allowing the JS caller
    /// to short-circuit with a simple null-check.
    pub fn process_key(&self, event: KeyEventInfo) -> Option<KeyDisposition> {
        self.engine.lock().unwrap().process_key_event(&event)
    }

    // ----- Phase 6: excmd registry & dot-repeat tracking ------------------

    /// Return the full built-in excmd registry.
    ///
    /// Each entry includes the excmd name, description, whether it runs in the
    /// content process (`content_flag`), and whether it can be dot-repeated
    /// (`repeatable`).  Useful for which-key introspection, `glide.keymaps.list`,
    /// and command-line completion.
    pub fn excmd_registry(&self) -> Vec<ExcmdInfo> {
        self.engine.lock().unwrap().excmd_registry()
    }

    /// Tokenize `input` (e.g. `"tab_next"` or `"mode_change normal"`) into a
    /// [`ParsedExcmd`] containing the command name and positional arguments.
    ///
    /// Returns `Err` when the input is empty or the command name is not in the
    /// built-in excmd registry.  User-defined excmds registered on the JS side
    /// are not validated here.
    pub fn parse_excmd(&self, input: String) -> Result<ParsedExcmd, ExcmdParseError> {
        self.engine.lock().unwrap().parse_excmd(&input)
    }

    /// Record `parsed` as the most-recently executed excmd for dot-repeat.
    ///
    /// Only excmds flagged as `repeatable` in the registry are stored; calls
    /// for non-repeatable commands are silently ignored so that
    /// [`Self::repeat_last`] continues to return the last repeatable one.
    pub fn note_executed(&self, parsed: ParsedExcmd) {
        self.engine.lock().unwrap().note_executed(parsed);
    }

    /// Return the last repeatable excmd passed to [`Self::note_executed`], or
    /// `None` when no repeatable excmd has been executed yet.
    ///
    /// The JS dot-repeat handler (`repeat_command`) calls this to reconstruct
    /// the excmd string and dispatch it instead of the JS `#last_command`.
    pub fn repeat_last(&self) -> Option<ParsedExcmd> {
        self.engine.lock().unwrap().repeat_last()
    }
}
