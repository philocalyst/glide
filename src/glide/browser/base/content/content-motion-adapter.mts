/* -*- indent-tabs-mode: nil; js-indent-level: 2 -*-
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Content-process adapter that drives the Rust `glide-modal` edit planner.
 *
 * `make_edit_plan` (in Rust) computes the *offsets* for a motion against a text
 * snapshot; this module reads the editor's current text/selection, asks Rust for
 * an {@link EditPlan}, and applies the resulting instructions to the editor.
 *
 * For motions that aren't yet modelled by the Rust planner these functions
 * return `false`, letting `GlideHandlerChild` fall back to the legacy
 * `motions.mts` helpers.
 */

import type * as RustGlideModalT from "../../../generated/@types/RustGlideModal.d.ts";

const RustGlideModal = ChromeUtils.importESModule(
  "moz-src:///toolkit/components/uniffi-bindgen-gecko-js/components/generated/RustGlideModal.sys.mjs",
);
const motions = ChromeUtils.importESModule("chrome://glide/content/motions.mjs");

// The planner is stateless (no key/mode tracking is used here), so a single
// bridge instance is reused for every motion.
const bridge = new RustGlideModal.GlideModalBridge();

type MotionKind = RustGlideModalT.MotionKind;
type EditPlanBehavior = RustGlideModalT.EditPlanBehavior;

/**
 * A minimal editor shape; the real `nsIEditor.selection` is a DOM `Selection`
 * which exposes `collapse`/`extend`, used here to apply offset ranges.
 */
interface MotionEditor {
  selection: {
    isCollapsed: boolean;
    focusNode?: { textContent: string | null } | null;
    focusOffset: number;
    anchorOffset: number;
    collapse(node: Node, offset: number): void;
    extend(node: Node, offset: number): void;
  };
  insertText?(text: string): void;
}

function operator_motion_kind(sequence: string): MotionKind | null {
  switch (sequence) {
    case "iw":
      return RustGlideModal.MotionKind.InnerWord;
    case "h":
      return RustGlideModal.MotionKind.Left;
    case "l":
      return RustGlideModal.MotionKind.Right;
    case "j":
      return RustGlideModal.MotionKind.Down;
    case "k":
      return RustGlideModal.MotionKind.Up;
    case "d":
      return RustGlideModal.MotionKind.DeleteLine;
    default:
      return null;
  }
}

function direct_motion_kind(keyseq: string): MotionKind | null {
  switch (keyseq) {
    case "w":
      return RustGlideModal.MotionKind.WordForward;
    case "W":
      return RustGlideModal.MotionKind.BigWordForward;
    case "e":
      return RustGlideModal.MotionKind.EndWord;
    case "b":
      return RustGlideModal.MotionKind.WordBackward;
    case "B":
      return RustGlideModal.MotionKind.BigWordBackward;
    case "{":
      return RustGlideModal.MotionKind.ParagraphBackward;
    case "}":
      return RustGlideModal.MotionKind.ParagraphForward;
    case "0":
      return RustGlideModal.MotionKind.StartOfLine;
    case "^":
      return RustGlideModal.MotionKind.FirstNonWhitespace;
    case "$":
      return RustGlideModal.MotionKind.EndOfLine;
    default:
      return null;
  }
}

/**
 * Execute an operator (`d`/`c`) + motion via the Rust planner.
 *
 * Returns `true` if the motion was handled, `false` to fall back to the legacy
 * motion implementation.
 */
export function execute_operator_motion_plan(
  editor: nsIEditor,
  sequence: string,
  _operator: string,
): boolean {
  const motion_kind = operator_motion_kind(sequence);
  if (motion_kind == null) {
    return false;
  }

  const applied = apply_plan(
    editor as unknown as MotionEditor,
    motion_kind,
    RustGlideModal.EditPlanBehavior.ExtendSelectionFromFocus,
  );
  if (!applied) {
    return false;
  }

  // The selection now spans the motion; deletion (and any insert for `c`) is
  // driven by `GlideHandlerChild` based on the collapsed-ness of the selection.
  return true;
}

