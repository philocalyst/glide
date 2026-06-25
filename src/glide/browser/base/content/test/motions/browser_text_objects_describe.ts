// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/* oxlint-disable no-unbound-method */

/* Any copyright is dedicated to the Public Domain.
 * https://creativecommons.org/publicdomain/zero/1.0/ */

"use strict";

/**
 * Stage B: text objects routed through the typed editing-action descriptor.
 * `diw`, `daw`, `di(`, `da(`, `di"`, `da"` now emit a typed `Range` target
 * from the Rust engine and are applied by `editing-actions.mts`.
 */

const INPUT_TEST_FILE = "http://mochi.test:8888/browser/glide/browser/base/content/test/mode/input_test.html";

add_task(async function test_delete_inside_word() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_edit, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 0,
    });

    await set_text("Hello world", "diw from middle of word");
    await set_selection(2, "l");
    await test_edit("diw", " world", 0, " ");

    await set_text("Hello world", "diw from start of word");
    await set_selection(0, "H");
    await test_edit("diw", " world", 0, " ");

    await set_text("Hello world", "diw from end of word");
    await set_selection(4, "o");
    await test_edit("diw", " world", 0, " ");
  });
});

add_task(async function test_delete_around_word() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_edit, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 0,
    });

    await set_text("Hello world foo", "daw deletes word and trailing space");
    await set_selection(0, "H");
    await test_edit("daw", "world foo", 0, "w");

    await set_text("foo bar baz", "daw from middle");
    await set_selection(4, "b");
    await test_edit("daw", "foo baz", 4, " ");
  });
});

add_task(async function test_delete_inside_parens() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_edit, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 0,
    });

    await set_text("foo(bar)baz", "di( deletes inside parens");
    await set_selection(4, "b");
    await test_edit("di(", "foo()baz", 4, ")");

    await set_text("foo(bar: true)baz", "di( with more content");
    await set_selection(5, "a");
    await test_edit("di(", "foo()baz", 4, ")");
  });
});

add_task(async function test_delete_around_parens() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_edit, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 0,
    });

    await set_text("foo(bar)baz", "da( deletes parens and content");
    await set_selection(4, "b");
    await test_edit("da(", "foobaz", 3, "b");
  });
});

add_task(async function test_delete_inside_quotes() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_edit, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 0,
    });

    await set_text('say "hello" world', "di\" deletes inside quotes");
    await set_selection(6, "h");
    await test_edit('di"', 'say "" world', 5, '"');

    await set_text('say "hello world" end', "di\" with longer content");
    await set_selection(7, "e");
    await test_edit('di"', 'say "" end', 5, '"');
  });
});

add_task(async function test_delete_around_quotes() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_edit, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 0,
    });

    await set_text('say "hello" world', "da\" deletes quotes and content");
    await set_selection(6, "h");
    await test_edit('da"', "say  world", 4, " ");
  });
});

add_task(async function test_change_inside_word() {
  await BrowserTestUtils.withNewTab(INPUT_TEST_FILE, async browser => {
    const { set_text, test_edit, set_selection } = GlideTestUtils.make_input_test_helpers(browser, {
      text_start: 0,
    });

    await set_text("Hello world", "ciw replaces word (enter insert)");
    await set_selection(0, "H");
    // `ciw` deletes the word and enters insert mode; we type replacement text.
    await test_edit("ciwX", "X world", 0, "X");
  });
});
