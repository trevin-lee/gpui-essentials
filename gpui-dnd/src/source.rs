//! Drag sources: react-dnd's `useDrag`.

use std::rc::Rc;

use gpui::{AnyElement, App, ElementId, Rems, SharedString, Window};

use crate::item::{DragItem, DropResult};
use crate::monitor::DragMonitor;

type MakeItems<T> = Rc<dyn Fn(&mut App) -> Vec<T>>;
type CanDrag = Rc<dyn Fn(&App) -> bool>;
type OnBegin = Rc<dyn Fn(&DragMonitor, &mut Window, &mut App)>;
type OnEnd<T> = Rc<dyn Fn(&DragEnd<T>, &mut Window, &mut App)>;
type IsDragging<T> = Rc<dyn Fn(&[T]) -> bool>;
type BuildTyped<T> = Rc<dyn Fn(&[T], &mut Window, &mut App) -> AnyElement>;
pub(crate) type MakeItem = Rc<dyn Fn(&mut App) -> Option<DragItem>>;
pub(crate) type OnEndErased = Rc<dyn Fn(&DragItem, &EndInfo, &mut Window, &mut App)>;
pub(crate) type IsDraggingErased = Rc<dyn Fn(&DragItem) -> bool>;

/// What started a drag.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Mouse,
    Touch,
    Keyboard,
}

/// How a drag ended, as the source hears it in [`DragSource::on_end`].
#[derive(Debug)]
pub struct DragEnd<'a, T> {
    /// Every item dragged; one unless the source is a [`DragSource::many`].
    pub items: &'a [T],
    /// A target took it (react-dnd's `didDrop`).
    pub dropped: bool,
    /// The target that took it: the outermost one that handled it.
    pub target: Option<&'a ElementId>,
    /// What the targets that took it returned; the outermost one's wins.
    pub result: Option<&'a DropResult>,
    /// Escape, a drop where nothing takes it, a lost window or a cancelled touch.
    pub cancelled: bool,
    pub backend: Backend,
}

impl<T> DragEnd<'_, T> {
    /// The item, or the first of several.
    pub fn item(&self) -> &T {
        &self.items[0]
    }
}

/// What follows the pointer while this source's item is dragged.
pub enum Preview<T: 'static> {
    /// A lifted copy, the grab point staying under the pointer: the element `build` makes, or
    /// without one a card of the source's size in the theme's card colours, with a ring, a
    /// shadow, and the count when several items are dragged.
    Default(Option<BuildTyped<T>>),
    /// The provider's [`layer`](crate::DndProvider::layer) draws it (react-dnd's custom drag
    /// layer).
    Layer,
    /// Nothing: the source shows the drag itself.
    None,
}

impl<T: 'static> Preview<T> {
    /// The default preview drawn by `build`.
    pub fn build(build: impl Fn(&[T], &mut Window, &mut App) -> AnyElement + 'static) -> Self {
        Self::Default(Some(Rc::new(build)))
    }
}

impl<T: 'static> Default for Preview<T> {
    fn default() -> Self {
        Self::Default(None)
    }
}

/// A drag source (react-dnd's `useDrag`): what a [`dnd`](crate::dnd) element hands over when
/// it's dragged, and what it hears back.
pub struct DragSource<T: 'static> {
    make: MakeItems<T>,
    can_drag: Option<CanDrag>,
    on_begin: Option<OnBegin>,
    on_end: Option<OnEnd<T>>,
    is_dragging: Option<IsDragging<T>>,
    preview: Preview<T>,
    threshold: Option<Rems>,
}

