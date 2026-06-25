// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/* oxlint-disable no-unbound-method */

/* Any copyright is dedicated to the Public Domain.
 * https://creativecommons.org/publicdomain/zero/1.0/ */

"use strict";

/**
 * Stage A: counts (`3w`) and operator×motion composition (`dw`, `db`, `d$`,
 * `3dw`). These exercise the typed editing-action descriptor emitted by the
 * Rust engine and consumed by `editing-actions.mts`.
 */

const INPUT_TEST_FILE = "http://mochi.test:8888/browser/glide/browser/base/content/test/mode/input_test.html";

add_task(async function test_count_forward_word() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_motion, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 1,
    });

    await set_text("one two three four", "3w moves three words forward");
    await set_selection(0, "o");
    await test_motion("3w", 14, "f");

    await set_text("one two three four", "2w moves two words forward");
    await set_selection(0, "o");
    await test_motion("2w", 8, "t");

    await set_text("aaa bbb ccc ddd", "count past end stops at last word");
    await set_selection(0, "a");
    await test_motion("9w", 14, "d");
  });
});

add_task(async function test_count_back_word() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_motion, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 1,
    });

    await set_text("one two three four", "3b moves three words back");
    await set_selection(14, "f");
    await test_motion("3b", 0, "o");

    await set_text("one two three four", "2b moves two words back");
    await set_selection(14, "f");
    await test_motion("2b", 8, "t");
  });
});

add_task(async function test_operator_with_motion_word_forward() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_edit, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 0,
    });

    await set_text("hello world foo", "dw deletes to next word");
    await set_selection(0, "h");
    await test_edit("dw", "world foo", 0, "w");

    await set_text("hello world foo", "3dw deletes three words");
    await set_selection(0, "h");
    await test_edit("3dw", "foo", 0, "f");

    await set_text("foo bar baz qux", "2dw deletes two words");
    await set_selection(0, "f");
    await test_edit("2dw", "baz qux", 0, "b");
  });
});

add_task(async function test_operator_with_motion_word_back() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_edit, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 0,
    });

    // `db` from `world` in `hello world foo` selects back over ` ` (the
    // whitespace before `w`), leaving `hello world foo` with the caret on the
    // space (fixup moves onto the next char).
    await set_text("hello world foo", "db deletes back to previous word");
    await set_selection(6, "w");
    await test_edit("db", "hello world foo", 5, " ");

    await set_text("foo bar baz", "db from middle of word");
    await set_selection(8, "a");
    await test_edit("db", "foo az", 4, "a");
  });
});

add_task(async function test_operator_with_motion_line_end() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_edit, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 0,
    });

    await set_text("hello world", "d$ deletes to end of line");
    await set_selection(0, "h");
    await test_edit("d$", "", -1, "");

    await set_text("hello world", "d$ from middle of line");
    await set_selection(6, "w");
    await test_edit("d$", "hello ", 5, " ");
  });
});

add_task(async function test_operator_with_motion_line_start() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_edit, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 0,
    });

    await set_text("hello world", "d0 deletes to start of line");
    await set_selection(6, "w");
    await test_edit("d0", "world", 0, "w");
  });
});

add_task(async function test_legacy_fallback_still_works() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_edit, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 0,
    });

    // `iw` is a text object (Stage B) — it should still work via the fallback.
    await set_text("Hello world", "diw still works via fallback");
    await set_selection(2, "l");
    await test_edit("diw", " world", 0, " ");
  });
});
