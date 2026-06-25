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
  let handled = false;

  if (action.target.kind === "motion") {
    handled = apply_motion_action(editor, action, ctx);
  } else if (action.target.kind === "range") {
    handled = apply_range_action(editor, action, ctx);
  }

  if (!handled) {
    return false;
  }

  // Apply the operation now that the selection covers the target.
  return apply_operation(editor, action);
}

/**
 * Apply the operation portion of an editing action (delete/change/yank) to the
 * currently-selected range. Assumes the selection has already been extended to
 * cover the target by {@link apply_motion_action} or {@link apply_range_action}.
 */
function apply_operation(editor: nsIEditor, action: GlideEditingAction): boolean {
  switch (action.operation) {
    case "motion":
      return true;
    case "delete": {
      if (editor.selection.isCollapsed) {
        return true;
      }
      motions.delete_selection(editor, /* forward */ true);
      return true;
    }
    case "change": {
      if (!editor.selection.isCollapsed) {
        motions.delete_selection(editor, /* forward */ false);
      }
      return true;
    }
    case "yank": {
      if (!editor.selection.isCollapsed) {
        editor.copy();
        editor.selection.collapseToStart();
      }
      return true;
    }
    default:
      return false;
  }
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

  return true;
}

/**
 * Resolve a `Range` text-object target to a selection covering the object,
 * then return `true` so the caller applies the operation.
 *
 * Stage B handles `word`, `bracketed`, and `quote`; other range kinds
 * (`xml-tag`, `paragraph`, `sentence`) return `false` to fall back.
 */