impl<T: 'static> DragSource<T> {
    /// The item, made when the drag begins (react-dnd's `item` as a function).
    pub fn new(item: impl Fn(&mut App) -> T + 'static) -> Self {
        Self::many(move |cx| vec![item(cx)])
    }

    /// Several items dragged together (a selection), made when the drag begins. No drag
    /// begins when it makes none.
    pub fn many(items: impl Fn(&mut App) -> Vec<T> + 'static) -> Self {
        Self {
            make: Rc::new(items),
            can_drag: None,
            on_begin: None,
            on_end: None,
            is_dragging: None,
            preview: Preview::default(),
            threshold: None,
        }
    }

    /// The same item every time.
    pub fn item(item: T) -> Self
    where
        T: Clone,
    {
        Self::new(move |_| item.clone())
    }

    /// Whether a drag may begin now. Asked when it would.
    pub fn can_drag(mut self, can: impl Fn(&App) -> bool + 'static) -> Self {
        self.can_drag = Some(Rc::new(can));
        self
    }

    /// Called once the drag has begun.
    pub fn on_begin(mut self, f: impl Fn(&DragMonitor, &mut Window, &mut App) + 'static) -> Self {
        self.on_begin = Some(Rc::new(f));
        self
    }

    /// Called once the drag is over, dropped or not (react-dnd's `end`).
    pub fn on_end(mut self, f: impl Fn(&DragEnd<T>, &mut Window, &mut App) + 'static) -> Self {
        self.on_end = Some(Rc::new(f));
        self
    }

    /// Whether this element counts as the one being dragged, for an element drawn anew while
    /// the drag is on (an item that moved to another list): react-dnd's `isDragging`. By
    /// default, the element the drag started from.
    pub fn is_dragging(mut self, f: impl Fn(&[T]) -> bool + 'static) -> Self {
        self.is_dragging = Some(Rc::new(f));
        self
    }

    pub fn preview(mut self, preview: Preview<T>) -> Self {
        self.preview = preview;
        self
    }

    /// How far the mouse moves while pressed before the press becomes a drag. A quarter of a
    /// rem unless given, so it grows with the zoom.
    pub fn threshold(mut self, threshold: Rems) -> Self {
        self.threshold = Some(threshold);
        self
    }

    pub(crate) fn erase(self, label: Option<SharedString>) -> SourceSpec {
        let make = self.make;
        let make_label = label.clone();
        let on_end = self.on_end.map(|f| {
            Rc::new(
                move |item: &DragItem, end: &EndInfo, window: &mut Window, cx: &mut App| {
                    if let Some(items) = item.all::<T>().filter(|items| !items.is_empty()) {
                        f(
                            &DragEnd {
                                items,
                                dropped: end.dropped,
                                target: end.target.as_ref(),
                                result: end.result.as_ref(),
                                cancelled: end.cancelled,
                                backend: end.backend,
                            },
                            window,
                            cx,
                        )
                    }
                },
            ) as OnEndErased
        });
        let is_dragging = self.is_dragging.map(|f| {
            Rc::new(move |item: &DragItem| item.all::<T>().is_some_and(|items| f(items)))
                as IsDraggingErased
        });
        let preview = match self.preview {
            Preview::Default(Some(build)) => PreviewSpec::Default(Some(Rc::new(
                move |item: &DragItem, window: &mut Window, cx: &mut App| match item.all::<T>() {
                    Some(items) => build(items, window, cx),
                    None => gpui::Empty.into_any_element(),
                },
            ))),
            Preview::Default(None) => PreviewSpec::Default(None),
            Preview::Layer => PreviewSpec::Layer,
            Preview::None => PreviewSpec::None,
        };
        SourceSpec {
            make: Rc::new(move |cx| {
                let items = make(cx);
                (!items.is_empty()).then(|| DragItem::new(items, make_label.clone()))
            }),
            can_drag: self.can_drag,
            on_begin: self.on_begin,
            on_end,
            is_dragging,
            preview,
            threshold: self.threshold,
        }
    }
}

use gpui::IntoElement as _;

/// How a drag ended, before it's typed for the source.
pub(crate) struct EndInfo {
    pub dropped: bool,
    pub target: Option<ElementId>,
    pub result: Option<DropResult>,
    pub cancelled: bool,
    pub backend: Backend,
}

pub(crate) type BuildPreview = Rc<dyn Fn(&DragItem, &mut Window, &mut App) -> AnyElement>;

pub(crate) enum PreviewSpec {
    Default(Option<BuildPreview>),
    Layer,
    None,
}

/// A source with its item type erased, as the registry keeps it.
pub(crate) struct SourceSpec {
    pub make: MakeItem,
    pub can_drag: Option<CanDrag>,
    pub on_begin: Option<OnBegin>,
    pub on_end: Option<OnEndErased>,
    pub is_dragging: Option<IsDraggingErased>,
    pub preview: PreviewSpec,
    pub threshold: Option<Rems>,
}
