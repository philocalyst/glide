use serde::{Deserialize, Serialize};
use thiserror::Error;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct EditorSelectionSnapshot {
    pub anchor_scalar_offset: u64,
    pub focus_scalar_offset: u64,
    pub is_collapsed: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct EditorSnapshot {
    pub text: String,
    pub selection: EditorSelectionSnapshot,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum EditInstruction {
    DeleteRange {
        start_scalar_offset: u64,
        end_scalar_offset: u64,
    },
    SelectRange {
        anchor_scalar_offset: u64,
        focus_scalar_offset: u64,
    },
    InsertText {
        scalar_offset: u64,
        text: String,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Record)]
pub struct EditPlan {
    pub instructions: Vec<EditInstruction>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum MotionKind {
    Left,
    Right,
    Up,
    Down,
    StartOfLine,
    FirstNonWhitespace,
    EndOfLine,
    WordForward,
    BigWordForward,
    EndWord,
    WordBackward,
    BigWordBackward,
    ParagraphBackward,
    ParagraphForward,
    InnerWord,
    DeleteLine,
    Substitute,
    OpenLineBelow,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize, uniffi::Enum)]
pub enum EditPlanBehavior {
    MoveCaret,
    ExtendSelectionFromFocus,
    ExtendSelectionFromAnchor,
}

#[derive(Debug, Error, uniffi::Error)]
pub enum EditingError {
    #[error("selection focus offset is out of bounds")]
    SelectionOutOfBounds,
}

pub fn utf16_offset_to_scalar_offset(text: &str, utf16_offset: usize) -> usize {
    let mut accumulated_utf16_units = 0usize;

    for (scalar_offset, character) in text.chars().enumerate() {
        if accumulated_utf16_units >= utf16_offset {
            return scalar_offset;
        }

        accumulated_utf16_units += character.len_utf16();
    }

    text.chars().count()
}

pub fn scalar_offset_to_utf16_offset(text: &str, scalar_offset: usize) -> usize {
    text.chars().take(scalar_offset).map(char::len_utf16).sum()
}

pub fn make_motion_edit_plan(
    snapshot: &EditorSnapshot,
    motion_kind: MotionKind,
    behavior: EditPlanBehavior,
) -> Result<EditPlan, EditingError> {
    let scalar_values: Vec<char> = snapshot.text.chars().collect();
    let scalar_length = scalar_values.len();
    let focus_scalar_offset = snapshot.selection.focus_scalar_offset as usize;
    if focus_scalar_offset > scalar_length {
        return Err(EditingError::SelectionOutOfBounds);
    }

    let plan = match motion_kind {
        MotionKind::Left => {
            selection_plan(snapshot, focus_scalar_offset.saturating_sub(1), behavior)
        }
        MotionKind::Right => {
            selection_plan(snapshot, usize::min(focus_scalar_offset + 1, scalar_length), behavior)
        }
        MotionKind::Up => {
            selection_plan(snapshot, vertical_motion_offset(&scalar_values, focus_scalar_offset, -1), behavior)
        }
        MotionKind::Down => {
            selection_plan(snapshot, vertical_motion_offset(&scalar_values, focus_scalar_offset, 1), behavior)
        }
        MotionKind::StartOfLine => selection_plan(
            snapshot,
            line_start_scalar_offset(&scalar_values, focus_scalar_offset),
            behavior,
        ),
        MotionKind::FirstNonWhitespace => selection_plan(
            snapshot,
            first_non_whitespace_scalar_offset(&scalar_values, focus_scalar_offset),
            behavior,
        ),
        MotionKind::EndOfLine => selection_plan(
            snapshot,
            line_end_scalar_offset(&scalar_values, focus_scalar_offset),
            behavior,
        ),
        MotionKind::WordForward => selection_plan(
            snapshot,
            word_forward_scalar_offset(&scalar_values, focus_scalar_offset, WordTraversalKind::Little),
            behavior,
        ),
        MotionKind::BigWordForward => selection_plan(
            snapshot,
            word_forward_scalar_offset(&scalar_values, focus_scalar_offset, WordTraversalKind::Big),
            behavior,
        ),
        MotionKind::EndWord => selection_plan(
            snapshot,
            word_end_scalar_offset(&scalar_values, focus_scalar_offset),
            behavior,
        ),
        MotionKind::WordBackward => selection_plan(
            snapshot,
            word_backward_scalar_offset(&scalar_values, focus_scalar_offset, WordTraversalKind::Little),
            behavior,
        ),
        MotionKind::BigWordBackward => selection_plan(
            snapshot,
            word_backward_scalar_offset(&scalar_values, focus_scalar_offset, WordTraversalKind::Big),
            behavior,
        ),
        MotionKind::ParagraphBackward => selection_plan(
            snapshot,
            paragraph_backward_scalar_offset(&scalar_values, focus_scalar_offset),
            behavior,
        ),
        MotionKind::ParagraphForward => selection_plan(
            snapshot,
            paragraph_forward_scalar_offset(&scalar_values, focus_scalar_offset),
            behavior,
        ),
        MotionKind::InnerWord => {
            let (start_scalar_offset, end_scalar_offset) =
                inner_word_scalar_offsets(&snapshot.text, focus_scalar_offset);
            EditPlan {
                instructions: vec![EditInstruction::SelectRange {
                    anchor_scalar_offset: start_scalar_offset as u64,
                    focus_scalar_offset: end_scalar_offset as u64,
                }],
            }
        }
        MotionKind::DeleteLine => {
            let mut end_scalar_offset = line_end_scalar_offset(&scalar_values, focus_scalar_offset);
            if scalar_values.get(end_scalar_offset) == Some(&'\n') {
                end_scalar_offset += 1;
            }

            selection_plan(snapshot, end_scalar_offset, behavior)
        }
        MotionKind::Substitute => {
            let current_character = scalar_values.get(focus_scalar_offset).copied();
            if current_character.is_none() || current_character == Some('\n') {
                EditPlan {
                    instructions: Vec::new(),
                }
            } else {
                EditPlan {
                    instructions: vec![EditInstruction::DeleteRange {
                        start_scalar_offset: focus_scalar_offset as u64,
                        end_scalar_offset: usize::min(focus_scalar_offset + 1, scalar_length) as u64,
                    }],
                }
            }
        }
        MotionKind::OpenLineBelow => EditPlan {
            instructions: vec![EditInstruction::InsertText {
                scalar_offset: line_end_scalar_offset(&scalar_values, focus_scalar_offset) as u64,
                text: "\n".into(),
            }],
        },
    };

    Ok(plan)
}

fn selection_plan(
    snapshot: &EditorSnapshot,
    target_scalar_offset: usize,
    behavior: EditPlanBehavior,
) -> EditPlan {
    let (anchor_scalar_offset, focus_scalar_offset) = match behavior {
        EditPlanBehavior::MoveCaret => (target_scalar_offset, target_scalar_offset),
        EditPlanBehavior::ExtendSelectionFromFocus => (
            snapshot.selection.focus_scalar_offset as usize,
            target_scalar_offset,
        ),
        EditPlanBehavior::ExtendSelectionFromAnchor => (
            snapshot.selection.anchor_scalar_offset as usize,
            target_scalar_offset,
        ),
    };

    EditPlan {
        instructions: vec![EditInstruction::SelectRange {
            anchor_scalar_offset: anchor_scalar_offset as u64,
            focus_scalar_offset: focus_scalar_offset as u64,
        }],
    }
}

fn line_start_scalar_offset(scalar_values: &[char], from_scalar_offset: usize) -> usize {
    let mut scalar_offset = usize::min(from_scalar_offset, scalar_values.len());

    while scalar_offset > 0 && scalar_values[scalar_offset.saturating_sub(1)] != '\n' {
        scalar_offset -= 1;
    }

    scalar_offset
}

fn line_end_scalar_offset(scalar_values: &[char], from_scalar_offset: usize) -> usize {
    let mut scalar_offset = usize::min(from_scalar_offset, scalar_values.len());

    while scalar_offset < scalar_values.len() && scalar_values[scalar_offset] != '\n' {
        scalar_offset += 1;
    }

    scalar_offset
}

fn current_column_offset(scalar_values: &[char], from_scalar_offset: usize) -> usize {
    let line_start = line_start_scalar_offset(scalar_values, from_scalar_offset);
    from_scalar_offset.saturating_sub(line_start)
}

fn vertical_motion_offset(
    scalar_values: &[char],
    from_scalar_offset: usize,
    line_delta: isize,
) -> usize {
    let current_line_start = line_start_scalar_offset(scalar_values, from_scalar_offset);
    let current_line_end = line_end_scalar_offset(scalar_values, from_scalar_offset);
    let current_column = current_column_offset(scalar_values, from_scalar_offset);

    if line_delta < 0 {
        if current_line_start == 0 {
            return from_scalar_offset;
        }

        let previous_line_end = current_line_start.saturating_sub(1);
        let previous_line_start = line_start_scalar_offset(scalar_values, previous_line_end);
        return usize::min(previous_line_start + current_column, previous_line_end);
    }

    if current_line_end >= scalar_values.len() {
        return from_scalar_offset;
    }

    let next_line_start = current_line_end + 1;
    let next_line_end = line_end_scalar_offset(scalar_values, next_line_start);
    usize::min(next_line_start + current_column, next_line_end)
}

fn first_non_whitespace_scalar_offset(scalar_values: &[char], from_scalar_offset: usize) -> usize {
    let line_start = line_start_scalar_offset(scalar_values, from_scalar_offset);
    let line_end = line_end_scalar_offset(scalar_values, from_scalar_offset);

    for scalar_offset in line_start..line_end {
        if !scalar_values[scalar_offset].is_whitespace() {
            return scalar_offset;
        }
    }

    line_start
}

#[derive(Clone, Copy)]
enum WordTraversalKind {
    Little,
    Big,
}

fn is_little_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn is_big_word_character(character: char) -> bool {
    !character.is_whitespace()
}

fn word_forward_scalar_offset(
    scalar_values: &[char],
    from_scalar_offset: usize,
    traversal_kind: WordTraversalKind,
) -> usize {
    let mut scalar_offset = usize::min(from_scalar_offset, scalar_values.len());

    while scalar_offset < scalar_values.len() && scalar_values[scalar_offset].is_whitespace() {
        scalar_offset += 1;
    }

    if scalar_offset >= scalar_values.len() {
        return scalar_offset;
    }

    let matches_word = |character: char| match traversal_kind {
        WordTraversalKind::Little => is_little_word_character(character),
        WordTraversalKind::Big => is_big_word_character(character),
    };

    let initial_character = scalar_values[scalar_offset];
    let in_word = matches_word(initial_character);
    while scalar_offset < scalar_values.len()
        && !scalar_values[scalar_offset].is_whitespace()
        && matches_word(scalar_values[scalar_offset]) == in_word
    {
        scalar_offset += 1;
    }

    while scalar_offset < scalar_values.len() && scalar_values[scalar_offset].is_whitespace() {
        scalar_offset += 1;
    }

    scalar_offset
}

fn word_backward_scalar_offset(
    scalar_values: &[char],
    from_scalar_offset: usize,
    traversal_kind: WordTraversalKind,
) -> usize {
    if scalar_values.is_empty() {
        return 0;
    }

    let mut scalar_offset = usize::min(from_scalar_offset.saturating_sub(1), scalar_values.len() - 1);

    while scalar_offset > 0 && scalar_values[scalar_offset].is_whitespace() {
        scalar_offset -= 1;
    }

    let matches_word = |character: char| match traversal_kind {
        WordTraversalKind::Little => is_little_word_character(character),
        WordTraversalKind::Big => is_big_word_character(character),
    };

    let in_word = matches_word(scalar_values[scalar_offset]);
    while scalar_offset > 0
        && !scalar_values[scalar_offset.saturating_sub(1)].is_whitespace()
        && matches_word(scalar_values[scalar_offset.saturating_sub(1)]) == in_word
    {
        scalar_offset -= 1;
    }

    scalar_offset
}

fn word_end_scalar_offset(scalar_values: &[char], from_scalar_offset: usize) -> usize {
    let mut scalar_offset = usize::min(from_scalar_offset, scalar_values.len());

    while scalar_offset < scalar_values.len() && scalar_values[scalar_offset].is_whitespace() {
        scalar_offset += 1;
    }

    if scalar_offset >= scalar_values.len() {
        return scalar_values.len();
    }

    let current_is_word = is_little_word_character(scalar_values[scalar_offset]);
    while scalar_offset + 1 < scalar_values.len()
        && !scalar_values[scalar_offset + 1].is_whitespace()
        && is_little_word_character(scalar_values[scalar_offset + 1]) == current_is_word
    {
        scalar_offset += 1;
    }

    scalar_offset
}

fn paragraph_backward_scalar_offset(scalar_values: &[char], from_scalar_offset: usize) -> usize {
    let current_line_start = line_start_scalar_offset(scalar_values, from_scalar_offset);
    if current_line_start == 0 {
        return 0;
    }

    let mut line_start = current_line_start.saturating_sub(1);
    while line_start > 0 {
        let start = line_start_scalar_offset(scalar_values, line_start);
        let end = line_end_scalar_offset(scalar_values, start);
        let is_blank = scalar_values[start..end]
            .iter()
            .all(|character| character.is_whitespace());
        if is_blank {
            return start;
        }

        if start == 0 {
            return 0;
        }

        line_start = start.saturating_sub(1);
    }

    0
}

fn paragraph_forward_scalar_offset(scalar_values: &[char], from_scalar_offset: usize) -> usize {
    let mut line_start = line_start_scalar_offset(scalar_values, from_scalar_offset);
    loop {
        let line_end = line_end_scalar_offset(scalar_values, line_start);
        if line_end >= scalar_values.len() {
            return line_end;
        }

        let next_line_start = line_end + 1;
        let next_line_end = line_end_scalar_offset(scalar_values, next_line_start);
        let is_blank = scalar_values[next_line_start..next_line_end]
            .iter()
            .all(|character| character.is_whitespace());
        if is_blank {
            return next_line_start;
        }

        line_start = next_line_start;
    }
}

fn inner_word_scalar_offsets(text: &str, from_scalar_offset: usize) -> (usize, usize) {
    let word_boundaries = UnicodeSegmentation::split_word_bound_indices(text)
        .map(|(byte_offset, word)| (text[..byte_offset].chars().count(), word))
        .collect::<Vec<_>>();

    for (index, (start_scalar_offset, word)) in word_boundaries.iter().enumerate() {
        let word_scalar_length = word.chars().count();
        let end_scalar_offset = start_scalar_offset + word_scalar_length;
        if from_scalar_offset >= *start_scalar_offset
            && from_scalar_offset < end_scalar_offset
            && !word.trim().is_empty()
        {
            return (*start_scalar_offset, end_scalar_offset);
        }

        if index + 1 == word_boundaries.len() && !word.trim().is_empty() {
            return (*start_scalar_offset, end_scalar_offset);
        }
    }

    (from_scalar_offset, from_scalar_offset)
}
