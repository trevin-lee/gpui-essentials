//! Labels and fields, as shadcn lays them out (its `label.tsx` and `field.tsx`): a [`Label`]
//! for a control, a press on which focuses or toggles it; a [`Field`] that puts a label, the
//! control, a description and an error message together, one above the other or side by side;
//! and [`FieldSet`], [`FieldLegend`] and [`FieldGroup`] to group fields.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{Field, Input, InputState};
//! # struct Form { email: Entity<InputState>, error: Option<SharedString> }
//! # impl Render for Form {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     Field::new("email")
//!         .label("Email")
//!         .for_focus(&self.email.focus_handle(cx))
//!         .description("We'll only use it to sign you in.")
//!         .error(self.error.clone())
//!         .child(Input::new(&self.email))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    AnyElement, App, ClickEvent, ElementId, FocusHandle, FontWeight, InteractiveElement as _,
    IntoElement, ParentElement, RenderOnce, Role, SharedString, StatefulInteractiveElement as _,
    StyleRefinement, Styled, Window, div, prelude::FluentBuilder as _, relative, rems,
};

use crate::base::{ActiveTheme as _, Disableable};

fn css(v: f32) -> gpui::Rems {
    rems(v / 15.)
}

type OnClick = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A control's name, drawn beside or above it: small medium-weight text that wraps. A press on
/// it focuses the control ([`for_focus`](Label::for_focus)) or does what the host says
/// ([`on_click`](Label::on_click): toggles a check box, say), as a web `<label for>` does. It
/// takes no focus itself.
#[derive(IntoElement)]
pub struct Label {
    id: ElementId,
    text: SharedString,
    focus: Option<FocusHandle>,
    on_click: Option<OnClick>,
    disabled: bool,
    style: StyleRefinement,
}