/**
 * Execute a direct (non-operator) motion via the Rust planner.
 */
export function execute_direct_motion_plan(
  editor: nsIEditor,
  keyseq: string,
  mode: GlideMode | undefined,
): boolean {
  const motion_kind = direct_motion_kind(keyseq);
  if (motion_kind == null) {
    return false;
  }

  const behavior = mode === "visual"
    ? RustGlideModal.EditPlanBehavior.ExtendSelectionFromAnchor
    : RustGlideModal.EditPlanBehavior.MoveCaret;

  return apply_plan(editor as unknown as MotionEditor, motion_kind, behavior);
}

function apply_plan(
  editor: MotionEditor,
  motion_kind: MotionKind,
  behavior: EditPlanBehavior,
): boolean {
  const focus_node = editor.selection.focusNode;
  const text = focus_node?.textContent;
  if (focus_node == null || text == null) {
    return false;
  }

  const snapshot = new RustGlideModal.EditorSnapshot({
    text,
    selection: new RustGlideModal.EditorSelectionSnapshot({
      anchorScalarOffset: utf16_to_scalar(text, editor.selection.anchorOffset),
      focusScalarOffset: utf16_to_scalar(text, editor.selection.focusOffset),
      isCollapsed: editor.selection.isCollapsed,
    }),
  });

  let plan: RustGlideModalT.EditPlan;
  try {
    plan = bridge.makeEditPlan(snapshot, motion_kind, behavior);
  } catch {
    // out-of-bounds selection etc. — defer to the fallback implementation.
    return false;
  }

  for (const instruction of plan.instructions) {
    if (instruction instanceof RustGlideModal.EditInstruction.SelectRange) {
      set_selection(
        editor,
        focus_node as unknown as Node,
        text,
        instruction.anchorScalarOffset,
        instruction.focusScalarOffset,
      );
    } else if (instruction instanceof RustGlideModal.EditInstruction.DeleteRange) {
      set_selection(
        editor,
        focus_node as unknown as Node,
        text,
        instruction.startScalarOffset,
        instruction.endScalarOffset,
      );
      if (!editor.selection.isCollapsed) {
        motions.delete_selection(editor as any, /* forward */ false);
      }
    } else if (instruction instanceof RustGlideModal.EditInstruction.InsertText) {
      const offset = scalar_to_utf16(text, instruction.scalarOffset);
      editor.selection.collapse(focus_node as unknown as Node, offset);
      if (!editor.insertText) {
        return false;
      }
      editor.insertText(instruction.text);
    }
  }

  return true;
}

function set_selection(
  editor: MotionEditor,
  node: Node,
  text: string,
  anchor_scalar: number,
  focus_scalar: number,
) {
  editor.selection.collapse(node, scalar_to_utf16(text, anchor_scalar));
  if (focus_scalar !== anchor_scalar) {
    editor.selection.extend(node, scalar_to_utf16(text, focus_scalar));
  }
}

/** Convert a scalar (code point) offset into a UTF-16 (DOM) offset. */
function scalar_to_utf16(text: string, scalar_offset: number): number {
  let utf16_offset = 0;
  let scalar_count = 0;
  for (const character of text) {
    if (scalar_count >= scalar_offset) {
      break;
    }
    utf16_offset += character.length;
    scalar_count++;
  }
  return utf16_offset;
}

/** Convert a UTF-16 (DOM) offset into a scalar (code point) offset. */
function utf16_to_scalar(text: string, utf16_offset: number): number {
  let utf16_count = 0;
  let scalar_offset = 0;
  for (const character of text) {
    if (utf16_count >= utf16_offset) {
      break;
    }
    utf16_count += character.length;
    scalar_offset++;
  }
  return scalar_offset;
}
