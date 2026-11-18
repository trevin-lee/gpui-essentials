# gpui-dnd design

A drag-and-drop toolkit for GPUI apps on macOS and iOS: what react-dnd gives a React app,
written for GPUI. It takes react-dnd's concepts (sources, targets, a monitor, collected state,
a drag layer, backends) and none of its code. On top it has what people build with react-dnd
by hand: a sortable list, auto-scroll at a scroll container's edge, and drops between lists
(kanban). It's drawn in gpui-core's theme and works at ⌘+ and ⌘− zoom.

## Decisions

### Beside GPUI's drag, not on it

GPUI 0.3.12 has a drag of its own: `on_drag(value, |value, offset, window, cx| view)` on a
source, `drag_over::<T>(style)`, `can_drop` and `on_drop::<T>` on targets, `on_drag_move::<T>`,
and `cx.active_drag`, one per app. It's good for a plain mouse drag between two elements, and
gpui-tables uses it. It can't carry what this crate has to give:

- **No end for the source.** `active_drag` is cleared on any mouse-up (`window.rs`, after the
  listeners run), so the source never hears whether or where it was dropped, and nothing can
  animate the preview back after an Escape or a drop outside.
- **No touch.** The preview is painted at `window.mouse_position() - cursor_offset`, and a
  touch drag sets `mouse_position` to where the touch *started*. gpui-tables draws its finger
  ghost itself for this reason.
- **No keyboard.** A drag starts only from a mouse move past the threshold.
- **No nesting rules.** The first matching `on_drop` in bubble order takes the value and stops
  propagation; there's no shallow and deep over, no way to pass a drop out to an outer target,
  and no drop result.
- **No hover with offsets per target,** and no monitor: a view can't ask "is something over
  me, can it drop here, where did the drag start".
- **One per app, typed by Rust type only,** and private (`cx.active_drag` is `pub(crate)`), so
  a crate can't start or end one itself.

So gpui-dnd keeps its own drag state in an app global and uses only GPUI's public primitives:
`window.on_mouse_event` for the mouse, `LongPressEvent` and `TouchDragEvent` for touch, key
listeners for the keyboard, `window.defer_draw` for the drag layer, element state for the
registrations, and `request_animation_frame` only while something moves. It never touches
`cx.active_drag`, so it coexists with code still using GPUI's drag (a GPUI drag and a gpui-dnd
drag can't both be in progress: a gpui-dnd source doesn't start while `cx.has_active_drag()`).