impl Label {
    pub fn new(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Self {
        Self {
            id: id.into(),
            text: text.into(),
            focus: None,
            on_click: None,
            disabled: false,
            style: StyleRefinement::default(),
        }
    }

    /// The control a press on it gives the focus to (a field's
    /// [`InputState`](crate::base::InputState), whose handle `focus_handle(cx)` gives).
    pub fn for_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    /// Called on a press on it (after it's focused [`for_focus`](Self::for_focus)'s control):
    /// the host toggles its check box or switch here.
    pub fn on_click(mut self, f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self {
        self.on_click = Some(Rc::new(f));
        self
    }
}

impl Disableable for Label {
    /// Dimmed, and a press on it does nothing.
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl Styled for Label {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Label {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let pressable = !self.disabled && (self.focus.is_some() || self.on_click.is_some());
        let (focus, on_click) = (self.focus, self.on_click);
        let mut el = div()
            .id(self.id.clone())
            .role(Role::Label)
            .debug_selector({
                let id = self.id.clone();
                move || format!("label-{id}")
            })
            .min_w_0()
            .text_size(theme.text.sm)
            .line_height(relative(1.375))
            .font_weight(FontWeight::MEDIUM)
            .when(self.disabled, |d| d.opacity(0.5))
            .when(pressable, |d| {
                d.cursor_pointer().on_click(move |e, window, cx| {
                    if let Some(f) = &focus {
                        window.focus(f, cx);
                    }
                    if let Some(f) = &on_click {
                        f(e, window, cx);
                    }
                })
            });
        gpui::Refineable::refine(el.style(), &self.style);
        el.child(self.text)
    }
}

/// A field: its [`label`](Field::label), its control (its children), a
/// [`description`](Field::description) and an [`error`](Field::error), one above the other, or
/// side by side with [`horizontal`](Field::horizontal) (the label and description beside the
/// control, as for a switch or a check box). An error shows in the destructive colour, the
/// label turning that colour too. To assistive tech it's a group named by its label and
/// described by its error, else its description; the control itself takes its own name (an
/// [`Input`](crate::base::Input)'s `accessibility_label`, a check box's label).
#[derive(IntoElement)]
pub struct Field {
    id: ElementId,
    label: Option<SharedString>,
    focus: Option<FocusHandle>,
    on_label_click: Option<OnClick>,
    description: Option<SharedString>,
    error: Option<SharedString>,
    horizontal: bool,
    control_first: bool,
    disabled: bool,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl Field {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            label: None,
            focus: None,
            on_label_click: None,
            description: None,
            error: None,
            horizontal: false,
            control_first: false,
            disabled: false,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// The control's name, drawn as a [`Label`].
    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// The control a press on the label focuses ([`Label::for_focus`]).
    pub fn for_focus(mut self, focus: &FocusHandle) -> Self {
        self.focus = Some(focus.clone());
        self
    }

    /// Called on a press on the label ([`Label::on_click`]): the host toggles its control.
    pub fn on_label_click(
        mut self,
        f: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_label_click = Some(Rc::new(f));
        self
    }

    /// Help under the label and control, in the muted colour. It wraps.
    pub fn description(mut self, text: impl Into<SharedString>) -> Self {
        self.description = Some(text.into());
        self
    }

    /// What's wrong with the value, in the destructive colour; `None` for nothing. It wraps.
    pub fn error(mut self, error: Option<impl Into<SharedString>>) -> Self {
        self.error = error.map(Into::into);
        self
    }

    /// The label (and description) beside the control rather than above it: on the left, the
    /// control on the right, unless [`control_first`](Self::control_first).
    pub fn horizontal(mut self, horizontal: bool) -> Self {
        self.horizontal = horizontal;
        self
    }

    /// Side by side, the control before the label (a check box's layout).
    pub fn control_first(mut self, first: bool) -> Self {
        self.control_first = first;
        self
    }
}

impl Disableable for Field {
    /// The label and description dimmed, and a press on the label doing nothing. The control's
    /// own `disabled` is the host's to set.
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl ParentElement for Field {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl Styled for Field {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Field {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let invalid = self.error.is_some();
        let id = self.id.clone();
        let label = self.label.clone().map(|text| {
            let mut l = Label::new(
                ElementId::NamedChild(std::sync::Arc::new(id.clone()), "label".into()),
                text,
            )
            .disabled(self.disabled)
            .when(invalid, |l| l.text_color(theme.destructive));
            if let Some(f) = &self.focus {
                l = l.for_focus(f);
            }
            if let Some(f) = self.on_label_click.clone() {
                l = l.on_click(move |e, w, cx| f(e, w, cx));
            }
            l
        });
        let small = |text: SharedString, color| {
            div()
                .min_w_0()
                .text_size(theme.text.sm)
                .line_height(relative(1.5))
                .text_color(color)
                .child(text)
        };
        let description = self.description.clone().map(|d| {
            small(d, theme.muted_foreground)
                .when(self.disabled, |d| d.opacity(0.5))
                .debug_selector({
                    let id = id.clone();
                    move || format!("field-description-{id}")
                })
        });
        let error = self.error.clone().map(|e| {
            small(e, theme.destructive).debug_selector({
                let id = id.clone();
                move || format!("field-error-{id}")
            })
        });
        let control = div()
            .min_w_0()
            .flex()
            .flex_col()
            .when(!self.horizontal, |d| d.w_full())
            .when(self.horizontal, |d| d.flex_none().max_w_full())
            .children(self.children);
        let mut el = div()
            .id(self.id.clone())
            .role(Role::Group)
            .when_some(self.label.clone(), |d, l| d.aria_label(l))
            .when_some(
                self.error.clone().or(self.description.clone()),
                |d, text| d.aria_description(text),
            )
            .debug_selector({
                let id = id.clone();
                move || format!("field-{id}")
            })
            .flex()
            .w_full()
            .min_w_0();
        if self.horizontal {
            // The label, description and error together, beside the control.
            let content = div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(css(6.))
                .children(label)
                .children(description)
                .children(error);
            el = el.flex_row().items_center().gap(css(12.));
            el = if self.control_first {
                el.child(control).child(content)
            } else {
                el.child(content).child(control)
            };
        } else {
            el = el
                .flex_col()
                .gap(css(8.))
                .children(label)
                .child(control)
                .children(description)
                .children(error);
        }
        gpui::Refineable::refine(el.style(), &self.style);
        el
    }
}

/// A set of fields under a [`FieldLegend`], with an optional description: a fieldset, to
/// assistive tech a group named by its legend.
#[derive(IntoElement)]
pub struct FieldSet {
    id: ElementId,
    legend: Option<FieldLegend>,
    description: Option<SharedString>,
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl FieldSet {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            legend: None,
            description: None,
            children: Vec::new(),
            style: StyleRefinement::default(),
        }
    }

    /// Its title.
    pub fn legend(mut self, legend: impl Into<FieldLegend>) -> Self {
        self.legend = Some(legend.into());
        self
    }

    /// Help under the legend, in the muted colour.
    pub fn description(mut self, text: impl Into<SharedString>) -> Self {
        self.description = Some(text.into());
        self
    }
}

impl ParentElement for FieldSet {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl Styled for FieldSet {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for FieldSet {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        let name = self.legend.as_ref().map(|l| l.text.clone());
        let mut el = div()
            .id(self.id)
            .role(Role::Group)
            .when_some(name, |d, n| d.aria_label(n))
            .when_some(self.description.clone(), |d, t| d.aria_description(t))
            .flex()
            .flex_col()
            .w_full()
            .min_w_0()
            .gap(css(24.));
        gpui::Refineable::refine(el.style(), &self.style);
        let head = (self.legend.is_some() || self.description.is_some()).then(|| {
            div()
                .flex()
                .flex_col()
                .gap(css(6.))
                .min_w_0()
                .children(self.legend)
                .when_some(self.description, |d, t| {
                    d.child(
                        div()
                            .min_w_0()
                            .text_size(theme.text.sm)
                            .line_height(relative(1.5))
                            .text_color(theme.muted_foreground)
                            .child(t),
                    )
                })
        });
        el.children(head).children(self.children)
    }
}

/// A [`FieldSet`]'s title: medium-weight text in the base size, or the label's smaller size
/// with [`label_size`](FieldLegend::label_size). It wraps.
#[derive(IntoElement)]
pub struct FieldLegend {
    text: SharedString,
    label_size: bool,
}

impl FieldLegend {
    pub fn new(text: impl Into<SharedString>) -> Self {
        Self {
            text: text.into(),
            label_size: false,
        }
    }

    /// In a label's size (shadcn's `variant="label"`), for a set inside a form's section.
    pub fn label_size(mut self, label: bool) -> Self {
        self.label_size = label;
        self
    }
}

impl<T: Into<SharedString>> From<T> for FieldLegend {
    fn from(text: T) -> Self {
        FieldLegend::new(text)
    }
}

impl RenderOnce for FieldLegend {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme();
        div()
            .min_w_0()
            .font_weight(FontWeight::MEDIUM)
            .text_size(if self.label_size {
                theme.text.sm
            } else {
                theme.text.base
            })
            .line_height(relative(1.375))
            .child(self.text)
    }
}

/// Fields one above the other with room between them (shadcn's `FieldGroup`).
#[derive(IntoElement, Default)]
pub struct FieldGroup {
    children: Vec<AnyElement>,
    style: StyleRefinement,
}

impl FieldGroup {
    pub fn new() -> Self {
        Self::default()
    }
}

impl ParentElement for FieldGroup {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements)
    }
}

impl Styled for FieldGroup {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for FieldGroup {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut el = div().flex().flex_col().w_full().min_w_0().gap(css(28.));
        gpui::Refineable::refine(el.style(), &self.style);
        el.children(self.children)
    }
}
