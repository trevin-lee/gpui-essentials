# gpui-dnd

Drag and drop for [GPUI](https://www.gpui.rs) apps on macOS and iOS, in the manner of
[react-dnd](https://react-dnd.github.io/react-dnd/): drag sources, drop targets that nest, a
monitor of the drag, and a preview that follows the pointer. On top of those come sortable
lists, kanban boards and auto-scroll. Drags work by mouse and trackpad, by touch, and by
keyboard, with each step said to VoiceOver. Everything is drawn in gpui-core's theme and
follows ⌘+ and ⌘− zoom. Nothing runs while nothing moves.

![A kanban board: a card lifted out of To do and held over Doing, where the cards have parted
to open a gap for it](docs/kanban-drag.png)

See [DESIGN.md](DESIGN.md) for the whole API, the decisions behind it, and what the
workspace's other crates would need to move their own drag code onto it.

## Install

From this repository, at a revision. It pins GPUI exactly, as the workspace's other crates do,
so your app must use the same GPUI (gpui-pre 0.3.12).

```toml
[dependencies]
gpui-dnd = { git = "https://github.com/trevin-lee/gpui-essentials", rev = "<rev>" }
```

Wrap your window's content in a `DndProvider`. With gpui-core, it goes **inside** `Root`, as
the content of the view Root shows. The preview is drawn above Root's popovers, dialogs and
toasts.

```rust,ignore
use gpui_dnd::*;

impl Render for Board {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        DndProvider::new(div().size_full().child(self.columns(cx)))
    }
}
// gpui_core::open_window(options, cx, |_, cx| cx.new(|_| Board::new()))
```

## Sources and targets

```rust,ignore
#[derive(Clone)]
struct Card(u32);

// A drag source and a drop target are `dnd` elements: divs with an id.
dnd("card-1")
    .label("Write the brief")
    .source(DragSource::item(Card(1)).on_end(|end, _, _| {
        if end.dropped { /* it moved */ }
    }))
    .dragging_style(|s| s.opacity(0.4))
    .child("Write the brief");

dnd("done")
    .label("Done")
    .target(
        DropTarget::new()
            .accept::<Card>()
            .can_drop(|item, _, _| item.get::<Card>().is_some_and(|c| c.0 != 0))
            .on_hover(|_, m, _, _| { /* m.offset_in_target(), m.is_over(Over::Shallow) */ })
            .on_drop(|item, _, _, _| {
                let card = item.get::<Card>().unwrap();
                DropOutcome::Handled(None)
            }),
    )
    .child("Done");
```

- An item's type is its Rust type: a target `accept::<Card>()`s. `DragSource::many` drags a
  selection, and the default preview shows how many.
- Many sources in one view (a board's cards): build one `DragSource::keyed(|key, cx| item)` per
  view, `.shared()`, and give each element `dnd(id).shared_source(&shared, key)`: nothing per
  element but its key; the item is made from the key when a drag begins.
- Targets nest. The innermost under the pointer is asked first. It returns `Handled`, `Pass`
  (the next one out decides) or `HandledContinue` (both do, and the outer one sees
  `did_drop()`).
- `DragMonitor::of(cx)` reads the drag anywhere. `cx.collect_dnd(|m| …)` keeps a value from it
  in a view's state, and notifies the view only when that value changes.
- Escape, or a drop where nothing takes it, sends the preview back to its source in 200 ms. The
  source hears `on_end` when it lands.

## Sortable lists and kanban

```rust,ignore
Sortable::new("todo", self.todo.clone())
    .group("board")              // lists with the same group take each other's items
    .label("To do")
    .item_label(|c: &Card| c.title.clone())
    .item(|card, item, _, cx| card_view(card, item, cx))
    .on_move(move |m: &SortableMove<Card>, _, cx| { /* remove at m.from, insert at m.to */ })
    .scroll(&self.todo_scroll)   // scrolls, and auto-scrolls near its ends during a drag
```

For long lists, `VirtualSortable::new(id, keys, &scroll_handle)` takes the same builder methods
over a `uniform_list`: only the rows on screen are drawn, and drags, auto-scroll and the keyboard
work through rows that aren't drawn yet (rows must be the same height).

The items part to open a gap where the dragged one would land, then slide back if it doesn't.
`Placeholder::Line` draws a line instead, and `Placeholder::Custom` fills the gap with your
element. You own the order: apply each `SortableMove` to your keys. A kanban board is a row of
`Sortable`s with one group.

![A list of tasks with one lifted and held lower, the others parted around the gap, beside a
row of tag chips](docs/sortable-drag.png)

## Keyboard, touch and auto-scroll

- **Keyboard.** A free-standing source is a Tab stop, and Space or Enter picks it up. A
  `Sortable` is one Tab stop: the arrows move the focus to an item, then Space picks it up.
  During the drag, the arrows move to the nearest place in their direction that can take the
  item (one place at a time in a list, across to the next column too). Space or Enter drops,
  and Escape cancels. Each step is said in a live region, from the sources' and targets'
  labels: "Over Doing, position 2 of 3." `DndProvider::announcements` rewords it.
- **Touch.** A long press picks a source up, so a touch that moves at once still scrolls the
  list. `DragSource::touch_activation(TouchActivation::Immediate)` picks up as soon as the
  finger moves, for handles.
- **Auto-scroll.** `.auto_scroll(&handle, AutoScroll::vertical())`, called last on a scroll
  container, scrolls it while a drag is near its edges, faster the nearer, until the pointer
  leaves the edge. `Scrollable` covers your own scrollers.

## Drags the host starts

A view that draws its items on one canvas (a calendar's events) recognises its own presses and
hands them over:

- `press_at(window, cx, id, at, source, Press { backend, threshold, grab, size, label })` from
  the press's handler becomes a drag once the pointer moves past the threshold. A press that
  doesn't move that far stays the host's click.
- `begin_at` starts one at once, from a long press or a key; with `Backend::Keyboard`, the
  target's `keyboard_step` and `describe` drive it.
- `cancel(window, cx)` ends it, and `DragSource::cursor` picks the cursor by what's dragged.
- `DropTarget::catch_outside(true)` keeps hearing the drag anywhere in the window and takes its
  drop there, for a canvas that clamps to itself.
- `auto_scroll_zone(window, cx, bounds, scroller, options)` scrolls a band of the canvas, with
  `AutoScroll::for_items` and `beyond_edges`.

## Previews

The default preview is a card of the source's size, lifted, with the grab point kept under the
pointer; `Preview::build` draws your own element there instead. `Preview::Layer` hands the
drawing to the provider's layer, which is react-dnd's `useDragLayer`: you get the monitor and
the preview's origin each frame. `Preview::None` draws nothing.

![Two photos picked up together, drawn by a custom layer as a stack with a count, held over an
album](docs/custom_preview-drag.png)

![Nested targets: a file held over a folder inside a drive; the folder shows it's under the
pointer and the drive that it's inside](docs/nested-hover.png)

## react-dnd → gpui-dnd

| react-dnd | gpui-dnd |
| --- | --- |
| `<DndProvider backend={HTML5Backend}>` | `DndProvider::new(content)`, with `.mouse(on)`, `.touch(on)`, `.keyboard(on)` |
| `HTML5Backend`, `TouchBackend` | built in: `Backend::Mouse`, `Backend::Touch` (a long press, or `TouchActivation::Immediate`) |
| — | `Backend::Keyboard`: Space, the arrows and Escape, with announcements |
| `useDrag({ type, item, canDrag, end, isDragging })` | `dnd(id).source(DragSource::new(\|cx\| item).can_drag(…).on_end(…).is_dragging(…))` |
| `item` as a function | `DragSource::new(\|cx\| …)`; `DragSource::item(value)` for a fixed one |
| — | `DragSource::many(\|cx\| vec![…])`: a selection; `item.all::<T>()`, `item.count()` |
| item type (a string or symbol) | the item's Rust type: `ItemType::of::<T>()` |
| `useDrop({ accept, canDrop, hover, drop })` | `dnd(id).target(DropTarget::new().accept::<T>().can_drop(…).on_hover(…).on_drop(…))` |
| `drag(drop(ref))` | `.source(…).target(…)` on one `dnd` |
| `drop` returning an object | `DropOutcome::Handled(Some(DropResult::new(…)))`, read with `end.result` |
| an outer target checking `monitor.didDrop()` | `DropOutcome::HandledContinue` or `Pass`; `monitor.did_drop()`, `monitor.drop_result()` |
| `monitor.isOver({ shallow: true })` | `is_over(id, Over::Shallow)`, `Over::Deep` |
| `monitor.canDrop()` | `can_drop_on(id)`; `DropMonitor::can_drop()` |
| `getItem()`, `getItemType()` | `item()`, `item_type()` |
| `getInitialClientOffset()`, `getClientOffset()` | `initial_client_offset()`, `client_offset()` |
| `getDifferenceFromInitialOffset()`, `getSourceClientOffset()` | `difference_from_initial_offset()`, `source_client_offset()` |
| `collect` → props | `cx.collect_dnd(\|m\| …)` → `Collected<C>`; or `dragging_style`, `over_style`, `can_drop_style` |
| `useDragLayer` | `DndProvider::layer(\|m, _, _\| …)` with `Preview::Layer`; `m.preview_origin()` |
| `connectDragPreview(getEmptyImage())` | `Preview::None` |
| `end(item, monitor)` with `didDrop()` | `on_end(\|end: &DragEnd<T>, _, _\| …)`: `end.dropped`, `end.target`, `end.result`, `end.cancelled` |
| react-dnd-scrolling | `.auto_scroll(&handle, AutoScroll::default())` |
| react-sortable-hoc, dnd-kit's sortable | `Sortable` (`group` for kanban, `Placeholder`, `can_move`) |

## Examples

```sh
cargo run -p gpui-dnd --example kanban          # columns of cards, by mouse, touch and keyboard
cargo run -p gpui-dnd --example sortable        # a list, and a row of chips with a line placeholder
cargo run -p gpui-dnd --example nested          # nested targets, shallow and deep, passing drops out
cargo run -p gpui-dnd --example custom_preview  # a custom drag layer for a selection
```

The pictures come from the examples, rendered offscreen on macOS:
`cargo run -p gpui-dnd --features shots --example kanban -- --shot out.png --scene drag`. Each
example lists its scenes.
