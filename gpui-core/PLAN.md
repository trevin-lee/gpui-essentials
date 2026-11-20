# gpui-core plan

gpui-core is a GPUI component crate. It replaced GPUI Kit (`gpui-kit`, Longbridge, Apache-2.0)
in this workspace's crates, written from scratch. GPUI itself is `gpui-pre =0.3.12`, the
snapshot of Zed's GPUI that `gpui-kit =0.9.1` pins, so a host still linking the kit links the
same GPUI and can use both side by side while it moves over (setting both themes meanwhile).

## How the crate is shaped

- **No facade.** gpui-pre's derive and test macros (`#[derive(IntoElement)]`, `actions!`,
  `#[gpui::test]`) emit `gpui::…` paths. They rewrite those paths only when the calling crate
  depends on a crate named exactly `gpui-kit`, because that name is hard-coded in
  gpui-pre-macros. A gpui-core facade therefore can't make them resolve. Consumers depend on GPUI
  directly as `gpui = { package = "gpui-pre" }` (a workspace dependency), as Zed's own crates
  do, and on gpui-core for components. Moving from the kit is mechanical:
  - `gpui_kit::*` becomes `gpui::*`;
  - `gpui_kit::component::X` becomes `gpui_core::X`;
  - `#[gpui_kit::test]` becomes `#[gpui::test]`;
  - `gpui_kit::init` becomes `gpui_core::init`;
  - `gpui_kit::application()` becomes `gpui_platform::application()`.
- **iOS as well as macOS.** GPUI has an iOS backend (`gpui-pre-ios` 0.3.12), and a host may
  run the same components on iPhone and iPad. So:
  - gpui-core depends on `gpui` only, never on a platform crate or a macOS-only API;
  - nothing works by hover alone (tooltips have a press-and-hold or focus path);
  - `Sizable` offers a touch size with targets of at least 44 pt;
  - text input goes only through GPUI's platform input handler, so IME and secure entry work
    on both platforms.
  CI builds gpui-core for `aarch64-apple-ios-sim`.
- **Theme from the host.** `Theme` is a plain struct that the host installs as a GPUI global
  (`Theme::set(theme, cx)`, read with `cx.theme()`). Its color fields use shadcn/ui's
  `globals.css` token names in snake_case: `background`, `foreground`, `card`, `popover`,
  `primary`, `secondary`, `muted`, `muted_foreground`, `accent`, `border`, `input`, `ring`,
  `destructive`, `sidebar`, and so on, the names in `design/tokens.json`, so a host with
  tokens of those names can pass its theme straight through. Other fields: font family and
  size, mono family and size, radii. The built-in default is a charcoal dark theme. There is no
  light/dark switching logic in the crate.
- **Zoom.** Sizes are in rems. `Root` (optional) sets the window's rem from the theme's font
  size, so ⌘+ and ⌘− scale everything.
- **Rules every component follows:**
  - nothing redraws while idle (no timers or animation frames unless something is moving);
  - animations run on GPUI's frame clock and are never stepped;
  - text never overflows its box;
  - every component works from the keyboard and shows a focus ring drawn with `ring`;
  - links, actions and images are emitted as events for the host, and the crate never fetches
    or opens anything itself.
- **Icons.** Lucide's whole set is vendored from upstream (ISC; its licence sits beside the
  SVGs and is credited in NOTICE) by `scripts/lucide.py`. The crate serves it at
  `icons/<name>.svg` through an `AssetSource` (`Icons`) and names it with `IconName`. Hosts can
  put their own assets in front. It's always built in; the minified SVGs add about 750 KB.
- **Tests.**
  - Unit and interaction tests per component.
  - A headless overflow test that renders every component with long, unbreakable and
    non-Latin text at widths from 40 px to 1600 px and zooms from 50% to 200%, and asserts
    that no text box exceeds its parent.
  - An idle test asserting that no frames are requested after settling.
  - A `shots` feature for pictures, like the other crates have.

## Popover, Menu, Picker

Shared overlays, in three layers, each on the one before:

1. **Popover.**
   - **Anchoring:** it anchors to an element's bounds. It goes below by default, flips above or
     shifts sideways at the window's edge, and is at least as wide as its anchor, at most as wide
     as the window, with a maximum height and its own scrolling.
   - **Closing:** a click outside, Escape, or its anchor scrolling out of view closes it, and a
     second press on the anchor toggles it.
   - **Focus:** while open it holds the keyboard. On close the focus goes back to whatever opened
     it, or to a focus handle the host names (a table's cell focus).
   - **Content:** it hosts any content, such as the date grid.
   **Sheet** is the same API for touch: a bottom sheet with 44-point rows, for a choice or
   details shown at an element on the Mac and as a sheet on touch.
2. **Menu.**
   - **Entries:** each has an icon and a label, can be checked (toggles, the current option) or
     disabled, and can carry a host-drawn chip (a select option's colour). There are
     separators, and 44 pt entries on touch.
   - **Keys:** Up and Down move a highlight that the pointer moves too, Enter picks, Escape
     closes.
3. **Picker.** A Menu with an Input at the top that filters its entries as you type. A
   single-select picker closes on a pick; a multi-select one stays open.

Rules for all three, beyond the crate's:
- no global state, and nothing drawn while closed;
- colours from a palette the host passes in (gpui-agents' `CardTheme` and gpui-tables'
  `TableTheme` have their own), with the global Theme as the default.

## Licence hygiene

gpui-core is written from scratch. The kit is studied for behaviour, not copied. If any code is
adapted from it, the kit's Apache-2.0 notice goes into the repository's NOTICE, with a comment
at the adapted code. Lucide's ISC licence is in NOTICE.

## Possible later: shaping off the main thread

Measured 2026-11-17 for gpui-tables' 10,000-row table, release, on an M4 Pro. After a sort,
filter and group, the draw takes about 8 ms. About 5 ms of that is Core Text shaping the
roughly 180 lines that come into view, about 30 µs a line, and the rest is an ordinary frame
(about 2.5 ms). A frame that scrolls a page of new rows costs the same: about 11 ms, 159 lines
shaped. Frames that bring in nothing new shape nothing, because GPUI's line cache keeps the
last frame's lines. A cache can't help here, since the text was never shaped before.

What would help is shaping the coming rows on a background thread (Core Text is thread-safe)
and drawing them with a text element of gpui-core's own that paints pre-shaped lines. It saves
about 5 ms on a sort or a page jump. Ordinary scrolling brings in a few lines a frame. Not
done: a custom text element is too much risk for 5 ms.
