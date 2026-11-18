//! An input group, as shadcn draws it (its `input-group.tsx`): an [`Input`] with addons inside
//! its border, before or after the text: an icon (a search glass), text (`https://`, `.com`)
//! or the host's own element (a button). The border, fill and focus ring are the whole group's.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{IconName, InputGroup, InputState};
//! # struct Form { site: Entity<InputState> }
//! # impl Render for Form {
//! fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
//!     InputGroup::new(&self.site)
//!         .accessibility_label("Website")
//!         .leading_text("https://")
//!         .trailing_text(".com")
//! }
//! # }
//! ```

use gpui::{
    AnyElement, App, ElementId, Entity, Focusable as _, InteractiveElement as _, IntoElement,
    ParentElement as _, Rems, RenderOnce, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, rems,
};

use crate::base::{
    ActiveTheme as _, ControlSize, Icon, IconName, Input, InputState, Sizable, focus_ring,
};

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// What goes before or after the text.
enum Addon {
    Icon(IconName),
    Text(SharedString),
    /// The host's: a button, a badge, a spinner. It keeps its own presses.
    Element(AnyElement),
}

/// A field with addons inside its border. It draws the [`Input`] of the [`InputState`] it's
/// given without the field's own look, and draws that look (the border, the input fill, the
/// focus ring while its text has the keyboard) around the text and the addons together. A
/// press on an icon or text addon puts the keyboard in the text, as a web `<label>` around
/// them would.
#[derive(IntoElement)]
pub struct InputGroup {
    state: Entity<InputState>,
    leading: Vec<Addon>,
    trailing: Vec<Addon>,
    label: Option<SharedString>,
    invalid: bool,
    size: ControlSize,
    style: StyleRefinement,
}

impl InputGroup {
    pub fn new(state: &Entity<InputState>) -> Self {
        Self {
            state: state.clone(),
            leading: Vec::new(),
            trailing: Vec::new(),
            label: None,
            invalid: false,
            size: ControlSize::Medium,
            style: StyleRefinement::default(),
        }
    }

    /// An icon before the text, in the muted colour.
    pub fn leading_icon(mut self, icon: IconName) -> Self {
        self.leading.push(Addon::Icon(icon));
        self
    }

    /// Text before the text (`https://`), in the muted colour.
    pub fn leading_text(mut self, text: impl Into<SharedString>) -> Self {
        self.leading.push(Addon::Text(text.into()));
        self
    }

    /// The host's element before the text: a button, say.
    pub fn leading(mut self, element: impl IntoElement) -> Self {
        self.leading
            .push(Addon::Element(element.into_any_element()));
        self
    }

    /// An icon after the text.
    pub fn trailing_icon(mut self, icon: IconName) -> Self {
        self.trailing.push(Addon::Icon(icon));
        self
    }

    /// Text after the text (`.com`, `USD`).
    pub fn trailing_text(mut self, text: impl Into<SharedString>) -> Self {
        self.trailing.push(Addon::Text(text.into()));
        self
    }

    /// The host's element after the text: a button, a count, a spinner.
    pub fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing
            .push(Addon::Element(element.into_any_element()));
        self
    }

    /// What the field is for, to assistive technology ([`Input::accessibility_label`]).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Drawn with the destructive colour's border, as a field that failed its check.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }
}

impl Sizable for InputGroup {
    /// Only [`ControlSize::Touch`] changes it: 44 points tall rather than 36.
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Styled for InputGroup {
    /// The group's style: its width, say (it fills its container's otherwise).
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for InputGroup {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let dark = theme.mode.is_dark();
        let focus = self.state.read(cx).focus_handle(cx);
        // The field's own ring shows while it has the keyboard from the keyboard
        // (`focus-visible`); the group shows it around everything. GPUI redraws the window when
        // the focus or the kind of input changes, so this is current.
        let ring = focus.is_focused(window) && window.last_input_was_keyboard();
        let border = if self.invalid {
            if dark {
                theme.destructive.opacity(0.5)
            } else {
                theme.destructive
            }
        } else if ring {
            theme.ring
        } else {
            theme.input
        };
        let mut shadows = Vec::new();
        if ring {
            shadows.push(focus_ring(cx));
        } else if self.invalid {
            let mut r = focus_ring(cx);
            r.color = theme.destructive.opacity(if dark { 0.4 } else { 0.2 });
            shadows.push(r);
        }
        let id = self.state.entity_id().as_u64();
        let muted = theme.muted_foreground;
        let side = |addons: Vec<Addon>, name: &'static str| {
            let element = addons.iter().any(|a| matches!(a, Addon::Element(_)));
            let empty = addons.is_empty();
            let addons = addons.into_iter().map(move |a| match a {
                Addon::Icon(icon) => Icon::new(icon)
                    .size(css(16.))
                    .color(muted)
                    .into_any_element(),
                Addon::Text(text) => div()
                    .min_w_0()
                    .flex_shrink_1()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .text_color(muted)
                    .child(text)
                    .into_any_element(),
                Addon::Element(e) => div().flex_none().child(e).into_any_element(),
            });
            (!empty).then(|| {
                div()
                    .debug_selector(move || format!("input-group-{name}-{id}"))
                    .flex()
                    .flex_shrink_1()
                    .min_w_0()
                    .items_center()
                    .gap(css(8.))
                    .children(addons)
                    // A button sits closer to the edge than text does, as shadcn's.
                    .map(|d| if element { d } else { d.px(css(4.)) })
            })
        };
        let leading_has_element = self.leading.iter().any(|a| matches!(a, Addon::Element(_)));
        let trailing_has_element = self.trailing.iter().any(|a| matches!(a, Addon::Element(_)));
        let leading = side(self.leading, "leading");
        let trailing = side(self.trailing, "trailing");
        let touch = self.size == ControlSize::Touch;
        let input = Input::new(&self.state)
            .appearance(false)
            .when_some(self.label, |i, l| i.accessibility_label(l))
            .flex_1()
            .min_w_0()
            .px(css(4.));
        let mut group = div()
            .id(ElementId::NamedInteger("input-group".into(), id))
            .debug_selector(move || format!("input-group-{id}"))
            .flex()
            .items_center()
            .w_full()
            .min_w_0()
            .min_h(css(36.))
            .when(touch, |d| d.min_h(theme.touch_target))
            .pl(css(if leading_has_element { 4. } else { 8. }))
            .pr(css(if trailing_has_element { 4. } else { 8. }))
            .rounded(theme.radius_md())
            .border_1()
            .border_color(border)
            .when(dark, |d| d.bg(theme.input.opacity(0.3)))
            // The ring (or an invalid field's red one) drawn just outside it, not under it, so
            // the group doesn't clip: its text addons and its input clip themselves.
            .relative()
            .children({
                let radius = theme.radius_md().to_pixels(window.rem_size());
                shadows
                    .into_iter()
                    .map(move |s| crate::base::ring_outside(s, radius, gpui::px(1.)))
            })
            .text_size(theme.text.sm)
            .cursor_text()
            // A press on an addon's icon or text (or the padding) puts the keyboard in the text;
            // a button's own press stops before it gets here.
            .on_click({
                let focus = focus.clone();
                move |_, window, cx| window.focus(&focus, cx)
            });
        gpui::Refineable::refine(group.style(), &self.style);
        group.children(leading).child(input).children(trailing)
    }
}
