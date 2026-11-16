# gpui-core

Components for [GPUI](https://www.gpui.rs) apps on macOS and iOS, drawn in the host's theme.
It replaces GPUI Kit in this workspace's crates, covering what they use. See
[PLAN.md](PLAN.md) for the inventory and the build order.

- **Theme.** `Theme` holds the colours, fonts and sizes. Its colour names are the web app's
  CSS variables in snake case (`background`, `muted_foreground`, `ring`, …). The host sets it
  with `Theme::set`, and components read it with `cx.theme()`.
- **Root.** It makes the theme's `font_size` the window's rem, so every size in rems follows
  the host's zoom. It draws the window's surface in the theme and moves the focus with Tab.
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
