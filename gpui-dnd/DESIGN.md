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
/// react-dnd's <DndProvider backend>. The outermost one in a window does the work; a nested
/// one only draws its child, so a crate can wrap its own view and still work in a host that
/// wraps the whole window.
pub struct DndProvider { /* … */ }

impl DndProvider {
    pub fn new(child: impl IntoElement) -> Self;
    /// Whether each backend starts drags. All on by default.
    pub fn mouse(self, on: bool) -> Self;
    pub fn touch(self, touch: Option<TouchOptions>) -> Self;
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

#[derive(Clone, Debug)]
pub struct TouchOptions {
    /// How a touch picks up a source that doesn't say: a long press (the default: a plain
    /// touch still scrolls) or at once (handles, canvases).
    pub activation: TouchActivation,
}
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
| Tab | nothing while a drag is on (the drag holds the keyboard) |

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
- The preview, hover and drop follow the event's own position, not `window.mouse_position()`.
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
pub trait AutoScrollExt: StatefulInteractiveElement {
    /// While a drag is on and the pointer is within `edge` of this scroll container's sides,
    /// scroll it, faster the closer (to `speed` at the edge itself), until the pointer leaves
    /// the edge, the end is reached, or the drag ends.
    fn auto_scroll(self, handle: &ScrollHandle, options: AutoScroll) -> Self;
}

#[derive(Clone, Debug)]
pub struct AutoScroll {
    pub edge: Rems,     // 3 rem
    pub speed: f32,     // 1200 px per second at the edge, scaled by the rem
    pub axis: AxisSet,  // Vertical, Horizontal, Both
}

/// For something that isn't a ScrollHandle (a uniform_list's handle, a canvas that pans).
pub trait Scrollable: 'static {
    fn offset(&self) -> Point<Pixels>;
    fn max_offset(&self) -> Size<Pixels>;
    fn set_offset(&self, offset: Point<Pixels>, window: &mut Window, cx: &mut App);
}
```

It runs on animation frames, so it keeps scrolling while the pointer holds still at the edge,
and stops asking for frames the moment it can't scroll. After each step it hit-tests again, so
targets that scrolled under the pointer get `on_hover`. Nested containers (a board's
horizontal scroll and each column's vertical one) each scroll on their own axis; the innermost
that can scroll goes first.

### Sortable lists and kanban

```rust
/// A list whose items reorder by dragging, by touch and by keyboard, and, with a `group`,
/// move to other lists of the group. The host owns the order: on a move it hears
/// `on_move` and changes its data.
pub struct Sortable<K: Clone + Eq + Hash + 'static> { /* … */ }

impl<K: Clone + Eq + Hash + 'static> Sortable<K> {
    pub fn new(id: impl Into<ElementId>, keys: impl IntoIterator<Item = K>) -> Self;
    pub fn axis(self, axis: Axis) -> Self;                    // Vertical
    /// Lists with the same group take each other's items.
    pub fn group(self, group: impl Into<SharedString>) -> Self;
    pub fn placeholder(self, placeholder: Placeholder) -> Self; // Gap
    pub fn item(
        self,
        render: impl Fn(&K, &SortableItem, &mut Window, &mut App) -> AnyElement + 'static,
    ) -> Self;
    /// Shown when the list has no items (still a target).
    pub fn empty(self, render: impl Fn(&mut Window, &mut App) -> AnyElement + 'static) -> Self;
    pub fn can_move(self, f: impl Fn(&SortableMove<K>, &App) -> bool + 'static) -> Self;
    pub fn on_move(self, f: impl Fn(&SortableMove<K>, &mut Window, &mut App) + 'static) -> Self;
    pub fn auto_scroll(self, handle: &ScrollHandle) -> Self;
    pub fn gap(self, gap: Rems) -> Self;
}
impl Styled for Sortable<K> {}
impl RenderOnce for Sortable<K> {}

pub enum Placeholder {
    /// The items part to leave a gap of the dragged item's size where it'd land (animated).
    Gap,
    /// A 2 px line in the theme's primary colour where it'd land; nothing moves.
    Line,
    /// Your element in the gap.
    Custom(Rc<dyn Fn(Size<Pixels>, &mut Window, &mut App) -> AnyElement>),
}

pub struct SortableItem {
    pub index: usize,
    pub dragging: bool,  // this item is the one held
    pub focused: bool,
    pub lifted: bool,    // picked up by the keyboard
}

#[derive(Clone, Debug, PartialEq)]
pub struct SortableMove<K> {
    pub key: K,
    pub from: (ElementId, usize),
    /// The index in the destination as it'd be with the item already removed.
    pub to: (ElementId, usize),
}
```

- The insertion index is measured against item midpoints in the list's content coordinates,
  so it stays right while the list scrolls under a held drag.
- `Gap` slides the items apart in 150 ms (FLIP: each item keeps its old place and moves to the
  new one) and the dropped item settles into the gap; nothing animates under reduced motion.
- The list is one Tab stop; before a pick-up the arrows move the focus between items.
- The keyboard moves one slot per arrow along the axis, and across the axis to the
  neighbouring list of the group (the nearest one in that direction), at the nearest index.
- A kanban board is a row of `Sortable`s with one `group`, inside an `auto_scroll` row, each
  column itself `auto_scroll`ing.

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

**gpui-blocks** (block reorder). Hand-rolled: a block (or a selected run) follows the pointer as
a shadowed copy while the others spring apart, the gap measured against midpoints in content
coordinates snapshotted at the start (culled blocks get estimated ones), one undo step on
release, ⌘⇧↑↓ by keyboard, no touch reorder, no Escape, no auto-scroll.
- Needs: `Sortable` driven by its own layout (blocks are laid out by the engine's element, not
  as children): a lower-level `SortableState` that takes item extents and hands back the gap
  and the slide offsets, so the engine keeps painting; multi-item drags; spring motion with the
  release velocity as an option of the slide animation; a "settled" signal.
- Gains: Escape with the return animation, touch reorder by long press, auto-scroll, keyboard
  pick-up with announcements.
- Hard part: motion, layout and culling are intertwined in the engine's prepaint.

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

**gpui-flow.** Node moves, connections, resize, pan, lasso and ink are gestures on a zoomed
canvas, not drag-and-drop: they stay its own. What maps: dropping a node onto another to
reparent it in a tree layout, dropping nodes into a frame, and dragging an item from outside
the canvas (a palette) onto it.
- Needs: target regions in flow coordinates (a transform from window to canvas space on the
  target), a `Scrollable` that pans the viewport for auto-pan at the canvas edge, and modifiers
  on the monitor.
- Its touch claims (`TouchDragEvent` per node) and Escape put-back are its own and stay.
