/* -*- indent-tabs-mode: nil; js-indent-level: 2 -*-
 * This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

const text_obj = ChromeUtils.importESModule("chrome://glide/content/text-objects.mjs");
const { assert_never } = ChromeUtils.importESModule("chrome://glide/content/utils/guards.mjs");

/**
 * A minimal representation of `nsIEditor` so that we can re-implement editors
 * for non-standard cases, e.g. Google Docs, Monaco.
 */
export interface Editor {
  selection: {
    isCollapsed: boolean;
    focusNode?: {
      textContent: string | null;
    } | null;
    focusOffset: number;
    anchorOffset: number;
  };
  selectionController: {
    characterMove(forward: boolean, extend: boolean): void;
    lineMove(forward: boolean, extend: boolean): void;
    intraLineMove(forward: boolean, extend: boolean): void;
  };
  deleteSelection(action: number, stripWrappers: number): void;
}

/**
 * Offset primitives for the descriptor-driven editing executor
 * (`editing-actions.mts`) and legacy content handlers.
 *
 * Each function moves the caret (or extends the selection) by one unit; the
 * executor composes them to realize counts, operator×motion, and text objects.
 */

/**
 * Returns the offset of the caret in the current line.
 *
 * This returns `0` in a couple of different situations:
 * - caret is at the BOF
 * - caret is at the first character after the BOF
 * - caret is at the newline char (i.e. at the start of the line)
 *
 * col = 0
 * ```
 * █foo
 * ---
 * █oo
 * ---
 * foo
 * █bar
 * ```
 *
 * col = 2
 * ```
 * foo
 * b█r
 * ```
 */
export function get_column_offset(editor: Editor): number {
  let pos = 0;
  let i = editor.selection.focusOffset - 1;
  const content = editor.selection.focusNode?.textContent;
  if (content == null) {
    throw new Error("No focused text content");
  }

  while (true) {
    const char = content.charAt(i);
    if (char === "\n" || i <= 0) {
      break;
    }
    pos++;
    i--;
  }

  return pos;
}

export function start_of_word(editor: Editor) {
  const starting_cls = text_obj.cls(current_char(editor));

  while (text_obj.cls(current_char(editor)) === starting_cls) {
    editor.selectionController.characterMove(/* forward */ false, /* extend */ false);

    if (is_bof(editor) || is_eol(editor)) {
      break;
    }
  }

  // correct off-by-one
  editor.selectionController.characterMove(/* forward */ true, /* extend */ false);
}

export function end_of_word(
  editor: Editor,
  props?: { extend?: boolean; inclusive?: boolean },
) {
  if (is_eol(editor)) {
    // if we're at the end of the line then there's nothing more we can do
    return;
  }

  const extend = props?.extend ?? false;
  const inclusive = props?.inclusive ?? false;
  const starting_cls = text_obj.cls(current_char(editor));

  if (inclusive) {
    // include the current char
    editor.selectionController.characterMove(false, false);
  }

  while (text_obj.cls(next_char(editor)) === starting_cls) {
    editor.selectionController.characterMove(true, extend);

    if (is_eof(editor) || is_eol(editor)) {
      break;
    }
  }
}

/**
 * Move the selection forward 1 word.
 *
 * `bigword=true`  -> equivalent to `W`
 * `bigword=false` -> equivalent to `w`
 */
export function forward_word(
  editor: Editor,
  bigword: boolean,
  extend: boolean,
) {
  const starting_cls = text_obj.cls(current_char(editor));

  // we always want to move one character forward no matter what
  editor.selectionController.characterMove(true, extend);

  // go one char past end of current word (if any)
  if (starting_cls !== text_obj.CLS_WHITESPACE) {
    if (bigword) {
      // for bigword, the word boundary is any whitespace
      while (text_obj.cls(current_char(editor)) !== text_obj.CLS_WHITESPACE) {
        editor.selectionController.characterMove(true, extend);
        if (is_eof(editor) || is_eol(editor)) {
          break;
        }
      }
    } else {
      // for non-bigword, the word boundary is anything other than the starting class
      while (text_obj.cls(current_char(editor)) === starting_cls) {
        editor.selectionController.characterMove(true, extend);
        if (is_eof(editor) || is_eol(editor)) {
          break;
        }
      }
    }
  }

  // find the next word
  while (text_obj.cls(current_char(editor)) === text_obj.CLS_WHITESPACE) {
    editor.selectionController.characterMove(true, extend);

    if (is_eof(editor) || is_eol(editor)) {
      break;
    }
  }
}

