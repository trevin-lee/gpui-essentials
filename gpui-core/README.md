# gpui-core

Components for [GPUI](https://www.gpui.rs) apps on macOS and iOS, drawn in the host's theme.
It replaces GPUI Kit in this workspace's crates, covering what they use. See
[PLAN.md](PLAN.md) for the inventory and the build order.

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
  `border_color`, `Popover::colors`.
- **Popover.** `Popover::new(id).open(open).on_dismiss(…).trigger(element).child(…)` hangs a
  panel from its trigger, or from a box or point in the window with `anchor`. It goes below, or
  above where there's more room, stays inside the window and scrolls past `max_height`. A click
  outside, Escape, or the trigger scrolling away calls `on_dismiss` with the reason; the host
  owns `open`. While open it has the keyboard, and on closing it hands the focus back.
- **Sheet.** `Sheet::new(id).open(open).on_dismiss(…).child(…)` is Popover's touch form: a
  panel across the bottom of the window over a scrim, centred and capped at `max_width` on a
  wide window, scrolling past nine tenths of its height. A press on the scrim or Escape calls
  `on_dismiss`; it takes the keyboard while open and hands it back on closing.
- **PopupMenu.** `PopupMenu::new(id).item(PopupMenuItem::new(id, label)…).separator()
  .on_select(…)`, usually as a Popover's content. Entries take an icon, a check, a shortcut's
  text, a host-drawn chip, and can be disabled. Up and Down move a highlight over what can be
  picked (the pointer moves it too), Enter or a press picks, and Escape closes the popover.
  `touch(true)` makes rows 44 points tall. (`Menu` and `MenuItem` are GPUI's app menus.)
- **Picker.** `Picker::new(id).items(…).on_select(…)` is a PopupMenu with a field at the top:
  typing filters the entries (every word, case aside) and highlights the first match, Up and
  Down move through the matches while the field keeps the keyboard, Enter picks. A
  single-choice host closes its popover on a pick; a several-choice one keeps it open.
- **Rules.** Nothing redraws while nothing changes, text stays in its box at any width,
  everything works from the keyboard with a focus ring, nothing works by hover alone, and links
  and actions are events for the host.

Depend on GPUI under its own name, at the same version as this crate:

```toml
[dependencies]
gpui = { package = "gpui-pre", version = "=0.3.12" }
gpui-core = { git = "https://github.com/trevin-lee/gpui-essentials" }
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

## Licence

Apache-2.0.
