// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

export const BUILTIN_GLIDE_MODES = [
  "normal",
  "insert",
  "visual",
  "op-pending",
  "ignore",
  "command",
  "hint",
] as const satisfies readonly GlideMode[];

export const BUILTIN_MODE_CARET_CONFIG = {
  normal: { caret: "block" },
  insert: { caret: "line" },
  visual: { caret: "block" },
  "op-pending": { caret: "underline" },
  ignore: { caret: "line" },
  command: { caret: "line" },
  hint: { caret: "block" },
} as const satisfies Record<GlideMode, { caret: "block" | "line" | "underline" }>;