function apply_range_action(
  editor: nsIEditor,
  action: GlideEditingAction,
  _ctx: { mode: GlideMode; operator: GlideOperator | null },
): boolean {
  const target = action.target;
  const range_kind = target.range;
  if (range_kind === undefined) {
    return false;
  }

  // Start from the current caret position as the anchor.
  if (!editor.selection.isCollapsed) {
    editor.selection.collapse(editor.selection.focusNode, editor.selection.focusOffset);
  }

  switch (range_kind) {
    case "word":
      return select_word_range(editor, target.inclusive ?? true, target.wordStyle === "big");
    case "bracketed":
      return select_bracketed_range(editor, target.left ?? "(", target.right ?? ")", target.inclusive ?? false);
    case "quote":
      return select_quote_range(editor, target.quote ?? "\"", target.inclusive ?? false);
    default:
      // `xml-tag`, `paragraph`, `sentence`, `line`, `buffer`, `item` — not yet
      // implemented in the content executor; fall back to the legacy path.
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

// ---------------------------------------------------------------------------
// Text-object range primitives (Stage B)
// ---------------------------------------------------------------------------

/**
 * Select the word around the cursor (`iw` / `aw`).
 *
 * `inclusive=true` (`aw`) also includes surrounding whitespace; `inclusive=false`
 * (`iw`) selects just the word characters. Mirrors the legacy `select_motion`
 * `iw` arm but adds the `aw` whitespace expansion.
 */
function select_word_range(editor: nsIEditor, inclusive: boolean, bigword: boolean): boolean {
  // Move to the start of the current word, then extend to its end.
  motions.start_of_word(editor);
  const anchor_node = editor.selection.focusNode;
  const anchor_offset = editor.selection.focusOffset;

  motions.end_of_word(editor, { extend: true, inclusive: true });

  if (inclusive) {
    // `aw`: also include trailing whitespace (or leading if at end of line).
    const text = editor.selection.focusNode?.textContent ?? "";
    const focus = editor.selection.focusOffset;
    if (focus < text.length && text.charAt(focus) === " ") {
      while (focus < text.length && text.charAt(focus) === " ") {
        editor.selectionController.characterMove(true, true);
      }
    } else if (anchor_offset > 0 && text.charAt(anchor_offset - 1) === " ") {
      // No trailing whitespace; include leading whitespace instead.
      let back = anchor_offset - 1;
      while (back > 0 && text.charAt(back - 1) === " ") {
        back--;
      }
      if (anchor_node != null) {
        editor.selection.collapse(anchor_node, anchor_offset);
        while (editor.selection.focusOffset > back) {
          editor.selectionController.characterMove(false, true);
        }
        // Re-extend to the end of the word.
        motions.end_of_word(editor, { extend: true, inclusive: true });
      }
    }
  }

  return !editor.selection.isCollapsed;
}

/**
 * Select the text between matching bracket characters (`i(` / `a(`).
 *
 * `inclusive=false` (`i(`) selects only the content between the brackets;
 * `inclusive=true` (`a(`) also includes the bracket characters themselves.
 */
function select_bracketed_range(editor: nsIEditor, left: string, right: string, inclusive: boolean): boolean {
  const text = editor.selection.focusNode?.textContent;
  if (text == null) {
    return false;
  }

  const caret = editor.selection.focusOffset;
  // Search backwards for the opening bracket, tracking nesting depth.
  let depth = 0;
  let open_index = -1;
  for (let i = caret - 1; i >= 0; i--) {
    const ch = text.charAt(i);
    if (ch === right) {
      depth++;
    } else if (ch === left) {
      if (depth === 0) {
        open_index = i;
        break;
      }
      depth--;
    }
  }
  if (open_index === -1) {
    return false;
  }

  // Search forwards for the closing bracket, tracking nesting depth.
  depth = 0;
  let close_index = -1;
  for (let i = open_index + 1; i < text.length; i++) {
    const ch = text.charAt(i);
    if (ch === left) {
      depth++;
    } else if (ch === right) {
      if (depth === 0) {
        close_index = i;
        break;
      }
      depth--;
    }
  }
  if (close_index === -1) {
    return false;
  }

  // Select the range. `focusOffset` is 1-based (caret is after the char at
  // offset-1), so we set the selection from open_index+1 to close_index+1
  // (exclusive of the brackets for `i`, inclusive for `a`).
  const node = editor.selection.focusNode!;
  const start = inclusive ? open_index + 1 : open_index + 2;
  const end = inclusive ? close_index + 2 : close_index + 1;

  if (end <= start) {
    return false;
  }

  editor.selection.collapse(node, start);
  editor.selection.extend(node, end);
  return true;
}

/**
 * Select the text between matching quote characters (`i"` / `a"`).
 *
 * `inclusive=false` (`i"`) selects only the content between the quotes;
 * `inclusive=true` (`a"`) also includes the quote characters themselves.
 */
function select_quote_range(editor: nsIEditor, quote: string, inclusive: boolean): boolean {
  const text = editor.selection.focusNode?.textContent;
  if (text == null) {
    return false;
  }

  const caret = editor.selection.focusOffset;

  // Find the nearest pair of quotes around the cursor. If the cursor is on a
  // quote char, treat it as the opening quote. Otherwise search backwards for
  // the opening quote, then forwards for the closing one.
  let open_index = -1;
  if (caret > 0 && text.charAt(caret - 1) === quote) {
    open_index = caret - 1;
  } else {
    // Search backwards for an odd-counted quote (opening).
    let count = 0;
    for (let i = caret - 1; i >= 0; i--) {
      if (text.charAt(i) === quote) {
        count++;
        open_index = i;
      }
    }
    if (count % 2 === 0) {
      // Even count means the cursor is before the opening quote; search forward.
      open_index = -1;
      for (let i = caret; i < text.length; i++) {
        if (text.charAt(i) === quote) {
          open_index = i;
          break;
        }
      }
    }
  }
  if (open_index === -1 || open_index + 1 >= text.length) {
    return false;
  }

  // Find the closing quote after the opening one.
  let close_index = -1;
  for (let i = open_index + 1; i < text.length; i++) {
    if (text.charAt(i) === quote) {
      close_index = i;
      break;
    }
  }
  if (close_index === -1) {
    return false;
  }

  const node = editor.selection.focusNode!;
  const start = inclusive ? open_index + 1 : open_index + 2;
  const end = inclusive ? close_index + 2 : close_index + 1;

  if (end <= start) {
    return false;
  }

  editor.selection.collapse(node, start);
  editor.selection.extend(node, end);
  return true;
}
