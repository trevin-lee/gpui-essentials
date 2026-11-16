# gpui-core plan

gpui-core is a GPUI component crate that replaces GPUI Kit (`gpui-kit`, Longbridge,
Apache-2.0) in this workspace's crates and in the apps that use them. It covers only what
those crates and apps use, written from scratch. GPUI itself stays where it is:
`gpui-pre =0.3.12`, the snapshot of Zed's GPUI that `gpui-kit =0.9.1` pins. Because of that,
a host still linking the kit links the same GPUI during the migration.

## What gets replaced

GPUI Kit is a facade over four layers: GPUI (`gpui-pre`, re-exported at its root), `gpui-base`
(~138k lines), `gpui-component` (~113k lines) and `gpui-kit-assets` (1,830 Lucide icons). The
crates use only a small part of it. The inventory below comes from `rg` over the four crates,
gpui-blocks' engine (`gpui-blocks/engine`) and the Mac
app (`open-cloud-agents/clients/desktop`), on 2026-11-15.

| Item (kit path) | flow | calendar | chatwidgets | blocks (+fork) | Mac app |
| --- | :-: | :-: | :-: | :-: | :-: |
| GPUI through the kit (`gpui_kit::*`, `prelude::FluentBuilder`, `actions!`) | ● | ● | ● | ● | ● |
| `init`, `application`, `open_window`, `#[gpui_kit::test]` (99 uses) | ● | ● | ● | ● | ● |
| `Theme`, `ThemeMode`, `ActiveTheme` (colors, fonts, radii) | ● | ● | ● | ● | ● |
| Root: rem from `theme.font_size` every frame, surface font and colours, Tab / shift-Tab focus, ⌘C across selected text | ● | ● | ● | ● | ● |
| Unstyled `base::Button` (role, accessibility label, disabled, selected, on_click) | | | ● (~18) | | |
| Styled `Button` / `ButtonGroup` / `ButtonVariants`, `Selectable`, `Sizable`, `Disableable` | | ● | | (fork toolbar, unused) | ● (`Disableable`) |
| Icons: `IconName` (ChevronLeft/Right), `assets::IconName::ALL`, `Assets` (Lucide) | | ● | ● | ● | ● |
| `Tooltip::new` | | | ● | | ● (10) |
| `Input` / `InputState` / `InputEvent`, `Textarea` / `TextareaState`, `OutdentInline` | | | ● | | ● (~30) |
| `TextView` / `TextViewStyle` (markdown display) | | | ● | ● | ● |
| `Switch` | | | | | ● (6) |
| `Slider` / `SliderState` / `SliderEvent` | | | ● | | |
| `DatePicker` / `DatePickerState` / `DatePickerEvent` | | | ● | | |
| `base::text` markdown model: `MarkdownNode`, `MarkdownPlugin`, `MarkdownExtensions`, `editor_host::{BlockNode, TextMark, LinkMark, ColumnAlign, parse_markdown, …}` | | | | ● | |
| `Toggle`, `DropdownMenu` / `PopupMenuItem`, `Toolbar`, `Separator`, `StyledExt`, `SemanticThemeTokens`, `base::input::Paste` | | | | ● (fork) | |

The chatwidgets session's fuller inventory agrees with this one. It adds the call counts:
inputs are built 27 times in 18 app files plus chatwidgets, with `.placeholder`,
`.appearance(false)`, `.auto_grow`, `.masked`, `.submit_on_enter` and the events PressEnter,
Change, Focus and Blur. It also lists two kit quirks the app works around: the Input's 10 px
padding even with `appearance(false)`, and the textarea placeholder overflowing. gpui-core
fixes both instead of copying them.

Nothing else in the kit is used. There are no menus, popovers, dialogs, sheets,
notifications, lists, tables, trees, scrollbars, Dock or Resizable: the app builds its own on
GPUI's `deferred` and `anchored`. gpui-flow draws its own controls and icons. It needs only GPUI, the theme's colors and
`init`.

## How the crate is shaped

- **No facade.** gpui-pre's derive and test macros (`#[derive(IntoElement)]`, `actions!`,
  `#[gpui::test]`) emit `gpui::…` paths. They rewrite those paths only when the calling crate
  depends on a crate named exactly `gpui-kit`, because that name is hard-coded in
  gpui-pre-macros. A gpui-core facade therefore can't make them resolve. Consumers depend on GPUI
  directly as `gpui = { package = "gpui-pre" }` (a workspace dependency), as Zed's own crates
  do, and on gpui-core for components. The migration is mechanical:
  - `gpui_kit::*` becomes `gpui::*`;
  - `gpui_kit::component::X` becomes `gpui_core::X`;
  - `#[gpui_kit::test]` becomes `#[gpui::test]`;
  - `gpui_kit::init` becomes `gpui_core::init`;
  - `gpui_kit::application()` becomes `gpui_platform::application()`.
