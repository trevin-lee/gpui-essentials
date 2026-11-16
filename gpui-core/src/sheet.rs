//! A bottom sheet: a panel across the bottom of the window over a scrim, for touch. It has the
//! same API as [`Popover`](crate::Popover) without the anchor: the host owns whether it's open,
//! and closes it from [`Sheet::on_dismiss`], which runs on a press on the scrim and on Escape.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Press, Sheet};
//! # struct Panel { open: bool }
//! # impl Render for Panel {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     div().size_full().child(
//!         Sheet::new("choices")
//!             .open(self.open)
//!             .on_dismiss(cx.listener(|this, _, _, cx| {
//!                 this.open = false;
//!                 cx.notify();
//!             }))
//!             .child(Press::new("one").min_h(px(44.)).child("One")),
//!     )
//! }
//! # }
//! ```

use gpui::{
    AnyElement, App, Bounds, ContentMask, Element, ElementId, FocusHandle, GlobalElementId,
    InspectorElementId, InteractiveElement as _, IntoElement, LayoutId, MouseButton, ParentElement,
    Pixels, Point, Position, Rems, RenderOnce, Role, Stateful, StatefulInteractiveElement as _,
    Style, StyleRefinement, Styled, Window, div, hsla, relative, rems,
};

use crate::popover::{
    CONTEXT, Dismiss, DismissFn, DismissReason, PRIORITY, PopoverColors, overlay_focus,
};
use crate::theme::ActiveTheme as _;

/// A panel across the bottom of the window, over a scrim that dims the rest; at most as wide as
/// [`max_width`](Sheet::max_width) (centred on a wide window) and as tall as nine tenths of the
/// window, its content scrolling past that. It's drawn only while open.
///
/// While open it has the keyboard (unless [`Sheet::take_focus`] is off), and when it closes the
/// focus goes back to what had it, or to [`Sheet::return_focus`]. For that, keep drawing the
/// sheet while it's closed: closed, it draws nothing.
///
/// Its rows are the host's, at 44 points for touch ([`ControlSize::Touch`](crate::ControlSize)).
#[derive(IntoElement)]
pub struct Sheet {
    id: ElementId,
    open: bool,
    on_dismiss: Option<DismissFn>,
    take_focus: bool,
    return_focus: Option<FocusHandle>,
    max_width: Rems,
    colors: Option<PopoverColors>,
    label: Option<gpui::SharedString>,
    style: StyleRefinement,
    children: Vec<AnyElement>,
}

impl Sheet {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            open: false,
            on_dismiss: None,
            take_focus: true,
            return_focus: None,
            max_width: rems(40.),
            colors: None,
            label: None,
            style: StyleRefinement::default(),
            children: Vec::new(),
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// Runs on a press on the scrim, and on Escape. The host closes it here.
    pub fn on_dismiss(
        mut self,
        f: impl Fn(&DismissReason, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(std::rc::Rc::new(f));
        self
    }

    /// Whether it takes the keyboard while open (on by default). Off, the focus stays where it
    /// was, and Escape is the host's to handle.
    pub fn take_focus(mut self, take: bool) -> Self {
        self.take_focus = take;
        self
    }

    /// Where the focus goes when it closes, instead of what had it when it opened.
    pub fn return_focus(mut self, handle: &FocusHandle) -> Self {
        self.return_focus = Some(handle.clone());
        self
    }

    /// The widest it draws, centred on a window wider than that (40 rem by default).
    pub fn max_width(mut self, width: Rems) -> Self {
        self.max_width = width;
        self
    }

    pub fn colors(mut self, colors: PopoverColors) -> Self {
        self.colors = Some(colors);
        self
    }

    pub fn accessibility_label(mut self, label: impl Into<gpui::SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl ParentElement for Sheet {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

/// The content's own style: padding, gap, and so on.
impl Styled for Sheet {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Sheet {
    fn render(mut self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        crate::init_soon(cx);
        let focus = overlay_focus(
            &self.id,
            self.open,
            self.take_focus,
            self.return_focus.clone(),
            window,
            cx,
        );
        SheetElement {
            scrim: self.open.then(|| self.scrim(&focus, cx)),
        }
    }
}

impl Sheet {
    fn scrim(&mut self, focus: &FocusHandle, cx: &App) -> Stateful<gpui::Div> {
        let theme = cx.theme();
        let colors = self
            .colors
            .unwrap_or_else(|| PopoverColors::from_theme(theme));
        let on_scrim = self.on_dismiss.clone();
        let on_escape = self.on_dismiss.clone();
        let mut content = div()
            .id("sheet-content")
            .flex()
            .flex_col()
            .min_h_0()
            .min_w_0()
            .overflow_y_scroll()
            .px(rems(12. / 15.))
            .pb(rems(20. / 15.));
        gpui::Refineable::refine(content.style(), &self.style);
        let content = content.children(std::mem::take(&mut self.children));
        let mut panel = div()
            .id("sheet")
            .role(Role::Dialog)
            .key_context(CONTEXT)
            .track_focus(focus)
            // Over the scrim, so a press inside isn't one on it.
            .occlude()
            .flex()
            .flex_col()
            .w_full()
            .max_w(self.max_width)
            .max_h(relative(0.9))
            .overflow_hidden()
            .bg(colors.background)
            .text_color(colors.foreground)
            .border_t_1()
            .border_color(colors.border)
            .rounded_t(theme.radius_xl())
            .shadow_lg()
            .debug_selector(|| "gpui-core-sheet".into())
            .on_action(move |_: &Dismiss, window, cx| {
                if let Some(f) = &on_escape {
                    f(&DismissReason::Escape, window, cx);
                }
            })
            .child(
                // The grabber.
                div()
                    .flex_none()
                    .self_center()
                    .w(rems(36. / 15.))
                    .h(rems(4. / 15.))
                    .mt(rems(8. / 15.))
                    .mb(rems(8. / 15.))
                    .rounded_full()
                    .bg(theme.muted_foreground.opacity(0.5)),
            )
            .child(content);
        if let Some(label) = self.label.clone() {
            panel = panel.aria_label(label);
        }
        div()
            .id(self.id.clone())
            .occlude()
            .flex()
            .flex_col()
            .justify_end()
            .items_center()
            .bg(hsla(0., 0., 0., 0.45))
            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                if let Some(f) = &on_scrim {
                    f(&DismissReason::ClickOutside, window, cx);
                }
            })
            .child(panel)
    }
}

/// Takes no room where it's declared, and draws the open sheet over the whole window.
struct SheetElement {
    scrim: Option<Stateful<gpui::Div>>,
}

impl IntoElement for SheetElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for SheetElement {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let style = Style {
            position: Position::Absolute,
            ..Style::default()
        };
        (window.request_layout(style, None, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(scrim) = self.scrim.take() else {
            return;
        };
        let view = window.viewport_size();
        let mut scrim = scrim.w(view.width).h(view.height).into_any_element();
        scrim.layout_as_root(view.map(gpui::AvailableSpace::Definite), window, cx);
        window.defer_draw(
            scrim,
            Point::default(),
            PRIORITY,
            None::<ContentMask<Pixels>>,
        );
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        _: &mut Window,
        _: &mut App,
    ) {
    }
}
