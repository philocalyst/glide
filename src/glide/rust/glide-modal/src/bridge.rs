use std::sync::Mutex;

use crate::actions::{GlideMode, KeymapDefinition, ResolvedKeyResult};
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
}