What it gives up: GPUI suppresses `.hover()` styles while its own drag is active, and can hand
a drag to the OS (`external_drag_payload`) when it leaves the window. Neither applies to a
gpui-dnd drag: elements under the preview keep their hover styles (the crate's own elements
don't use `.hover()` while a drag is on), and drags stay inside their window. Dragging files in
from Finder stays GPUI's `on_drop::<ExternalPaths>`.

### Item types are Rust types

react-dnd's item type is a string or symbol, and the item a plain object. Here the item is
any `T: 'static`, and its Rust type is its item type: a target `accept::<Card>()`s, and reads
the item back with `item.get::<Card>()`. Two kinds with the same shape get newtypes
(`struct TaskCard(Id)`, `struct ColumnCard(Id)`), which the compiler then keeps apart. Each
type has a name for announcements (`ItemType::name`, from `std::any::type_name` unless the
source gives a label).

### Registration lives in element state

Sources, targets and auto-scroll zones are registered as they're laid out: each keeps an
`Rc` of its record (bounds, callbacks, parent) in its element state, and the window's registry
holds `Weak`s. GPUI drops element state an element didn't use in a frame, so a registration
lives exactly as long as its element is drawn, survives a view drawn from cache, and needs no
clearing pass. The registry is per window; a drag never leaves its window.

### Hit testing is geometric

The mouse could use hitboxes, but the keyboard has no pointer and a touch drag moves GPUI's
`mouse_position` only during a long press. So every backend reports a position in window
coordinates, and the registry finds the targets under it: those whose visible bounds (bounds
cut by their content mask, so a target scrolled out of its list isn't hit) contain the
position. The topmost is the one painted last; its chain of enclosing targets, outermost first,
is the drag's **target stack** (react-dnd's `targetIds`).

### Redrawing

As the pointer moves, the provider's own view is notified, not the whole window: a
`window.refresh()` would draw every cached view anew. Elements styled by the drag
(`dragging_style`, `over_style`) are drawn with the provider's view; a view drawn from cache
(`AnyView::cached`) that shows something about the drag collects it (`collect_dnd`) and is
notified only when that changes.

### A source costs little a frame

Every source is drawn each frame its view is. So a `dnd` is one element round its div (not a
`RenderOnce` and a wrapper), registers nothing when it's neither a source nor a target, and adds
no listeners of its own: the active provider holds one press, long-press and touch-drag listener
for the window and finds the source under the press among the registered hitboxes, the
innermost first. Many sources of one view share one source built once
(`DragSource::keyed(|key, cx| item)….shared()`, then `dnd(id).shared_source(&shared, key)`),
each giving only its key; the item is made from the key when a drag begins. At 500 small
elements a frame (release, ci-runner, `tests/source_cost.rs`), a plain div takes 1.30 µs each,
a `dnd` with a shared source 2.07 µs, one with its own source 2.45 µs.

A drag's own cost is spread thin. Moved onto gpui-dnd (#372), a calendar drag frame took 11 µs
more than calendar's own window listener (1.045 against 1.056 ms on a busy week, about 1%):
the target lookup, the monitor's snapshot, the hover call and the zone check of each move, and
the provider's and target's elements of each frame. Skipping the catch-outside scan when
nothing catches, sharing the target list instead of copying it, and asking `can_drop` only of
targets that have one (#380) measured the same as before, so they weren't made: no single
part is worth more machinery.

### Idle costs nothing

No timer or frame request exists unless a drag is moving, an auto-scroll is scrolling, or a
preview or a sortable gap is animating. A drag held still schedules nothing. A test in every PR
counts renders: none while idle, none while a drag holds still, none after it ends. While the
pointer moves, a frame is drawn for each move (as with GPUI's drag); collected state notifies
only the views whose collected values changed.

## The API

Everything is re-exported at the crate root. Ids are GPUI's `ElementId`, unique in their
window.

### The provider

```rust
/// Wraps the window's content (or a subtree): listens for the drag's moves, draws the drag
/// layer above everything, and holds the live region that announces keyboard drags.
/// react-dnd's <DndProvider backend>. One provider per window is active: the first drawn when
/// none is, staying active while it's drawn (a token in its element state, so a cached view
/// keeps it). It does the work; every other one, nested or beside it, only draws its child. So
/// a crate can wrap its own view, several such views can sit side by side, and a host can wrap
/// the whole window. Drags redraw the active provider's view, so its preview is never replayed
/// stale from a cache, and auto-scroll's containers are stamped by frame and sealed as it
/// paints, so a sibling drawn before it still counts.
pub struct DndProvider { /* … */ }

impl DndProvider {
    pub fn new(child: impl IntoElement) -> Self;
    /// Whether each backend starts drags. All on by default.
    pub fn mouse(self, on: bool) -> Self;
    pub fn touch(self, on: bool) -> Self;
    pub fn keyboard(self, on: bool) -> Self;
    /// A custom drag layer (react-dnd's useDragLayer): called on every move while a drag whose
    /// preview is `Preview::Layer` is on, drawn above everything at no fixed place.
    pub fn layer(
        self,
        layer: impl Fn(&DragLayerMonitor, &mut Window, &mut App) -> Option<AnyElement> + 'static,
    ) -> Self;
    /// What the live region says. Defaults to English sentences built from the labels.
    pub fn announcements(self, announcements: Announcements) -> Self;
}
impl RenderOnce for DndProvider {}

/// How a touch picks a source up (`DragSource::touch_activation`): after a long press (the
/// default: a plain touch still scrolls) or as soon as the finger moves (handles, canvases).
pub enum TouchActivation { LongPress, Immediate }
```

With gpui-core the provider goes **inside** `Root`, as the content of the view Root shows:
`Root::new(cx.new(|_| App::new()), …)`, where `App::render` returns
`DndProvider::new(content)`. The preview is deferred at a higher priority than anything Root
draws, so it's above Root's popovers, dialogs and toasts either way; inside, the provider's view
is the app's, which is what's redrawn as the drag moves.

### Sources and targets: the `dnd` element

One element can be a source, a target, or both (a sortable item is both), like react-dnd's
`drag(drop(ref))`. It's a div: it takes styles and children, and its id is its source and
target id.

```rust
pub fn dnd(id: impl Into<ElementId>) -> Dnd;

pub struct Dnd { /* a Stateful<Div> and the specs */ }
impl Dnd {
    pub fn source<T: 'static>(self, source: DragSource<T>) -> Self;
    pub fn target(self, target: DropTarget) -> Self;
    /// The theme's ring drawn round a target while an item it can take is over it. On.
    pub fn drop_indicator(self, on: bool) -> Self;
    /// Styles while this element's own item is being dragged (react-dnd's collected
    /// isDragging, without a view re-render): typically `|s| s.opacity(0.4)`.
    pub fn dragging_style(self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self;
    /// Styles while an item this target can take is over it (shallow), and while one that it
    /// can take is being dragged anywhere.
    pub fn over_style(self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self;
    pub fn can_drop_style(self, f: impl FnOnce(StyleRefinement) -> StyleRefinement) -> Self;
    /// The accessible name used in announcements ("Card: Write the brief").
    pub fn label(self, label: impl Into<SharedString>) -> Self;
}
impl Styled for Dnd {}
impl ParentElement for Dnd {}
impl InteractiveElement for Dnd {}
impl StatefulInteractiveElement for Dnd {}
impl RenderOnce for Dnd {}

/// A drag handle inside a `dnd` whose source is `handle_only()`: only it starts the drag, and
/// it's the Tab stop for the keyboard. react-dnd's separate drag ref.
pub fn drag_handle(id: impl Into<ElementId>) -> DragHandle;
```

#### Sources (useDrag)

```rust
pub struct DragSource<T: 'static> { /* … */ }

impl<T: 'static> DragSource<T> {
    /// The item, made when the drag begins (react-dnd's `item` as a function).
    pub fn new(item: impl Fn(&mut App) -> T + 'static) -> Self;
    /// Several items of one type dragged together (a selection): what gpui-tables' rows and
    /// gpui-blocks' runs of blocks need. No drag begins when it makes none.
    pub fn many(items: impl Fn(&mut App) -> Vec<T> + 'static) -> Self;
    /// The same item every time.
    pub fn item(item: T) -> Self where T: Clone;
    pub fn can_drag(self, can: impl Fn(&App) -> bool + 'static) -> Self;
    /// Called once the drag has begun (past the threshold, after the long press, or on Space).
    pub fn on_begin(self, f: impl Fn(&DragMonitor, &mut Window, &mut App) + 'static) -> Self;
    /// Called once it's over, dropped or not (react-dnd's `end(item, monitor)`).
    pub fn on_end(self, f: impl Fn(&DragEnd<T>, &mut Window, &mut App) + 'static) -> Self;
    /// Whether this source's element counts as the one being dragged, for a source drawn anew
    /// mid-drag (a sortable item that moved lists): react-dnd's `isDragging(monitor)`. The
    /// default compares source ids.
    pub fn is_dragging(self, f: impl Fn(&[T]) -> bool + 'static) -> Self;
    pub fn preview(self, preview: Preview<T>) -> Self;
    /// Only a `drag_handle` inside starts the drag.
    pub fn handle_only(self) -> Self;
    /// How far the mouse moves before a press becomes a drag. Default 0.25 rem.
    pub fn threshold(self, threshold: Rems) -> Self;
    pub fn touch_activation(self, activation: TouchActivation) -> Self;
}

pub enum Preview<T: 'static> {
    /// A lifted copy: the element built by `build`, or, without one, a card of the source's
    /// size in the theme's card colours with a ring and shadow, and a count badge when several
    /// items are dragged. The grab point stays under the pointer.
    Default(Option<Rc<dyn Fn(&[T], &mut Window, &mut App) -> AnyElement>>),
    /// The provider's `layer` draws it (react-dnd's custom drag layer).
    Layer,
    /// Nothing follows the pointer (the source shows the drag itself, as a sortable gap does).
    None,
}
impl<T> Preview<T> {
    pub fn build(f: impl Fn(&[T], &mut Window, &mut App) -> AnyElement + 'static) -> Self;
}

pub struct DragEnd<'a, T> {
    /// One, unless the source is a `many`; `end.item()` is the first.
    pub items: &'a [T],
    /// Dropped on a target that handled it (react-dnd's didDrop).
    pub dropped: bool,
    /// Where: the target that handled it, and what it returned.
    pub target: Option<&'a ElementId>,
    pub result: Option<&'a DropResult>,
    /// Ended by Escape, a drop on nothing, a lost window or a cancelled touch.
    pub cancelled: bool,
    pub backend: Backend,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend { Mouse, Touch, Keyboard }
```

#### Targets (useDrop)

```rust
pub struct DropTarget { /* … */ }

impl DropTarget {
    pub fn new() -> Self;
    /// An item type this target takes; call once per type.
    pub fn accept<T: 'static>(self) -> Self;
    /// Asked as the pointer moves over it, and as the drag draws (for `can_drop_style`), so it
    /// gets the drag's monitor rather than one for a target the pointer may not be over.
    pub fn can_drop(
        self,
        f: impl Fn(&DragItem, &DragMonitor, &App) -> bool + 'static,
    ) -> Self;
    /// Called on every move while an accepted item is over it, deep or shallow, whether or not
    /// it can drop (react-dnd's hover). Innermost first.
    pub fn on_hover(
        self,
        f: impl Fn(&DragItem, &DropMonitor, &mut Window, &mut App) + 'static,
    ) -> Self;
    /// Called on a drop, innermost target first, for each target in the stack that can drop.
    pub fn on_drop(
        self,
        f: impl Fn(&DragItem, &DropMonitor, &mut Window, &mut App) -> DropOutcome + 'static,
    ) -> Self;
    /// Where a keyboard drag puts its pointer on this target (default: its centre).
    pub fn keyboard_anchor(self, f: impl Fn(Bounds<Pixels>) -> Point<Pixels> + 'static) -> Self;
}

pub enum DropOutcome {
    /// Handled here; the targets outside aren't asked.
    Handled(Option<DropResult>),
    /// Handled here, and the targets outside are asked too: they see `did_drop()` true and
    /// this result (react-dnd's default chaining).
    HandledContinue(Option<DropResult>),
    /// Not handled; the next target out is asked as if this one weren't there.
    Pass,
}

/// The dragged item, its type erased: always a `Vec<T>` inside, one long for a single item.
#[derive(Clone)]
pub struct DragItem { /* Rc<dyn Any>, the count, ItemType, label */ }
impl DragItem {
    /// The item, or the first of several.
    pub fn get<T: 'static>(&self) -> Option<&T>;
    pub fn all<T: 'static>(&self) -> Option<&[T]>;
    pub fn count(&self) -> usize;
    pub fn is<T: 'static>(&self) -> bool;
    pub fn item_type(&self) -> ItemType;
    pub fn label(&self) -> Option<&SharedString>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ItemType { /* TypeId and name */ }
impl ItemType {
    pub fn of<T: 'static>() -> Self;
    pub fn name(&self) -> &'static str;
}

/// What a target hands back to the source, any value.
#[derive(Clone)]
pub struct DropResult(Rc<dyn Any>);
impl DropResult {
    pub fn new<R: 'static>(value: R) -> Self;
    pub fn get<R: 'static>(&self) -> Option<&R>;
}
```

### The monitor and collected state

```rust
/// The drag as it is now (react-dnd's DragSourceMonitor, DropTargetMonitor and
/// DragLayerMonitor in one). Read anywhere with `DragMonitor::of(cx)`. It's a snapshot (an `Rc`
/// of the drag's state, made anew on each change), so it borrows nothing and a callback can keep
/// it while it changes the app.
#[derive(Clone, Default)]
pub struct DragMonitor { /* Option<Rc<Snapshot>> */ }

impl DragMonitor {
    pub fn of(cx: &App) -> DragMonitor;
    pub fn is_dragging(&self) -> bool;
    /// Whether this source's item is the one being dragged.
    pub fn is_dragging_source(&self, source: &ElementId) -> bool;
    pub fn item(&self) -> Option<&DragItem>;
    pub fn item_type(&self) -> Option<ItemType>;
    pub fn source(&self) -> Option<&ElementId>;
    pub fn backend(&self) -> Option<Backend>;
    /// Where the drag began, and where the pointer (or the keyboard's anchor) is, in window
    /// coordinates.
    pub fn initial_client_offset(&self) -> Option<Point<Pixels>>;
    pub fn client_offset(&self) -> Option<Point<Pixels>>;
    pub fn difference_from_initial_offset(&self) -> Option<Point<Pixels>>;
    /// The source's top-left when the drag began, and where it'd be now if it moved with the
    /// pointer: what a custom preview draws at.
    pub fn initial_source_client_offset(&self) -> Option<Point<Pixels>>;
    pub fn source_client_offset(&self) -> Option<Point<Pixels>>;
    pub fn source_size(&self) -> Option<Size<Pixels>>;
    /// The targets under the pointer that accept the item, outermost first.
    pub fn target_ids(&self) -> Vec<ElementId>;
    pub fn is_over(&self, target: &ElementId, depth: Over) -> bool;
    pub fn can_drop_on(&self, target: &ElementId) -> bool;
    /// During a drop: whether a target inside already handled it, and its result.
    /// Ended without a drop, its preview on the way back: still dragging until it lands.
    pub fn is_returning(&self) -> bool;
    pub fn did_drop(&self) -> bool;
    pub fn drop_result(&self) -> Option<&DropResult>;
    pub fn modifiers(&self) -> Modifiers;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Over { Shallow, Deep }

/// A target's view of the monitor, given to its callbacks.
pub struct DropMonitor { /* DragMonitor and the target */ }
impl DropMonitor {
    pub fn target(&self) -> &ElementId;
    pub fn bounds(&self) -> Bounds<Pixels>;
    pub fn is_over(&self, depth: Over) -> bool;
    pub fn can_drop(&self) -> bool;
    pub fn offset_in_target(&self) -> Option<Point<Pixels>>;
}
impl Deref for DropMonitor { type Target = DragMonitor; }

/// The drag layer's view: the monitor, plus where to draw the preview.
pub struct DragLayerMonitor { /* … */ }
impl Deref for DragLayerMonitor { type Target = DragMonitor; }

/// react-dnd's `collect`: a value worked out from the monitor on every change, that notifies
/// its view only when it changes. Dropping it unsubscribes.
pub struct Collected<C> { /* Rc<RefCell<C>> and the subscription */ }
impl<C: Clone> Collected<C> {
    pub fn get(&self) -> C;
}

pub trait CollectExt {
    fn collect_dnd<C: PartialEq + Clone + 'static>(
        &mut self,
        f: impl Fn(&DragMonitor) -> C + 'static,
    ) -> Collected<C>;
}
impl<V: 'static> CollectExt for Context<'_, V> {}
```

So a column that needs `is_over` in its own state (to show a "Drop here" row) takes
`cx.collect_dnd(|m| m.is_over(&"todo".into(), Over::Shallow))` once in its constructor and
reads `.get()` in render; the column re-renders when the answer flips, not on every move.
Styling an element by the drag needs neither: `dragging_style`, `over_style` and
`can_drop_style` are read as the element draws.

### Keyboard

A free-standing source is a Tab stop (its handle, if `handle_only`), described to VoiceOver as
"Press Space to pick up". A `Sortable` is one Tab stop for the whole list, however long (ARIA's
listbox pattern): the arrows move the focus between its items, and Space picks the focused one
up. Then:

| Key | Does |
| --- | --- |
| Space, Enter | pick up the focused source; drop where it is |
| ↑ ↓ ← → | move to the nearest target in that direction that can take the item (innermost first); in a sortable, to the next slot |
| Escape | cancel: the preview goes back and the source keeps its focus |
| Tab, and every other key | nothing while a keyboard drag is on: it holds the keyboard |

Keys reach the drag through `cx.intercept_keystrokes`, before any binding or element. A source
takes no focus from a mouse press (a focus change redraws every view), and a press anywhere ends
a keyboard drag. A source that the keyboard shouldn't pick up says `DragSource::keyboard(false)`.
The live region's text is also readable with `gpui_dnd::announcement(window, cx)`, for a host
that routes announcements itself; mouse and touch drags aren't announced. A target whose own
sentence says everything gives it with `DropTarget::say(|item, monitor, said, cx| …)` (`Said::Over` or `Said::Dropped`),
said word for word in place of "Over …" and "Dropped … on …" (a calendar's "Standup, Tuesday 9
to 9:30 AM, moving to Wednesday 10 AM", in its own formats). The drop's sentence is asked
before `on_drop`, while the target still has the drag.

The keyboard's pointer sits at each target's `keyboard_anchor`, so hover and drop run exactly as
for the mouse, offsets included. Each step is announced in a live region (a `Role::Status`
node in the provider): "Picked up Write the brief. Arrow keys move, Space drops, Escape
cancels.", "Over Doing, position 2 of 4.", "Dropped in Doing, position 2.", "Cancelled. Write
the brief is back in To do." `Announcements` overrides each sentence:

```rust
#[derive(Clone)]
pub struct Announcements {
    pub picked_up: Rc<dyn Fn(&DragItem) -> SharedString>,
    pub over: Rc<dyn Fn(&DragItem, Option<&SharedString>) -> SharedString>,
    pub dropped: Rc<dyn Fn(&DragItem, Option<&SharedString>) -> SharedString>,
    pub cancelled: Rc<dyn Fn(&DragItem) -> SharedString>,
}
impl Default for Announcements {}
```

### Touch

- **Long press** (the default). GPUI's recognizer offers a `LongPressEvent` after half a second
  without moving; the source under it claims it (`window.prevent_default()`), and the drag
  follows `Moved` to `Ended` (drop) or `Cancelled` (cancel). Before then a touch scrolls, so a
  sortable list inside a scroll view still scrolls by touch.
- **At once** (`TouchActivation::Immediate`, for handles). The source claims the
  `TouchDragEvent` GPUI offers on touch-down, and the drag starts when the finger moves past
  GPUI's touch slop.
- The preview, hover and drop follow the event's own position, not `window.mouse_position()`,
  which GPUI leaves where an at-once touch drag started.
- The source under the finger is found with `Hitbox::is_hovered_at(start_position)`, so a
  popover over it still hides it.
- Tests simulate `TouchEvent`s and advance the clock past the long-press delay.

### Previews and the drag layer

The provider draws the preview with `window.defer_draw` at priority 100, above gpui-core's
popovers (1), dialogs (2) and toasts (3). It takes no hitbox, so it never hides the targets
from the pointer. The default preview keeps the grab point under the pointer and is drawn at
the source's size. When a drag is cancelled, or dropped where nothing takes it, the preview
goes back to the source's place in 200 ms (`RETURN_DURATION`), ease-out (at once if
`cx.reduce_motion()` or the preview is `None`), and `on_end` runs when it lands, as a drag on the
Mac ends once its image has slid back. Meanwhile no target is under it, moves and releases are
ignored, and a press or a second Escape lands it at once. A drop on a target ends it at once; a sortable makes the preview
settle into the gap first.

### Auto-scroll

```rust
pub trait AutoScrollExt: IntoElement {
    /// While a drag is on and the pointer is within `edge` of this scroll container's sides,
    /// scroll it, faster the closer (to `speed` at the edge itself), until the pointer leaves
    /// the edge, the end is reached, or the drag ends. Called last on the container.
    fn auto_scroll(self, scroller: &(impl Scrollable + Clone), options: AutoScroll)
        -> AutoScrollArea;
}

#[derive(Clone, Copy, Debug)]
pub struct AutoScroll {
    pub edge: Rems,      // 3 rem
    pub speed: f32,      // 75 rem per second at the edge, eased in quadratically
    pub vertical: bool,
    pub horizontal: bool,
}

/// GPUI's `ScrollHandle` and `UniformListScrollHandle`, or your own (a canvas that pans).
pub trait Scrollable: 'static {
    fn offset(&self) -> Point<Pixels>;
    fn max_offset(&self) -> Point<Pixels>;
    fn set_offset(&self, offset: Point<Pixels>, window: &mut Window, cx: &mut App);
}
```

It runs on animation frames (`window.on_next_frame`), so it keeps scrolling while the pointer
holds still at the edge, and stops asking for frames the moment nothing under the pointer can
scroll that way. After each step it hit-tests again, so targets that scrolled under the pointer
get `on_hover`. Nested containers each scroll on their own axis; the innermost that can still
scroll goes first. Keyboard drags don't auto-scroll (their pointer jumps to targets). The scroll
containers are gathered each frame the provider's view draws, so one inside another view drawn
from cache isn't seen.

### Sortable lists and kanban

```rust
/// A list whose items reorder by dragging, by touch and by keyboard, and, with a `group`,
/// move to other lists of the group. The host owns the order: it hears `on_move` and changes
/// its keys.
#[derive(IntoElement)]
pub struct Sortable<K: Clone + Eq + Hash + 'static> { /* … */ }

impl<K: Clone + Eq + Hash + 'static> Sortable<K> {
    pub fn new(id: impl Into<ElementId>, keys: impl IntoIterator<Item = K>) -> Self;
    pub fn axis(self, axis: Axis) -> Self;                      // Vertical
    pub fn group(self, group: impl Into<SharedString>) -> Self; // lists of a group share items
    pub fn placeholder(self, placeholder: Placeholder) -> Self; // Gap
    /// Draws an item; also its preview (`SortableItem::preview`).
    pub fn item(
        self,
        render: impl Fn(&K, &SortableItem, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self;
    pub fn empty(self, render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self;
    pub fn label(self, label: impl Into<SharedString>) -> Self;   // "To do", for VoiceOver
    pub fn item_label(self, label: impl Fn(&K) -> SharedString + 'static) -> Self;
    pub fn can_move(self, f: impl Fn(&SortableMove<K>, &App) -> bool + 'static) -> Self;
    pub fn on_move(self, f: impl Fn(&SortableMove<K>, &mut Window, &mut App) + 'static) -> Self;
    pub fn gap(self, gap: Rems) -> Self;                        // half a rem
    /// The list scrolls along its axis with this handle, and auto-scrolls during a drag.
    pub fn scroll(self, handle: &ScrollHandle) -> Self;
}
impl Styled for Sortable<K> {}

pub enum Placeholder {
    /// The items part, a gap the dragged item's size opening where it'd land (animated).
    Gap,
    /// A 2 px line in the theme's ring colour where it'd land; nothing moves.
    Line,
    /// The items part, and your element fills the gap.
    Custom(Rc<dyn Fn(Size<Pixels>, &mut Window, &mut App) -> AnyElement>),
}

pub struct SortableItem { pub index: usize, pub dragging: bool, pub focused: bool, pub preview: bool }

#[derive(Clone, Debug, PartialEq)]
pub struct SortableMove<K> {
    pub key: K,
    pub from: (ElementId, usize),
    /// The index in the destination counted without the item: remove, then insert.
    pub to: (ElementId, usize),
}

/// What a sortable item drags; other targets can accept it (a bin).
pub struct SortableDrag<K> { pub list: ElementId, pub key: K, pub from: usize, pub size: Size<Pixels>, /* group */ }
```

- **Measuring.** The items keep their places in the layout while a drag is on, and are drawn
  moved (`window.with_element_offset`, in prepaint). The midpoints the landing place is measured
  against never move under the pointer, and a list that scrolls mid-drag measures afresh each
  frame.
- **Sliding.** `Gap` slides the items apart in 150 ms (`SLIDE_DURATION`), ease-out. The list it
  leaves closes up; the list it would join grows by the item's size. After the drop, the item
  settles from where its preview was let go into its new place. Under reduced motion nothing
  slides.
- **Rules.** Lists without a shared `group` take only their own items. `can_move` is asked
  while hovering, so no gap opens where the move isn't allowed, and again at the drop.
- **Keyboard.** The list is one Tab stop however long (ARIA's listbox pattern: `Role::List`,
  the focused item its active descendant). Before a pick-up, the arrows along the axis (and
  Home and End) move the focus between items, and Space or Enter picks up the focused one. Then
  the arrows move it one place at a time: each gap between items is a keyboard slot of the
  list's target (`DropTarget::keyboard_slots`). Across the axis they move to the nearest place
  in the neighbouring list of the group. The live region says "Over To do, position 2 of 4"
  (`DropTarget::describe`). After a keyboard drop, the focus follows the item to its new list
  and place.
- **Kanban.** A board is a row of `Sortable`s with one `group`, inside an `auto_scroll` row,
  each column `scroll`ing on its own.
- **Long lists: `VirtualSortable`.** `VirtualSortable::new(id, keys: Rc<[K]>, &UniformListScrollHandle)`
  draws its rows with `uniform_list`, so only those on screen are drawn. Every row is the same
  height (GPUI's `last_item_size.contents` over the count), so where each row is, where the
  pointer lands and where the keyboard goes next are worked out from the scroll offset, rows
  drawn or not. Only the rows on screen are moved. A keyboard step goes through
  `DropTarget::keyboard_step`: the list scrolls the next place into view and points there.
  Auto-scroll uses the `uniform_list`'s handle. It takes the same `SortableDrag<K>` as `Sortable`,
  so the two share items within a group. New keys (a new `Rc`) tell it the host moved
  something. At 10,000 rows, a frame of a drag takes 0.14 ms (median, release, ci-runner) and an
  auto-scroll frame 0.08 ms.

## Drags a host starts itself (for gpui-calendar)

Agreed with the lead and the gpui-calendar agent (#362); step 1 is built (see the order at the
end). gpui-calendar draws a whole view on one canvas and decides
itself what a press means (`paint::target_at`): empty time creates, an event moves, an event's
bottom (or an all-day span's right edge) resizes, a "+N" list's row moves its event, and only
the selected event follows a finger at once. So a drag there can't start from a `dnd` element
that's the thing pressed. What calendar does today, and must keep (from a survey of
`calendar.rs`, `paint.rs`, `time.rs` and their tests):

- the press becomes a drag past `4 × scale` px (8 on touch), or at once on a long press;
- the proposal is worked out in display-zone wall minutes from the last frame's `Geo`, snapped
  to `snap_minutes`, across DST (`time::moved`, `resized`, `created`), and between the all-day
  row and the hours (the event changes shape mid-drag);
- calendar draws the ghost in place on its canvas, with the original faded;
- it emits a proposal (`Create`, `Move`, `Resize`) and changes nothing itself;
- a repeating event's drop waits for the "this / following / all" question with the ghost
  left where it was dropped;
- Escape drops the drag (and the selection on a second Escape); a cancelled touch clears it;
- a release outside the window commits;
- the pointer is followed across the whole window (`paint.rs:703-739`) and clamped to the grid:
  a move or a release over the header, the mini month or the sidebar proposes and commits at the
  clamped spot;
- the cursor follows the drag's kind;
- the grid auto-scrolls near its top and bottom, in minutes;
- there's no keyboard move or resize yet.

And these, which the new API must leave alone:
- a press that never passes the threshold reaches calendar unchanged, so clicks and
  double-clicks work as now (`press_at` doesn't stop the press's propagation);
- only the left button starts a drag (calendar calls `press_at` only for it);
- read-only events get no `press_at` or `begin_at`;
- hover cursors before a drag stay calendar's; `DragSource::cursor` applies only while dragging;
- only the selected event's touch is claimed, so panning and period swipes stay calendar's;
- a drag from a "+N" row closes that list (calendar, in `on_begin`).

### What already fits

- **The target.** Calendar's body (one element) is a `dnd` target that accepts its own item
  type. `on_hover` gets the pointer (`client_offset`) and works the proposal out with its `Geo`
  as it does now, into its own state, and notifies itself. Its areas (the grid, the all-day row,
  the month) stay calendar's: the item changes shape between them, so they aren't separate
  targets.
- **The ghost.** `Preview::None`: calendar keeps drawing it on its canvas, and a cancel ends at
  once (no slide back, since nothing is drawn to slide).
- **The drop.** `on_drop` calls calendar's `finish`. For a repeating event it stores its
  question as now and returns `Handled`: gpui-dnd's drag is over, calendar's question and ghost
  carry on (no "pending" drop in gpui-dnd).
- **Cancel.** gpui-dnd takes Escape during a drag and the source's `on_end` hears
  `cancelled`, where calendar clears its drag (its question keys come after the drop, when no
  drag holds the keyboard). A touch cancelled is the same. A release outside the window already
  drops where the pointer last was. With `catch_outside` (below) that drop is calendar's, as
  it commits now.
- **Touch begin.** `on_begin` is where calendar sets up a long press's create (an hour from the
  snapped minute).

### What's missing: five additions

```rust
/// Starts a drag the host chose to start: a press on something gpui-dnd doesn't know is a
/// source (a canvas's event, a list row outside it). It becomes a drag once the pointer moves
/// `threshold` away (the mouse, or a finger whose touch drag the host claimed), as a source's
/// press does. `id` names the drag's source for the monitor (`is_dragging_source`); `grab` is
/// where in the item it was taken (the preview's and `source_client_offset`'s anchor).
pub fn press_at<T: 'static>(
    window: &mut Window,
    cx: &mut App,
    id: impl Into<ElementId>,
    at: Point<Pixels>,
    source: DragSource<T>,
    options: Press,
);

/// The same, begun at once: a long press, or a key.
pub fn begin_at<T: 'static>(
    window: &mut Window,
    cx: &mut App,
    id: impl Into<ElementId>,
    at: Point<Pixels>,
    source: DragSource<T>,
    options: Press,
) -> bool;

pub struct Press {
    pub backend: Backend,          // Mouse, Touch or Keyboard
    pub threshold: Option<Pixels>, // a source's quarter rem unless given
    pub grab: Point<Pixels>,       // from the item's top-left; zero for a create
    pub size: Size<Pixels>,        // the item's, for a preview and the monitor
}

/// Ends the window's drag as Escape would (for a host whose own key, or its question, must
/// win).
pub fn cancel(window: &mut Window, cx: &mut App);

impl<T> DragSource<T> {
    /// The cursor while it's dragged, by what's dragged: calendar's ClosedHand to move,
    /// ResizeUpDown (or LeftRight) to resize, Arrow to create. ClosedHand unless given.
    pub fn cursor(self, f: impl Fn(&T, &DragMonitor) -> CursorStyle + 'static) -> Self;
}

/// A scroll zone the host registers from its own paint, for a band of a canvas (calendar's
/// hours, between `grid_top` and `grid_bottom`) scrolled by something that isn't an element's
/// handle (`Shared.scroll`, in minutes, through `Scrollable`). Only for drags whose item
/// `for_items` takes (calendar's: creating, moving over the hours, resizing in them).
pub fn auto_scroll_zone(
    window: &mut Window,
    cx: &mut App,
    bounds: Bounds<Pixels>,
    scroller: Rc<dyn Scrollable>,
    options: AutoScroll,
);

impl AutoScroll {
    pub fn for_items(self, f: impl Fn(&DragItem) -> bool + 'static) -> Self;
}

impl DropTarget {
    /// This target takes the drag wherever the pointer is in the window, when no other target
    /// under the pointer takes it: `on_hover` keeps hearing the pointer (past its bounds, for
    /// the host to clamp) and a release there calls its `on_drop`, not a cancel. `is_over`
    /// stays geometric, so the host can tell. If two such targets take a drag, the one drawn
    /// last wins; calendar's `can_drop` takes only its own view's items, so two calendars in
    /// a window each keep their own drags.
    pub fn catch_outside(self, on: bool) -> Self;
}
```

- **Why `catch_outside`, not a drop point in `on_end`.** Calendar clamps the ghost while the
  pointer is over the header or sidebar, not only on release. A drop point in `on_end` would
  give it the release but none of the moves before it, so it would need its window-level move
  listeners back, which this proposal removes. With `catch_outside`, the moves and the drop
  take the same path inside and outside the grid (`on_hover`, then `on_drop` → `finish`, with
  the repeating question). A drop on another target that takes the item (a sidebar list, one
  day) still goes there, and a drop on nothing still ends in calendar's `on_drop`, never as a
  cancel.

- `press_at` and `begin_at` reuse the source machinery: the pending press, the threshold, the
  touch slop, `on_begin`, `on_end`, the provider's listeners, the keyboard. There's no element,
  so no record in the registry. The item's spec is held by the drag itself, as it already is
  once a drag starts.
- **No cost a frame.** The source is built at the press, not each frame, so calendar needs no
  shared source (see "A source costs little a frame"). A host already holding a
  `SharedSource<K>` for its elements could pass it and a key instead (`press_shared_at`), if one
  turns up.
- **Touch.** Calendar claims the `TouchDragEvent` for the selected event itself (its predicate)
  and calls `press_at` with `Backend::Touch` and its 8 × scale slop. For a long press it calls
  `begin_at` from its `LongPressEvent`. gpui-dnd then follows the touch as for its own sources.
- **Auto-scroll.** It moves to timed frames, fixing calendar's "only while the pointer moves"
  (README:436). It scrolls by `Scrollable::set_offset`, which calendar maps to minutes.

### What calendar gains, and the keyboard

- **Keyboard move and resize, new.** On the selected event, Space (or a key the host picks)
  calls `begin_at(.., Backend::Keyboard)`, with the item saying move or resize.
  - Calendar's target gives `keyboard_step`. On the grid, ↑ and ↓ are one snap step (the end's,
    for a resize) and ← and → a day; in the month, a day or a week. Each returns the point for
    that time, scrolling the grid to show it, as `VirtualSortable` does.
  - `describe` gives VoiceOver the event, where it was and where it's going, at each step. The
    strings are calendar's: it already returns a `SharedString` from the host's closure, and
    gpui-dnd formats no times. Calendar builds them with its own formatting (12- or 24-hour,
    the display zone, full day names, the day named when it changes): "Standup, Tuesday 9 to
    9:30 AM, moving to Wednesday 10 AM" for a move, "…, ending at 10 AM" for a resize.
    Calendar's tests assert those strings. Space or Enter drops, with a repeating event's
    question after, and Escape cancels.
  - Creating by keyboard stays calendar's Enter and `n`.
- **The rest:** timed auto-scroll, one cursor and listener path shared with the rest of the app's
  drags, and the window-level listeners calendar registers in paint (`paint.rs:703-739`) gone.

### What stays calendar's

All of the geometry and time:
- `Geo`, `geo.pos`, `target_at`, the 44-point touch targets;
- snapping and DST;
- the all-day and hours conversion;
- proposals and edit scopes;
- the recurring question;
- the crowd list (its rows call `press_at` with the event held by its start);
- the agenda (no drags);
- read-only checks (calendar calls neither `press_at` nor `begin_at` for them);
- the double-click and the tap-tap creates.

gpui-dnd only owns the drag's life: when it begins, where the pointer is, the keys and the
touch while it's on, how it ends, the cursor, and timed scrolling.

### Order

1. **Built.** gpui-dnd: `press_at`, `begin_at`, `cancel`, `DragSource::cursor`, `auto_scroll_zone` with
   `for_items`, and `DropTarget::catch_outside`, with tests: a canvas starting drags, a press
   under the threshold reaching the host, touch claimed by the host, a zone without an element,
   and a release outside the target dropping on it (`tests/host.rs`). Building it added:
   - `Press::label`, the item's name for the keyboard's announcements;
   - `AutoScroll::beyond_edges`, since calendar scrolls its hours while the pointer is above
     the grid, over the header, not only near the edge;
   - `drag_cursor(cx)`, the cursor the drag shows, for tests and hosts.

   `AutoScroll` is `Clone` and no longer `Copy`: it holds the `for_items` filter.
2. gpui-calendar, with its owner: the mouse and touch paths onto them, its tests unchanged (the
   interaction and touch drag tests above are the contract), frame times before and after.
3. Keyboard move and resize in gpui-calendar, with announcements. Calendar's sentences are
   said as they are with `DropTarget::say`, added first in its own PR.

## Proposal: gpui-blocks' block drags on gpui-dnd

Not built. Reviewed by the gpui-blocks agent (#393), as #362 was by gpui-calendar's; building
waits for a quiet period in blocks' drag code. From a read of `engine/src/reorder.rs`,
`engine/src/side_drop.rs`, the block-drag half of `engine/src/state.rs` (`BlockDrag`,
`begin_block_drag` to `finish_block_drag`, `touch_pressed`, `touch_moved`, `lift_block`,
`lifted_touch`, `touch_ended`), the touch listeners in `engine/src/element.rs`, and the page
editor's grips, `scroll_near_edge` and `MoveBlockUp`/`Down` in `src/editor.rs`.

### What blocks does today, and must keep

- **Starting a drag.**
  - A press on a block's grip (drawn by the page editor, which calls `begin_block_drag`), or on
    a top-level table's or code block's edge handle (`drag_handle_at`), arms a drag.
  - The drag begins past 4 points.
  - A grip let go without moving opens the block menu, and Shift-click extends the block
    selection.
  - A press on one of a selected run of sibling blocks drags the run (`drag_run`). A drag from
    inside a column is `nested`.
- **Motion** (`reorder.rs`).
  - The dragged blocks follow the pointer exactly, in content y, which scrolling leaves alone.
  - The other top-level blocks spring aside to open the run's whole height. The gap is the one
    past every block whose middle, as drawn at the start, the pointer has passed. Blocks the
    cull didn't draw get estimated middles.
  - On release the blocks land from the pointer's speed, with a little give.
  - A keyboard move or an undo springs both ways without overshoot.
  - Reduced motion jumps, and nothing asks for frames once settled. The motion is springs, not
    timed curves, so a block caught mid-flight carries on.
- **Side drops** (`side_drop.rs`).
  - Once the pointer has gone sideways (or for any drag out of a column), the pointer within
    the side-drop reach `side_drop.rs` computes from a block's left or right edge makes a row of
    two columns.
  - Over a row's edge or the gap between its columns, it adds a column, up to `MAX_COLUMNS`.
  - Inside a column, it drops among the column's blocks. Out of a column, it drops into the
    top level.
  - A vertical or horizontal line shows the drop. Each drop is one undo step, and
    normalization takes away an emptied column and unwraps a one-column row.
- **The drop** is one undoable move (`move_run_to` or `drop_run`). An edit landing mid-drag
  (the revision changed) cancels it, and everything springs back.
- **Touch** (#332).
  - A long press *on text* selects the word, and moving extends the selection, as iOS does.
    This must stay exactly as it is.
  - A long press *off the text* (the gutter, a bullet, a picture, past a line's end; see
    `on_text`) lifts the top-level block under it, or the block in its column, once the
    finger moves 8 points (`TOUCH_LIFT`). It's drawn lifted, a little bigger and higher, and
    drops where the finger lets go. A cancelled touch puts it back.
  - The block sheet's Move lifts a block in place (`lift_block`). The next finger drag
    *anywhere* moves it by as much as the finger moves (`lifted_follow`), and a tap sets it
    down.
- **Edge scrolling.** While blocks are dragged, the page editor requests a frame every frame
  and scrolls a step toward an edge the pointer is near (`scroll_near_edge`), then continues
  the drag at the same pointer (`continue_block_drag`). Blocks is making it stop asking for
  frames when nothing scrolls, and moving `reorder.rs` to GPUI's clock (`web_time` on wasm),
  itself, before any of this.
- **Keys.** `MoveBlockUp` and `MoveBlockDown` move the caret's block, or the selected blocks,
  one place, instantly.
- **Readonly** documents don't drag.

### Who owns what

The engine keeps the document and the drawing; gpui-dnd takes the drag's life, as for calendar.

- **The engine keeps:**
  - what a press means: grips, edge handles, `on_text`, `TOUCH_LIFT`, runs, nested;
  - the side-drop geometry and `drop_run`, `move_run_to` and normalization;
  - the undo steps, the revision check, culling and estimated middles;
  - drawing the lifted blocks and the drop line;
  - the grip's click and Shift-click.
- **gpui-dnd takes:**
  - when the drag begins, the pointer and the finger while it's on, and the release (in the
    window or out of it);
  - Escape (new for blocks) and a lost touch;
  - the cursor while dragging;
  - the edge scrolling, on timed frames that stop when there's nothing left to scroll;
  - the keyboard pick-up (new) and its announcements.

### How it maps

- **The item.** `DraggedBlocks { editor: EntityId, first: BlockId, count: usize, nested: bool }`.
  It carries no document data: the engine's `BlockDrag` stays the state, as calendar's `Drag`
  does.
- **The sources are presses the engine recognises.**
  - A grip's or edge handle's mouse down calls `press_at(.., Press { threshold: 4 points,
    .. })` with a `DragSource::item(DraggedBlocks)`, `Preview::None`, `keyboard(false)`, a
    cursor (closed hand), `on_begin`, which activates the drag (`activate_block_drag`), and
    `on_end`, which clears a cancelled one.
  - A press under the threshold stays the grip's click, and opens the block menu as now.
  - `begin_block_drag(block, at)`, the host API, gains the window and context to do this:
    a breaking change for hosts that draw their own handles.
- **The target.** The editor's content is one `dnd` target with `catch_outside(true)` and a
  `can_drop` that takes only its own editor's blocks.
  - `on_hover` does what `block_drag_to` does now: velocity, `side_target_of`, `drop_gap`.
  - `on_drop` does what `finish_block_drag` does, and returns `Handled`.
  - The columns aren't separate targets: the engine's side-drop geometry (the reach
    `side_drop.rs` computes, the sideways rule, `MAX_COLUMNS`) is finer than nested targets
    would be, and it stays in `side_drop.rs`.
- **`SortableState` the engine drives.**
  - `reorder.rs` moves into gpui-dnd as `SortableState<K: Copy + Eq + Hash>`, generic over the
    key with its tests, as it already nearly is (only `BlockId` ties it to blocks).
  - Its API is the one blocks uses now: `begin_drag(middles)`, `drop_gap(&Dragged)`,
    `frame(slots, dragged, now, reduce_motion) -> (offsets, moving)`, `release(dragged)` and
    `cancel_drag()`. A `Dragged` holds the first key, a count, `dy`, the pointer and its
    velocity.
  - The engine goes on calling it from its prepaint with its own slots and painting the
    offsets, so layout, culling and motion stay together where they are now.
  - gpui-dnd gains a headless, spring-based reorder model. `Sortable` and `VirtualSortable`
    could later take their motion from it (springs that carry a release's velocity instead
    of their timed slide), but that's a separate change.
  - It uses GPUI's clock, `web_time::Instant` on wasm (#383). Today `reorder.rs` uses
    `std::time::Instant`, which doesn't build for the browser.
- **Touch.**
  - The engine's long-press listener stays, with the text-selection path untouched: on text, it
    never involves gpui-dnd.
  - Off the text, once the finger has moved `TOUCH_LIFT`, it calls `begin_at(.., Backend::Touch)`
    at the press, deferred out of the editor's update (gpui-dnd calls back as a drag begins).
    gpui-dnd then follows the long press's moves, and its end drops or its cancel puts back.
  - The engine's own long-press moves give way once a gpui-dnd drag is on.
- **Lifted in place** needs one addition: `Press::relative(true)`. The drag then follows the
  next finger by how far it moves, not where it is: the offset is taken when that finger
  lands, as `lifted_follow` does now. The block sheet's Move calls `begin_at(Touch)` with it,
  and a tap calls `cancel`.
- **Edge scrolling** becomes `.auto_scroll(&scroll, AutoScroll::vertical().for_items(..))` on
  the page's scroll container.
  - That replaces blocks' own edge-scroll canvas, with `block_drag_pointer` and
    `continue_block_drag`.
  - gpui-dnd's tick hovers again after each step, so the gap follows the content scrolled
    under a still pointer, as `continue_block_drag` does now.
- **An edit mid-drag** (the revision changed) calls `gpui_dnd::cancel`, deferred, and the
  blocks spring back as now. Blocks defers edits it makes itself, such as applying an unfurl
  answer, until no block drag is on, so this cancel is only for edits from outside.

### Keyboard block moves

`MoveBlockUp` and `MoveBlockDown` stay as they are: one place, instantly, like a menu's Alt-↑↓.
New is a pick-up, for moving a block or a run further or into columns, with each step said in
blocks' words. Blocks' tests assert these strings.

- **Picking up.**
  - Space while blocks are selected (no new chord: ⌘⇧M is Notion's comment key). The desktop
    block menu also gets a Move item.
  - Either calls `begin_at(.., Backend::Keyboard)` with the selected run, and the label as
    `Press::label`. The label is the kind plus up to 6 words ("Heading Plans for Q3"), or
    "3 blocks".
  - gpui-dnd says "Picked up {label}. Arrow keys move it, Space drops it, Escape cancels.",
    which is its own sentence with blocks' label.
- **Stepping.** The content target's `keyboard_step`:
  - ↑ and ↓ move to the previous or next gap between top-level blocks;
  - → into the neighbouring block's row, as a new column;
  - ← back out of the columns.
  Each step is a `drop_gap` or side-drop target the engine works out. The page scrolls it into
  view, and the step returns the point where the drop line is.
- **Saying it.** `DropTarget::say` (#385) gives blocks' sentences, word for word:
  - a step: "{label}, below {neighbour}, {n} of {m}";
  - into columns: "{label}, in a new column beside {neighbour}";
  - out of columns: "{label}, out of the columns, below {neighbour}";
  - the drop: "{label}, moved below {neighbour}", or "{label}, moved to the top" at index 0;
  - a cancel: "{label}, put back". This needs `Said::Cancelled`, below.
- **Dropping.** Space or Enter drops through the same `on_drop` as the pointer, as one undo
  step. Escape cancels, and the blocks spring back.

### What blocks gains

- Escape during a pointer, touch or keyboard drag, with the blocks springing back. The editor
  binds Escape to `SelectBlock`. During a drag, gpui-dnd takes Escape first (its keystroke
  interceptor stops it), so it cancels the drag and doesn't also select the block. A test
  checks that Escape mid-drag leaves the selection as it was.
- A release outside the window drops where the pointer last was.
- Edge scrolling that asks for no frames while nothing scrolls.
- The cursor while dragging.
- Keyboard pick-up with announcements.
- One drag at a time across the app: a block drag and a table's card drag can't run together.
- Two editors in a window keep their own drags.

### What gpui-dnd adds

1. `SortableState<K>`, from `reorder.rs`, with its tests.
2. `Press::relative`, for a lifted-in-place block, with a test of a finger landing elsewhere.
3. `Said::Cancelled`: on a keyboard drag's cancel, the target it was last over (here the
   editor, which catches the drag outside) is asked for its own sentence ("{label}, put back"),
   said in place of the provider's "Cancelled. … is back where it was.".

Nothing else: the rest is `press_at`, `begin_at`, `cancel`, `catch_outside`, `auto_scroll`,
`keyboard_step` and `say`, as calendar uses them.

### Order, once agreed and once blocks' drag code is quiet

Agreed with the gpui-blocks agent (#393). Building starts after blocks' bookmarks and Page
Up/Down PRs merge, when the lead says.


1. gpui-dnd: `SortableState`, `Press::relative` and `Said::Cancelled`, with tests. Blocks keeps
   its own `reorder.rs` until step 2.
2. gpui-blocks, reviewed by the blocks agent:
   - grips, edge handles, touch lift and lift-in-place onto gpui-dnd;
   - the content as target, and edge scrolling as `auto_scroll`;
   - `reorder.rs` swapped for `SortableState<BlockId>`.
   Its drag, touch and columns tests stay unchanged, plus one that Escape mid-drag leaves the
   selection as it was, with frame times before and after, interleaved on the quiet Mac.
3. Keyboard pick-up with blocks' sentences.

## Later: gpui-flow's canvas

Not proposed for building yet. gpui-flow's node moves, connections, resizing, panning, lasso and
ink are gestures on a zoomed canvas, not drag and drop, and stay its own. So do its tree
reparenting (a node drag dropped on another node, `drop_target` and `reparent` in flow
coordinates), frames adopting the nodes they enclose, and files dropped from the system (GPUI's
`ExternalPaths`: the OS's drags, not gpui-dnd's). What gpui-dnd would add:

- **Palette drops.** A host's palette lists node kinds as gpui-dnd sources: a shared source
  keyed by the kind, carrying a public `FlowPaletteItem` (a node template).
  - **The target:** the flow pane accepts that item. Its `on_hover` maps `client_offset`
    through `screen_to_flow`, and the canvas draws the ghost at the canvas's zoom and snap
    (`Preview::None`). Its `on_drop` emits a `FlowEvent::NodeDropped { item, position }` for
    the host to add, as `FilesDropped` does.
  - **Auto-pan:** `auto_scroll_zone` over the pane, with a `Scrollable` that pans the viewport.
  - **The keyboard:** the pane's `keyboard_step` moves the drop point a grid step, with
    flow's own sentences.
  - Nothing new is needed in gpui-dnd.
- **Reparenting by keyboard,** later: picking a node up with `begin_at(Keyboard)`, the arrows
  stepping to the next node it could go under (tree) or the frame it could go into, said in
  flow's words. The pointer path stays flow's.
- **Dragging nodes out to other views** (onto a doc page) would hand the canvas's own drag to
  gpui-dnd with `begin_at` when the pointer leaves the pane. It waits for a host that needs it.

## react-dnd → gpui-dnd

| react-dnd | gpui-dnd |
| --- | --- |
| `<DndProvider backend={HTML5Backend}>` | `DndProvider::new(content)`; `.mouse(on)`, `.touch(options)`, `.keyboard(on)` |
| `HTML5Backend`, `TouchBackend` | `Backend::Mouse`, `Backend::Touch` (long press or immediate) |
| — | `Backend::Keyboard` (Space, arrows, Escape, announcements) |
| `useDrag({ type, item, canDrag, end, isDragging, collect })` | `dnd(id).source(DragSource::new(item).can_drag(…).on_end(…).is_dragging(…))` |
| item type (string or symbol) | the item's Rust type: `ItemType::of::<T>()` |
| `useDrop({ accept, canDrop, hover, drop, collect })` | `dnd(id).target(DropTarget::new().accept::<T>().can_drop(…).on_hover(…).on_drop(…))` |
| `drag(drop(ref))` | `.source(…).target(…)` on one `dnd` |
| drag handle ref | `DragSource::handle_only()` + `drag_handle(id)` |
| `drop` returning a result | `DropOutcome::Handled(Some(DropResult::new(…)))` |
| outer target checking `monitor.didDrop()` | `DropOutcome::HandledContinue` / `Pass`; `monitor.did_drop()` |
| `isOver({ shallow: true })` | `is_over(id, Over::Shallow)` |
| `monitor.getItem()`, `getItemType()` | `item()`, `item_type()` |
| — | `DragSource::many(…)`, `item.all::<T>()`, `item.count()` (a selection) |
| `getInitialClientOffset()`, `getClientOffset()`, `getDifferenceFromInitialOffset()`, `getSourceClientOffset()` | `initial_client_offset()`, `client_offset()`, `difference_from_initial_offset()`, `source_client_offset()` |
| `collect` → props | `cx.collect_dnd(…)` → `Collected<C>`; or `dragging_style` / `over_style` |
| `useDragLayer` | `DndProvider::layer(…)` with `Preview::Layer` |
| `connectDragPreview(getEmptyImage())` | `Preview::None` |
| `endDrag` with `didDrop()` | `on_end(&DragEnd { dropped, target, result, cancelled, .. })` |
| react-dnd-scrolling, react-sortable-hoc, dnd-kit sortable | `auto_scroll`, `Sortable` |

## Pull requests

1. This document.
2. The crate, the provider, sources, targets, the monitor, `Collected`, and the mouse backend
   with the default preview; the workspace member and `.github/workflows/gpui-dnd.yml`.
3. The drag layer, `Preview::Layer` and `None`, and the animated return.
4. The keyboard backend and the announcements.
5. Touch.
6. `Sortable`, auto-scroll, kanban.
7. Examples (sortable, kanban, nested targets, custom preview), offscreen shots in `docs/`, and
   the README with install steps and the table above.

Every PR has headless tests (simulated mouse, keys, `TouchEvent`s with the clock advanced) and
an idle test that counts renders.

## Moving the workspace's crates onto it later

Not part of this work; what each crate would need, from a survey of its drag code.

**gpui-tables** (the first to move, the closest fit). It drags board cards within and across
columns, list rows across groups, table rows by their handle with the selection carried along,
and view tabs, with GPUI's `on_drag`/`on_drop`; touch picks up on a long press and draws its
own finger ghost; Escape is `cx.stop_active_drag` with no return animation; auto-scroll runs
one step per move.
- Needs: `Sortable` over `uniform_list` (virtualized, with a `Scrollable` for
  `UniformListScrollHandle`), items of different heights in a board column, multi-item payloads
  (the selected rows), `can_move` for sort and group rules, the sorted-view note while dragging,
  and keyboard moves across columns (it has only ⌘⇧↑↓ in the table).
- Goes away: the separate mouse and finger paths, the "stale active_drag" check in every render,
  the hand-drawn touch ghost, per-move scrolling.
- Column header reorder and resize stay its own (`Drag::Header`/`Resize`); header reorder could
  become a horizontal `Sortable` later.

**gpui-core** (PopupMenu's reorder groups, #293). Hand-rolled: an overlay sees the moves, a
lifted row and a drop line, long press on touch, Alt-↑↓, Escape cancels, the click after a drop
is swallowed, and nothing redraws while held still.
- Needs: `Sortable` with `Placeholder::Line` inside a popover (the drag layer above the
  popover), a reorder confined to a group run within a longer list, no submenu opening mid-drag
  (a `DragMonitor::is_dragging` check), the click swallowed after a drop (the crate does this for
  its sources), and Alt-↑↓ as a one-step keyboard move without a pick-up (`Sortable` should
  offer `move_keys`).
- A dependency question: gpui-dnd depends on gpui-core for the theme, so gpui-core can't use
  gpui-dnd. Either the menu keeps its own reorder, or the theme moves to a crate both depend on.
  Keeping the menu's own is the cheap answer.

**gpui-blocks** (block drags): see "Proposal: gpui-blocks' block drags on gpui-dnd".

**gpui-calendar** (moving and resizing events). Hand-rolled on a painted grid: positions are
(date, minute, area) snapped to 15 minutes; the ghost is drawn at the proposal; the host gets a
proposal and decides; drags also create events from empty time and resize their ends; long press
on touch, and an immediate touch drag on the selected event; Escape cancels; per-move
auto-scroll.
- Needs: targets that aren't elements, a **target region** registered from paint with its own
  hit test returning a typed position (`DropTarget::region(bounds, |p| -> Option<Slot>)`), drags
  started by the host itself (`Dnd::start(window, cx, item, at)`) for creating and resizing, and
  the drop's result left to the host (`DropOutcome::Pass` until it confirms). The grid's
  auto-scroll is a `Scrollable`.
- Gains: keyboard move and resize of events (it has none), the return animation, timed
  auto-scroll.
- Moving events with gpui-dnd is reasonable; creation and resize may stay its own.

**gpui-flow**: see "Later: gpui-flow's canvas".
