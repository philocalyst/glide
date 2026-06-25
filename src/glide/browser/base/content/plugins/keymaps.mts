// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

/**
 * This file defines builtin keymappings that depend on sandbox callbacks.
 *
 * Static string keybindings are registered by the modal engine so the modal
 * implementation can migrate as a single unit.
 */

import type { Sandbox } from "../sandbox.mts";

const hinting = ChromeUtils.importESModule("chrome://glide/content/hinting.mjs");

export function init(sandbox: Sandbox) {
  const { glide } = sandbox;
  glide.keymaps.set(
    "normal",
    "gI",
    () => glide.hints.show({ auto_activate: true, editable: true, pick: hinting.pickers.biggest_area }),
    { description: "Focus the largest editable element on the page" },
  );

  glide.keymaps.set("normal", "yf", () =>
    glide.hints.show({
      selector: "[href]",
      async action({ content }) {
        let href = await content.execute((target) => (target as HTMLAnchorElement).href);
        if (href.startsWith("mailto:")) {
          href = href.slice(7);
        } else if (href.startsWith("tel:") || href.startsWith("sms:")) {
          href = href.slice(4);
        }
        await navigator.clipboard.writeText(href);
      },
    }), { description: "Yank the URL of the selected hintable link to the clipboard" });
}
