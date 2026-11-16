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
gpui-blocks' markdown-editor fork (`trevin-lee/gpui-kit-markdown-editor@0ce34f8`) and the Mac
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
7. **The markdown engine under the blocks fork.** The fork has its own IME, keymap, model and
   undo. It draws through the kit's `base::text::editor_host`, the kit's 34k-line text engine,
   with about 190 references across 15 files. The fork's toolbar is unused, so it isn't needed.
   This is the largest single piece. When the blocks crate's turn comes, I'll choose between
   giving gpui-core a general markdown layout engine that TextView shares, and moving the
   fork's drawing onto plain GPUI text layout. gpui-blocks and its fork switch after this.
8. **The Mac app**, last. It needs 1–5 and Switch, and moves in one PR in open-cloud-agents
   that Main merges.

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