- **iOS as well as macOS.** GPUI has an iOS backend (`gpui-pre-ios` 0.3.12), and the shared
  editors may run in the iPhone and iPad apps. So:
  - gpui-core depends on `gpui` only, never on a platform crate or a macOS-only API;
  - nothing works by hover alone (tooltips have a press-and-hold or focus path);
  - `Sizable` offers a touch size with targets of at least 44 pt;
  - text input goes only through GPUI's platform input handler, so IME and secure entry work
    on both platforms.
  CI builds gpui-core for `aarch64-apple-ios-sim`.
- **Theme from the host.** `Theme` is a plain struct that the host installs as a GPUI global
  (`Theme::set(theme, cx)`, read with `cx.theme()`). Its color fields use the web's
  `globals.css` token names in snake_case: `background`, `foreground`, `card`, `popover`,
  `primary`, `secondary`, `muted`, `muted_foreground`, `accent`, `border`, `input`, `ring`,
  `destructive`, `sidebar`, and so on. These are the names the Desktop session is writing into
  `open-cloud-agents/design/tokens.json`, so the Mac app can pass its theme straight through.
  When that file lands I'll match the names to it exactly. Other fields: font family and size,
  mono family and size, radii. The built-in default is the Mac app's charcoal dark theme. There
  is no light/dark switching logic in the crate.
- **Zoom.** Sizes are in rems. `Root` (optional) sets the window's rem from the theme's font
  size, as the kit does, so ⌘+ and ⌘− scale everything.
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

## Build order (most used first)

1. **Theme + Root + init + `open_window`.** All five consumers use these. This is enough to
   unlock gpui-flow.
2. **Icons + Button** (with group, variants, selected, sizes, disabled). Unlocks gpui-calendar.
3. **Tooltip**, then **Switch** and **Slider**.
4. **Input and Textarea.** This is the hardest piece and the Mac app's most used one:
   - IME through GPUI's input handler, selection by mouse, word and line;
   - clipboard and undo/redo;
   - wrapping and scrolling, placeholder, masked input;
   - the standard macOS key bindings, plus `OutdentInline`.
5. **TextView** (markdown display with styles; links as events), on `pulldown-cmark`.
6. **DatePicker** (popover with a month grid). Together with 3–5, this unlocks
   gpui-chatwidgets.
7. **A rich-text paragraph layer, for the blocks fork.** The fork owns its model, IME, keymap
   and undo, but draws through the kit's 34k-line text engine (`base::text::editor_host`). Agreed
   with the gpui-blocks session (2026-11-15):
   - gpui-core gets one paragraph layer, shared with TextView (step 5) and with the Input's
     multi-line wrapping (step 4). It covers styled runs plus inline boxes (pictures, mention
     chips) on a baseline, wrapping to a width, index↔point both ways, and selection and range
     painting. GPUI's `StyledText` can't hold inline boxes, so the layer shapes runs itself. It
     may be built before step 4, since Input needs the same shaping and hit testing.
   - The fork's block chrome (headings, lists, quotes, callouts, toggles, code blocks, tables)
     stays in the fork, drawn with plain GPUI on the layer.
   - The fork's Markdown import moves to `markdown` (mdast) directly. That's the gpui-blocks
     session's work and needs nothing from gpui-core.
   The blocks session will write the fork's needs as tests against the layer's API.
8. **The Mac app**, last. It needs 1–5 and Switch, and moves in a few small PRs in
   open-cloud-agents, one area each, that Main merges: see "The Mac app's switch" below.

## After the kit: Popover, Menu, Picker

No crate used the kit's popovers or menus: the Mac app, gpui-blocks' slash menu,
gpui-chatwidgets' select and gpui-tables' header menus each build their own on GPUI's
`deferred(anchored())`. gpui-tables asked for shared ones (2026-11-16), in three layers, each on
the one before:

