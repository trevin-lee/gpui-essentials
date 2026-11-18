# gpui-dnd

Drag and drop for [GPUI](https://www.gpui.rs) apps on macOS and iOS, in the manner of
[react-dnd](https://react-dnd.github.io/react-dnd/): drag sources, drop targets that nest, a
monitor of the drag, and a preview that follows the pointer, drawn in gpui-core's theme.
See [DESIGN.md](DESIGN.md) for the whole API and how it maps to react-dnd's.

Being built in small steps: the mouse, sources, targets, the monitor, the default preview,
custom drag layers and the animated return are in; the keyboard, touch, sortable lists,
auto-scroll and kanban come next.

## Install

From this repository, at a revision (it pins GPUI exactly, as the workspace's other crates do):

```toml
[dependencies]
gpui-dnd = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<rev>" }
```

## Use

Wrap the window's content in a `DndProvider`. With gpui-core it goes **inside** `Root`, as the
content of the view Root shows; the preview is drawn above Root's popovers, dialogs and toasts.

```rust,ignore
use gpui_dnd::*;

#[derive(Clone)]
struct Card(u32);

impl Render for Board {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        DndProvider::new(
            div()
                .size_full()
                .child(
                    dnd("card-1")
                        .source(DragSource::item(Card(1)).on_end(|end, _, _| {
                            if end.dropped { /* it moved */ }
                        }))
                        .dragging_style(|s| s.opacity(0.4))
                        .child("Write the brief"),
                )
                .child(
                    dnd("done")
                        .target(DropTarget::new().accept::<Card>().on_drop(|item, _, _, _| {
                            let card = item.get::<Card>().unwrap();
                            DropOutcome::Handled(None)
                        }))
                        .child("Done"),
                ),
        )
    }
}
// Root::new(cx.new(|_| Board::new()), window, cx)
```

- An item's type is its Rust type: a target `accept::<Card>()`s. `DragSource::many` drags a
  selection, and the preview counts it.
- Targets nest. The innermost under the pointer is asked first; it returns `Handled`, `Pass`
  (the next one out decides) or `HandledContinue` (both do).
- `DragMonitor::of(cx)` reads the drag anywhere; `cx.collect_dnd(|m| …)` keeps a value from it
  in a view's state and notifies the view only when that value changes.
- `Preview::Layer` with `DndProvider::layer(|m, _, _| …)` draws your own preview at
  `m.preview_origin()` (react-dnd's `useDragLayer`); `Preview::None` draws none.
- Escape, or a drop where nothing takes it, sends the preview back to its source in 200 ms; the
  source hears `on_end` when it lands. Nothing runs while nothing moves.