/**
 * Move the selection to the end of the word.
 */
export function end_word(editor: Editor, extend: boolean) {
  do {
    // we always want to move one character forward no matter what
    editor.selectionController.characterMove(true, extend);

    if (is_eof(editor)) {
      return;
    }
  } while (text_obj.cls(current_char(editor)) === text_obj.CLS_WHITESPACE);

  const starting_cls = text_obj.cls(current_char(editor));

  while (text_obj.cls(next_char(editor)) === starting_cls) {
    editor.selectionController.characterMove(true, extend);
    if (is_eof(editor) || is_eol(editor)) {
      break;
    }
  }
}

/**
 * Move the selection backward 1 word.
 *
 * `bigword=true`  -> equivalent to `B`
 * `bigword=false` -> equivalent to `b`
 */
export function back_word(editor: Editor, bigword: boolean, extend: boolean) {
  // we always want to move one character back no matter what
  editor.selectionController.characterMove(/* forward */ false, extend);

  // find the end of the previous word
  while (text_obj.cls(current_char(editor)) === text_obj.CLS_WHITESPACE) {
    editor.selectionController.characterMove(/* forward */ false, extend);

    if (is_bof(editor)) {
      break;
    }
  }

  // go backwards until we find a new word class
  const starting_cls = text_obj.cls(current_char(editor));
  if (bigword) {
    while (text_obj.cls(current_char(editor)) !== text_obj.CLS_WHITESPACE) {
      editor.selectionController.characterMove(/* forward */ false, extend);

      if (is_bof(editor)) {
        break;
      }
    }
  } else {
    while (text_obj.cls(current_char(editor)) === starting_cls) {
      editor.selectionController.characterMove(/* forward */ false, extend);

      if (is_bof(editor)) {
        break;
      }
    }
  }

  // we moved one too far
  if (text_obj.cls(current_char(editor)) !== starting_cls || is_bof(editor)) {
    editor.selectionController.characterMove(/* forward */ true, extend);
  }
}

export function next_para(editor: nsIEditor) {
  while (true) {
    editor.selectionController.lineMove(true, false);
    editor.selectionController.intraLineMove(true, false);

    if (is_empty_line(editor) || is_eof(editor)) {
      break;
    }
  }
}

export function back_para(editor: nsIEditor) {
  while (true) {
    editor.selectionController.lineMove(false, false);
    editor.selectionController.intraLineMove(false, false);

    if (is_empty_line(editor) || is_bof(editor)) {
      break;
    }
  }

  if (is_bof(editor) && next_char(editor) !== "\n") {
    editor.selectionController.characterMove(true, false);
  }
}

/**
 * Move back one character, this does *not* cross line boundaries.
 */
export function back_char(editor: Editor, extend: boolean) {
  if (
    (selection_direction(editor) !== "forwards"
      && current_char(editor) === "\n")
    || editor.selection.focusOffset < 1
  ) {
    return;
  }
  editor.selectionController.characterMove(false, extend);
}

/**
 * Move forward one character, this does *not* cross line boundaries when
 * the selection is collapsed (a la normal mode) but does allow crossing
 * the line boundary by a single character when the selection is not collapsed
 * (a la visual mode).
 */
export function forward_char(editor: Editor, extend: boolean) {
  if (is_eof(editor)) {
    return;
  }

  // normal mode
  if (editor.selection.isCollapsed && next_char(editor) === "\n") {
    return;
  }

  // visual mode allows
  if (
    selection_direction(editor) !== "backwards"
    && current_char(editor) === "\n"
  ) {
    return;
  }

  editor.selectionController.characterMove(true, extend);
}

/**
 * Equivalent to `0`. Goes to the *very* beginning of the line, skipping
 * any leading whitespace.
 */
export function beginning_of_line(
  editor: Editor,
  extend: boolean,
  inclusive: boolean = false,
) {
  while (preceding_char(editor) !== "\n" && editor.selection.focusOffset > 1) {
    editor.selectionController.characterMove(false, extend);
  }

  if (inclusive && !is_bof(editor, "current")) {
    editor.selectionController.characterMove(false, extend);
  }
}

