# gpui-core

Components for [GPUI](https://www.gpui.rs) apps on macOS and iOS, drawn in the host's theme.
It replaced GPUI Kit in this workspace's crates, covering what they use. See
[PLAN.md](PLAN.md) for the inventory and the build order.

![gpui-core's gallery in the dark theme: buttons in four sizes and five variants, a segmented
control, toggles, a slider and an open menu with a check, a submenu, a shortcut and a disabled
item](docs/gallery.png)

- **Theme.** `Theme` holds the colours, fonts and sizes. Its colour names are the web app's
  CSS variables in snake case (`background`, `muted_foreground`, `ring`, …). The host sets it
  with `Theme::set`, and components read it with `cx.theme()`.
- **Root.** It makes the theme's `font_size` the window's rem, so every size in rems follows
  the host's zoom. It draws the window's surface in the theme and moves the focus with Tab.
- **Setup.** `gpui_core::init(cx)` binds the components' keys and sets the default theme. It
  also runs by itself the first time a `Root` or an `InputState` is made, so a crate that embeds
  gpui-core's components in a host that never calls it still works; calling it twice does
  nothing.
- **A palette of your own.** The look comes from the global `Theme`. An element drawn on a
  palette that differs from it (a light card in a dark app) takes its colours per element:
  `Press::focus_ring_color`, `Input::caret_color` / `selection_color` / `placeholder_color`,
  `Slider::track_color` / `fill_color` / `thumb_color`, `Tooltip::background` / `text_color` /
  `border_color`, `Popover::colors`, `DatePicker::colors` / `MonthGrid::colors`.
- **Popover.** `Popover::new(id).open(open).on_dismiss(…).trigger(element).child(…)` hangs a
  panel from its trigger, or from a box or point in the window with `anchor`. It goes below, or
  above where there's more room, stays inside the window and scrolls past `max_height`. A click
  outside, Escape, or the trigger scrolling away calls `on_dismiss` with the reason; the host
  owns `open`. While open it has the keyboard, and on closing it hands the focus back.
- **Sheet.** `Sheet::new(id).open(open).on_dismiss(…).child(…)` is Popover's touch form: a
  panel across the bottom of the window over a scrim, centred and capped at `max_width` on a
  wide window, scrolling past nine tenths of its height. A press on the scrim or Escape calls
  `on_dismiss`; it takes the keyboard while open and hands it back on closing.
- **Code editor.** `InputState::code(window, cx)` with `Input::new(&state).size_full()` in a box
  the host sizes: lines that don't wrap (it scrolls sideways to the caret), the theme's
  monospace font, line numbers (`Input::line_numbers(false)` hides them), Tab and Shift-Tab
  indenting a line or every selected line, Enter keeping the indent, and Ctrl-Tab moving the
  keyboard on. Only the rows in view are drawn, so a file of thousands of lines types as fast as
  a short one. Undo, the input method and the rest are a field's. `set_error_line(Some((line,
  message)))` marks the line a server said is wrong (gone when the text changes),
  `highlight(f, "yaml")` colours it (the `highlight` feature's `Highlighter`, or the host's), and
  ⌘F opens a find bar: matches found case aside as you type, Enter and Shift-Enter (⌘G and ⌘⇧G
  from the text) going round them, Escape closing it.
- **SegmentedControl.** `SegmentedControl::new(id).segment(id, label)….selected(id)
  .on_change(…)`: a few options side by side, one on (a theme's System, Light and Dark). It's a
  radio group to assistive tech, with one Tab stop (the option that's on); the arrows, Home and
  End move along and turn on what they reach, passing over a disabled option and stopping at
  the ends. `touch(true)` makes its options 44 points tall; in a narrow space their labels are
  cut short.
- **Toast.** `Toast::success("Saved.").show(window, cx)` (or `new`, `error`) shows a note at the
  bottom of a window whose root is a `Root`, as the web's are: newest at the bottom, at most
  three, gone by themselves after four seconds (eight for an error or one with an `action`),
  each with a close button. `id(…)` replaces one shown with the same id; `description(…)` adds a
  muted line. An error is an alert to assistive tech, the rest a status. Nothing redraws while
  one shows but its one timer.
- **Card.** `Card::new().title(…).description(…).action(element).child(…).footer(element)`:
  a raised panel in the theme's `card` colours with a border and the large radius. The title
  and description wrap beside the action at the top right; the children are its content; the
  footer is a row along the bottom on a quieter fill. `small()` tightens its spacing. It draws
  only the parts it's given and takes no input of its own.
- **Alert.** `Alert::new(id).icon(IconName::Info).title(…).description(…)`, `.destructive()`
  for the variant in the destructive colour: a bordered box on the card colour with the icon
  level with the title and the text beside it, wrapping. Children go under the description. A
  destructive alert is an alert to assistive technology, the default one a status, named by
  its title; `role(Role::Note)` (or another) says otherwise.
- **PopupMenu.** `PopupMenu::new(id).item(PopupMenuItem::new(id, label)…).separator()
  .on_select(…)`, usually as a Popover's content. Entries take an icon, a check, a shortcut's
  text, a host-drawn chip, and can be disabled. Up and Down move a highlight over what can be
  picked (the pointer moves it too), Enter or a press picks, and Escape closes the popover.
  `PopupMenuItem::submenu(menu)` opens a menu beside its entry, on the right unless there's
  no room: on a press, Enter, Space or Right, or when the pointer rests on it; Left or Escape
  closes only it, and its picks go to the outer menu's `on_select` unless it has its own. It's
  drawn in the colours of the popover it's in, stays its entry's while entries come and go,
  can be made only when it opens (`submenu_with(|| …)`), and a menu can open with one open
  and the keyboard in it (`open_submenu(id)`).
  `touch(true)` makes rows 44 points tall. (`Menu` and `MenuItem` are GPUI's app menus.)
- **Picker.** `Picker::new(id).items(…).on_select(…)` is a PopupMenu with a field at the top:
  typing filters the entries (every word, case aside) and highlights the first match, Up and
  Down move through the matches while the field keeps the keyboard, Enter picks. A
  single-choice host closes its popover on a pick; a several-choice one keeps it open.
- **TextView.** `TextView::markdown(id, source)` draws GitHub Markdown natively: selectable
  with `selectable(true)`, links as `on_link_click` events, and `[text][label]` links
  resolved to their definitions. `TextView::blocks(id, [(key, source), …])` draws several
  keyed sources as one document (a streamed reply's settled blocks and its live tail): spaced,
  selected and copied as one view, with each source read again only when its text changes and
  each top-level block laid out again only when it changes. A selection in unchanged sources
  stays while later ones change. `trailing_caret(Some(colour))` draws a caret after the last
  source's text, painted only, so it never moves a line.
- **Syntax highlighting** (the `highlight` feature, off by default for the grammars' size).
  `Highlighter::new()` knows Rust, Swift, Go, TypeScript/TSX, JavaScript/JSX, Python, JSON,
  TOML, YAML, Bash, SQL, Markdown, HTML, CSS, C and C++ by the names code fences use (case
  aside, first word, with aliases such as `rs`, `py`, `yml`, `c++`), and hosts can `register`
  more tree-sitter grammars. `highlight(lang, code, &SyntaxTheme)` gives ranges and styles;
  `into_fn(theme)` makes the `HighlightFn` (`text::HighlightFn`, there without the feature too)
  that hosts hand their code views. Colours are the theme's `syntax_*` tokens, each at least
  4.5:1 on the page, a card and `muted`.
- **Rules.** Nothing redraws while nothing changes, text stays in its box at any width,
  everything works from the keyboard with a focus ring, nothing works by hover alone, and links
  and actions are events for the host.

Depend on GPUI under its own name, at the same version as this crate:

```toml
[dependencies]
gpui = { package = "gpui-pre", version = "=0.3.12" }
gpui-core = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<a commit>" }
# The app's window. Without `font-kit`, GPUI draws no text on macOS (GPUI Kit turned it on
# for you).
gpui_platform = { package = "gpui-pre-platform", version = "=0.3.12", features = ["font-kit"] }
```

```rust
use gpui::*;
use gpui_core::{ActiveTheme as _, Theme};

struct Hello;

impl Render for Hello {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().text_color(cx.theme().muted_foreground).child("Hello")
    }
}

fn run(cx: &mut App) {
    gpui_core::init(cx);
    Theme::set(Theme::dark(), cx);
    gpui_core::open_window(WindowOptions::default(), cx, |_, cx| cx.new(|_| Hello)).unwrap();
}
```

Components draw Lucide icons from gpui-core's assets, so the app serves them:
`gpui_platform::application().with_assets(gpui_core::Icons)`.

## The gallery

Every component in one window, from the repository's root:

```sh
cargo run -p gpui-core --example gallery              # dark; --light for the light theme
cargo run -p gpui-core --features shots --example gallery -- --shot out.png
```

`--shot` draws the window offscreen into a PNG without showing it, as the picture above was made.

## Licence

Apache-2.0.
