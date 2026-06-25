// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/**
 * Stage A descriptor-driven editing executor.
 *
 * The Rust engine (built on modalkit) computes a typed editing descriptor for
 * every key — `EditingActionIntent { operation, target }` — that captures
 * counts, operator×motion composition, and motion kind. The parent converts it
 * to a plain `GlideEditingAction` (see `modal-engine.mts`) and forwards it with
 * `Glide::ExecuteContentCommand`. This module consumes that descriptor and
 * applies it to the focused editor by reusing the offset primitives in
 * `motions.mts`.
 *
 * Returning `false` from {@link apply_editing_action} signals the caller
 * (`GlideHandlerChild`) to fall back to the legacy per-key switch, so nothing
 * regresses while the descriptor path is rolled out.
 *
 * Stage A covers counts (`3w`) and operator×motion composition for the common
 * vim motions (`dw`, `db`, `d$`, `cw`, …). Text objects (`iw`, `di"`, `dit`)
 * are Stage B.
 */

import type { GlideEditingAction } from "./modal-engine.mts";
import type { GlideOperator } from "./browser-excmds-registry.mts";

const motions = ChromeUtils.importESModule("chrome://glide/content/motions.mjs");

/**
 * Apply a typed editing action to `editor`.
 *
 * @returns `true` if the action was fully handled; `false` if the caller should
 *          fall back to the legacy per-key path.
 */
export function apply_editing_action(
  editor: nsIEditor,
  action: GlideEditingAction,
  ctx: { mode: GlideMode; operator: GlideOperator | null },
): boolean {
  if (action.target.kind === "motion") {
    return apply_motion_action(editor, action, ctx);
  }

  // `line-range` (`dd`) and other target kinds are Stage B / fallback.
  return false;
}

/**
 * Resolve a `Motion` target to a caret offset by applying the matching offset
 * primitive `count` times, then apply the operation.
 */
function apply_motion_action(
  editor: nsIEditor,
  action: GlideEditingAction,
  ctx: { mode: GlideMode; operator: GlideOperator | null },
): boolean {
  const target = action.target;
  if (target.motion === undefined) {
    return false;
  }

  const count = Math.max(1, target.count);
  const direction = target.direction ?? "next";
  const bigword = target.wordStyle === "big";
  const extend = ctx.mode === "visual";

  // For operator+motion (Delete/Change/Yank), we first collapse the selection
  // to a known anchor, extend it across the motion, then apply the operation.
  // For a bare `Motion` operation, we just move the caret (extending in visual).
  const is_operator = action.operation === "delete" || action.operation === "change" || action.operation === "yank";

  if (is_operator && !editor.selection.isCollapsed) {
    // Make sure we start from the current caret position as the anchor.
    editor.selection.collapse(editor.selection.focusNode, editor.selection.focusOffset);
  }

  // Apply the offset primitive `count` times. For operator+motion, we extend
  // the selection on each step so the range covers everything the motion
  // traversed; for a bare motion, we just move (extend only in visual mode).
  const step_extend = is_operator || extend;

  for (let i = 0; i < count; i++) {
    if (!step_motion(editor, target.motion, direction, bigword, step_extend)) {
      // The motion couldn't be applied (e.g. already at boundary); stop early.
      break;
    }
  }

  switch (action.operation) {
    case "motion":
      return true;
    case "delete": {
      if (editor.selection.isCollapsed) {
        return true;
      }
      // `forward=true` fixes up the caret after `deleteSelection(ePrevious)`,
      // moving it onto the first remaining character — matching the legacy
      // `dh`/`dw` path. Correct for both forward and backward motions.
      motions.delete_selection(editor, /* forward */ true);
      return true;
    }
    case "change": {
      if (!editor.selection.isCollapsed) {
        // `forward=false` leaves the caret before the first char, ready for
        // insert mode (matching the legacy `c` path).
        motions.delete_selection(editor, /* forward */ false);
      }
      // Switch to insert mode is handled by the caller (mode bookkeeping).
      return true;
    }
    case "yank": {
      if (!editor.selection.isCollapsed) {
        editor.copy();
        // collapse to the original anchor (focus is at the end of the range)
        editor.selection.collapseToStart();
      }
      return true;
    }
    default:
      return false;
  }
}

/**
 * Apply one step of the given motion kind to `editor`.
 *
 * @param extend  If `true`, extend the selection while moving; otherwise just
 *                move the caret.
 * @returns `false` if the motion is unsupported or hit a boundary without
 *          moving; `true` if it stepped.
 */
function step_motion(
  editor: nsIEditor,
  motion: NonNullable<GlideEditingAction["target"]["motion"]>,
  direction: "previous" | "next",
  bigword: boolean,
  extend: boolean,
): boolean {
  // For `extend` we temporarily toggle the editor into a "visual-like" mode by
  // passing `extend=true` to the motion primitives. The legacy primitives use
  // `mode === "visual"` to decide extending, so for operator+motion we fake it
  // by always extending here.
  const fake_mode: GlideMode | undefined = extend ? "visual" : undefined;

  switch (motion) {
    case "word-begin": {
      if (direction === "next") {
        motions.forward_word(editor, bigword, fake_mode);
        return true;
      }
      // `b` / `B`: backwards word. The legacy primitive doesn't extend, so for
      // operator+motion (`db`) we walk back one char at a time extending.
      if (extend) {
        return step_back_word_extending(editor, bigword);
      }
      motions.back_word(editor, bigword);
      return true;
    }
    case "word-end": {
      if (direction === "next") {
        motions.end_word(editor, fake_mode);
        return true;
      }
      return false;
    }
    case "line-start": {
      motions.beginning_of_line(editor, extend);
      return true;
    }
    case "line-end": {
      // `$` is inclusive: extend onto the end-of-line character.
      motions.end_of_line(editor, extend, /* inclusive */ true);
      return true;
    }
    case "first-word": {
      motions.first_non_whitespace(editor, extend);
      return true;
    }
    case "line": {
      editor.selectionController.lineMove(direction === "next", extend);
      return true;
    }
    case "paragraph-begin": {
      if (direction === "next") {
        motions.next_para(editor);
      } else {
        motions.back_para(editor);
      }
      return true;
    }
    case "column": {
      // horizontal char motion — `h`/`l`. The legacy primitives don't cross
      // line boundaries, which matches vim.
      if (direction === "next") {
        motions.forward_char(editor, extend);
      } else {
        motions.back_char(editor, extend);
      }
      return true;
    }
    default:
      return false;
  }
}

/**
 * Extend the selection backwards by one word, since `motions.back_word`
 * doesn't take an `extend` parameter. We move back without extending, then
 * re-extend the selection forward to the previous caret position.
 */
function step_back_word_extending(editor: nsIEditor, bigword: boolean): boolean {
  const start_node = editor.selection.focusNode;
  const start_offset = editor.selection.focusOffset;

  // Move back without extending to find the start of the previous word.
  motions.back_word(editor, bigword);

  const new_offset = editor.selection.focusOffset;
  if (new_offset === start_offset) {
    return false;
  }

  // Re-select from the new position back to the original.
  if (start_node != null) {
    editor.selection.extend(start_node, start_offset);
  }
  return true;
}