/**
 * Equivalent to `$`. Goes to the *very* end of the line, skipping
 * any trailing whitespace.
 */
export function end_of_line(
  editor: Editor,
  extend: boolean,
  inclusive: boolean = false,
) {
  while (next_char(editor) !== "\n" && !is_eof(editor)) {
    editor.selectionController.characterMove(true, extend);
  }
  if (inclusive && next_char(editor) === "\n") {
    editor.selectionController.characterMove(true, extend);
  }
}

/**
 * Used for `I` and `^`.
 *
 * Goes to the first non-whitespace character in the line.
 */
export function first_non_whitespace(editor: Editor, extend: boolean = false) {
  if (is_eol(editor)) {
    return;
  }

  // TODO(someday): this probably has bad / weird implications for visual mode
  beginning_of_line(editor, extend, /* inclusive */ true);

  // In multiline text, beginning_of_line with inclusive=true may land us on the
  // newline of the previous line. If so, move forward to the current line.
  if (current_char(editor) === "\n") {
    editor.selectionController.characterMove(true, extend);
    // After moving forward, we're ON the first char of the line.
    // If it's already non-whitespace, we're done.
    if (!is_eol(editor) && !is_eof(editor) && text_obj.cls(current_char(editor)) !== text_obj.CLS_WHITESPACE) {
      return;
    }
  }

  // Skip past any leading whitespace by checking next_char.
  // (We're at focusOffset=0, so next_char is the first char of line)
  while (!is_eol(editor) && !is_eof(editor) && text_obj.cls(next_char(editor)) === text_obj.CLS_WHITESPACE) {
    editor.selectionController.characterMove(true, extend);
  }

  // The loop exits when next_char is NOT whitespace, but current_char is still
  // the last whitespace (or empty at focusOffset=0). Move one more to land ON
  // the first non-whitespace character.
  if (!is_eol(editor) && !is_eof(editor)) {
    editor.selectionController.characterMove(true, extend);
  }
}

/**
 * Delete the current selection range.
 */
export function delete_selection(editor: Editor, forward: boolean) {
  if (editor.selection.isCollapsed) {
    throw new Error("cannot delete collapsed selections");
  }

  editor.deleteSelection(/* action */ Ci.nsIEditor.ePrevious, /* stripWrappers */ Ci.nsIEditor.eStrip);

  if (forward && !is_eol(editor)) {
    forward_char(editor, false);
  }
}

function selection_direction(
  editor: Editor,
): "forwards" | "backwards" | "collapsed" {
  if (editor.selection.isCollapsed) {
    return "collapsed";
  }

  if (editor.selection.anchorOffset < editor.selection.focusOffset) {
    return "forwards";
  }

  return "backwards";
}

export function preceding_char(editor: Editor): string | null {
  const content = editor.selection.focusNode?.textContent;
  if (content == null) {
    throw new Error("No focused text content");
  }

  const index = editor.selection.focusOffset - 2;
  if (index < 0) {
    return null;
  }
  return content.charAt(index);
}

export function current_char(editor: Editor): string {
  const content = editor.selection.focusNode?.textContent;
  if (content == null) {
    throw new Error("No focused text content");
  }

  return content.charAt(editor.selection.focusOffset - 1);
}

export function next_char(editor: Editor): string {
  const content = editor.selection.focusNode?.textContent;
  if (content == null) {
    throw new Error("No focused text content, cannot move forward");
  }

  return content.charAt(editor.selection.focusOffset);
}

function is_empty_line(editor: Editor): boolean {
  // An empty line is when we're on a newline and either:
  // 1. The previous character is also a newline (empty line between text)
  // 2. The next character is also a newline (empty line between text)
  return (
    current_char(editor) === "\n"
    && (preceding_char(editor) === "\n" || next_char(editor) === "\n")
  );
}

export function is_bof(
  editor: Editor,
  pos: "left" | "current" = "current",
): boolean {
  switch (pos) {
    case "left":
      return editor.selection.focusOffset - 1 <= 0;
    case "current":
      return editor.selection.focusOffset <= 0;
    default:
      throw assert_never(pos);
  }
}

export function is_eof(editor: Editor): boolean {
  return (
    editor.selection.focusOffset
      === editor.selection.focusNode?.textContent?.length
  );
}

export function is_eol(editor: Editor): boolean {
  return current_char(editor) === "\n";
}