1. **Popover.**
   - **Anchoring:** it anchors to an element's bounds. It goes below by default, flips above or
     shifts sideways at the window's edge, and is at least as wide as its anchor, at most as wide
     as the window, with a maximum height and its own scrolling.
   - **Closing:** a click outside, Escape, or its anchor scrolling out of view closes it, and a
     second press on the anchor toggles it.
   - **Focus:** while open it holds the keyboard. On close the focus goes back to whatever opened
     it, or to a focus handle the host names (the table's cell focus).
   - **Content:** it hosts any content, such as the date grid. DatePicker's month moves onto it.
   **Sheet** is the same API for touch: a bottom sheet with 44-point rows. gpui-calendar asked
   for it (2026-11-16) for its recurring-edit choice and event details: a popover at the
   event on the Mac, a sheet on touch. The planned shape is
   `Popover::new(id).anchor(bounds or point).open(open).on_dismiss(…).child(content)`; the
   calendar draws a stand-in in that shape until it lands.
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
- colours from a palette the host passes in (gpui-chatwidgets' `CardTheme` and gpui-tables'
  `TableTheme` have their own), with the global Theme as the default.

## Switch order

1. **gpui-flow first**, after step 1. Its kit use is GPUI through `gpui_kit::*`,
   `Theme::global(cx).colors` in `FlowColors::from_theme`, and `init` in its tests. It's the
   smallest real proof of the path change, the test macro and the theme.
2. gpui-calendar, after step 2.
3. gpui-chatwidgets, after step 6.
4. gpui-blocks and its fork, after step 7.
5. The Mac app, after everything above.

A crate switches only when gpui-core covers all of its use and the crate's own tests, overflow
test and clippy all pass. Each switch is its own PR.

## The Mac app's switch

Inventory of `open-cloud-agents/clients/desktop/app` at `4d4ca666` (2026-11-16), from `rg` over
its sources. It already links gpui-core (pinned by rev) next to GPUI Kit 0.9.1, and draws
Popover-like and menu-like things itself, so the kit use left is small.

| Kit item | Uses | Files | gpui-core |
|---|---:|---:|---|
| `use gpui_kit::*` (GPUI re-exported) | 44 | 44 | `use gpui::*` (the `gpui-pre` crate) |
| `gpui_kit::prelude::FluentBuilder` | 33 | 33 | `gpui::prelude::FluentBuilder` |
| `gpui_kit::px`, `actions`, `Keystroke`, `RenderImage`, `canvas`, `Bounds`, `Window` | ~60 | – | GPUI's own |
| `InputState::new` (+ `.placeholder` 23, `.masked` 2) | 34 | 21 | `InputState::new`, same builders |
| `Input::new` / `Input as Field` (+ `.appearance(false)` 10) | 10 | 9 | `Input::new(..).appearance(..)` |
| `TextareaState::new(..).auto_grow(1, 6).submit_on_enter(true).autocorrect(..).autocapitalize(..)` | 6 | 6 | `InputState::textarea(..).auto_grow(..).submit_on_enter(..)`; a textarea's default keyboard is already sentence case with suggestions (`.keyboard(..)` otherwise) |
| `Textarea::new` | 6 | 6 | `Input::new` |
| `InputEvent::{PressEnter, Change, Blur, Focus}` | 25 / 17 / 4 / 2 | – | the same, `PressEnter { secondary }` |
| state `value`, `set_value(text, window, cx)`, `focus`, `is_focused`, `set_placeholder` | – | – | the same |
| state `select_all(window, cx)` | 5 | 5 | #130 |
| state `set_masked(bool, window, cx)` | 1 | 1 | #130 |
| `OutdentInline` (the composer's Shift-Tab steps its permission mode) | 1 | 1 | capture `gpui_core::FocusPrev`, Shift-Tab's action under Root |
| `component::tooltip::Tooltip::new(..).build(window, cx)` | 14 | 9 | `Tooltip::new(..).build(window, cx)` |
| `component::switch::Switch::new(..).checked(..)` (+ `Disableable` 4) | 11 | 6 | `Switch`, `Disableable` |
| `component::text::{TextView, TextViewStyle}` (the chat's Markdown) | 1 | 1 | `TextView::markdown(..).style(..).selectable(..)`; `heading_base_font_size` takes `Some(..)` |
| `component::{Theme, ThemeMode}` (the kit's theme, set in `theme.rs`) | 1 fn | 1 | deleted with the kit; `gpui_core::Theme` is set there already |
| `gpui_kit::init` / `open_window` / `application` | 4 / 1 / 1 | 3 | `gpui_core::init` / `open_window` / `gpui_platform::application` |
| `gpui_kit::assets::Assets` (the kit's icons, last in `AppAssets`) | 1 | 1 | dropped: nothing draws a kit icon once its components are gone |

Not used at all: the kit's Root (beyond `open_window`), Button, Icon, popovers, menus, lists,
scroll areas, `h_flex`/`v_flex`, `StyledExt`, `Sizable`. Everything is covered once #130 lands.

The switch, one area per PR, each bumping the app's gpui-core pin, with the app's own tests,
clippy and pictures checked before and after:

1. **The composer** onto `InputState::textarea` (`composer.rs`): the field the GPUI phone app's
   keyboard work shares.
2. **The forms**: the settings, rules, vault and other one-line fields, with `select_all` and
   `set_masked`.
3. **Switches and tooltips.**
4. **The chat's Markdown** onto gpui-core's TextView, its pictures compared.
5. **The rest and the dependency**: `init`, `open_window`, `application`, the assets fallback,
   the kit's theme, `gpui_kit::*` → `gpui::*`, and `gpui-kit` out of `Cargo.toml`.

Until 5, both libraries are linked and both themes set, as below.

## Coexistence during the migration

Both libraries link the same `gpui-pre`, so a host can use the kit and gpui-core side by side.
During that time the host sets both themes. The Mac app already maps its tokens onto the kit's
`Theme` in `app/src/theme.rs`, and will do the same for `gpui_core::Theme` until it switches.

## Licence hygiene

gpui-core is written from scratch. The kit is studied for behaviour, not copied. If any code is
adapted from it, the kit's Apache-2.0 notice goes into the repository's NOTICE, with a comment
at the adapted code. Lucide's ISC licence goes into NOTICE when the icons are vendored.

## CI

`.github/workflows/gpui-core.yml` is copied from `gpui-flow.yml`: macos-latest, fmt, clippy
(`-D warnings`, also with `shots`), test, and doc with warnings denied. A second job runs
`cargo build -p gpui-core --target aarch64-apple-ios-sim`.
