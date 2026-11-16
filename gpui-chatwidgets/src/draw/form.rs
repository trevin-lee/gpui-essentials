//! Forms and their controls, buttons and icon buttons. Text fields, the slider and an icon
//! button's tooltip are gpui-core's (their editing and dragging), drawn in the card's colours;
//! the date picker is still GPUI Kit's (its popover). The others are drawn here, as gpui-core's
//! presses so the keyboard reaches them.

use std::rc::Rc;

use gpui_core::{Disableable as _, InputState, Selectable as _, Slider, Tooltip};
use gpui_kit::component::date_picker::DatePicker;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::{Value as Json, json};

use super::content::Glyph;
use super::{Draw, pressable};
use crate::openui::{
    Button, ButtonVariant, Buttons, Choice, Direction, Field, Form, IconButton, Input, Size,
    Surface, Toggle,
};
use crate::view::{Binding, CardView};

/// A form field's key: its form's path and its place.
pub(crate) fn field_key(path: &str, i: usize) -> String {
    format!("{path}/f{i}")
}

/// A choice row's press: what it is to assistive technology (a check box, a radio button or a
/// switch), its label, and whether it's checked, which it reports as toggled. A tap picks
/// without taking the focus (from the chat's text field, say); Tab still stops on it.
fn choice_press(
    id: ElementId,
    t: &crate::theme::CardTheme,
    role: Role,
    label: &str,
    checked: bool,
    disabled: bool,
) -> gpui_core::Press {
    pressable(id, t)
        .role(role)
        .accessibility_label(label.to_string())
        .selected(checked)
        .disabled(disabled)
        .focus_on_press(false)
}

/// What a press in a form carries along: the form's name and its fields, to check first.
#[derive(Clone)]
pub(crate) struct FormRef {
    pub name: String,
    pub fields: Rc<Vec<(String, Binding)>>,
}

pub(crate) type Pick = Rc<dyn Fn(&mut CardView, String, &mut Window, &mut Context<CardView>)>;

impl Draw<'_, '_> {
    pub fn form(&mut self, f: &Form, path: &str) -> AnyElement {
        let t = self.t;
        let fields: Vec<(String, Binding)> = f
            .fields
            .iter()
            .enumerate()
            .map(|(i, fc)| {
                (
                    field_key(path, i),
                    Binding {
                        form: Some(f.name.clone()),
                        field: fc.field.clone(),
                    },
                )
            })
            .collect();
        let form = FormRef {
            name: f.name.clone(),
            fields: Rc::new(fields.clone()),
        };
        let mut col = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(t.space.l));
        for (fc, (key, binding)) in f.fields.iter().zip(fields.iter()) {
            let required = binding.field.rules.required();
            let label = if required {
                format!("{} *", fc.label)
            } else {
                fc.label.clone()
            };
            let error = self.v.ui.errors.get(key).cloned();
            let control = div()
                .w_full()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(self.px(6.))
                .when(!fc.label.is_empty(), |d| {
                    d.child(self.text(label, t.text.sm, t.colors.text, t.text.medium))
                })
                .child(self.field(key, binding))
                .when_some(fc.hint.clone(), |d, h| {
                    d.child(self.text(h, t.text.sm, t.colors.text_muted, self.regular()))
                })
                .when_some(error, |d, e| {
                    let sel = self.selector(&format!("error:{}", binding.field.name));
                    d.child(
                        self.text(e, t.text.sm, t.colors.danger.text, self.regular())
                            .debug_selector(move || sel),
                    )
                });
            col = col.child(control);
        }
        if !f.buttons.buttons.is_empty() {
            col = col.child(self.buttons(&f.buttons, &format!("{path}.b"), Some(&form), None));
        }
        col.into_any_element()
    }

    /// An input outside a form.
    pub fn loose_field(&mut self, field: &Field, path: &str) -> AnyElement {
        let binding = Binding {
            form: None,
            field: field.clone(),
        };
        let t = self.t;
        let error = self.v.ui.errors.get(path).cloned();
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(6.))
            .child(self.field(path, &binding))
            .when_some(error, |d, e| {
                let sel = self.selector(&format!("error:{}", field.name));
                d.child(
                    self.text(e, t.text.sm, t.colors.danger.text, self.regular())
                        .debug_selector(move || sel),
                )
            })
            .into_any_element()
    }

    /// The control for a field, reading its value from the store.
    fn field(&mut self, key: &str, b: &Binding) -> AnyElement {
        let t = self.t;
        let value = self.v.read(b);
        match &b.field.input {
            Input::Text { .. } | Input::TextArea { .. } => {
                let Some(entry) = self.v.texts.get(key) else {
                    return div().into_any_element();
                };
                let invalid = self.v.ui.errors.contains_key(key);
                let wrap = self.sideways(
                    div()
                        .id(self.id(&format!("{key}.field")))
                        .w_full()
                        .min_w_0(),
                    &format!("{key}.field"),
                );
                let area = matches!(b.field.input, Input::TextArea { .. });
                let field = self.text_field(&entry.state, area, invalid);
                wrap.child(field).into_any_element()
            }
            Input::Select {
                options,
                placeholder,
                ..
            } => {
                let current = value.as_ref().and_then(Json::as_str).map(str::to_string);
                let (k, binding) = (key.to_string(), b.clone());
                let pick: Pick = Rc::new(move |this, v, _, cx| {
                    this.ui.select_open = None;
                    this.edit(&k, &binding, Json::String(v), cx);
                });
                let opts: Vec<(String, String)> = options
                    .iter()
                    .filter(|o| !o.disabled)
                    .map(|o| (o.value.clone(), o.label.clone()))
                    .collect();
                self.dropdown(
                    key,
                    &opts,
                    current.as_deref(),
                    placeholder.as_deref().unwrap_or("Select..."),
                    pick,
                )
            }
            Input::Date { .. } => match self.v.dates.get(key) {
                Some(e) => self
                    .sideways(
                        div().id(self.id(&format!("{key}.date"))).w_full().min_w_0(),
                        &format!("{key}.date"),
                    )
                    .child(
                        DatePicker::new(&e.state)
                            .placeholder("Pick a date")
                            .w_full(),
                    )
                    .into_any_element(),
                None => div().into_any_element(),
            },
            Input::Slider {
                label, min, max, ..
            } => {
                let Some(e) = self.v.sliders.get(key) else {
                    return div().into_any_element();
                };
                let now = e.state.read(self.cx).value();
                let shown = crate::openui::num_text(now as f64);
                let (lo, hi) = crate::view::slider_range(*min, *max);
                div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(self.px(8.))
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .gap(self.px(8.))
                            .child(
                                self.line(
                                    label.clone().unwrap_or_else(|| b.field.name.clone()),
                                    t.text.sm,
                                    t.colors.text_muted,
                                    self.regular(),
                                )
                                .flex_1(),
                            )
                            .child(
                                self.line(shown, t.text.sm, t.colors.text, t.text.medium)
                                    .flex_none(),
                            ),
                    )
                    .child({
                        let sel = self.selector(&format!("slider:{key}"));
                        div().w_full().min_w_0().debug_selector(move || sel).child(
                            Slider::new(&e.state)
                                .track_color(t.colors.line_strong)
                                .fill_color(t.colors.accent)
                                .thumb_color(t.colors.background)
                                .w_full()
                                .when(t.touch, |s| s.touch()),
                        )
                    })
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .flex()
                            .flex_row()
                            .justify_between()
                            .child(self.line(
                                crate::openui::num_text(lo as f64),
                                t.text.xs,
                                t.colors.text_faint,
                                self.regular(),
                            ))
                            .child(self.line(
                                crate::openui::num_text(hi as f64),
                                t.text.xs,
                                t.colors.text_faint,
                                self.regular(),
                            )),
                    )
                    .into_any_element()
            }
            Input::CheckBoxes(items) => self.checkboxes(key, b, items, value.as_ref()),
            Input::Radio { options, default } => {
                let current = value
                    .as_ref()
                    .and_then(Json::as_str)
                    .map(str::to_string)
                    .or_else(|| default.clone());
                let mut col = div().w_full().min_w_0().flex().flex_col().gap(self.px(10.));
                for (i, o) in options.iter().enumerate() {
                    let on = current.as_deref() == Some(o.value.as_str());
                    let (k, binding, v) = (key.to_string(), b.clone(), o.value.clone());
                    let dot = div()
                        .flex_none()
                        .size(self.px(16.))
                        .mt(self.px(3.))
                        .rounded_full()
                        .border_1()
                        .border_color(if on {
                            t.colors.accent
                        } else {
                            t.colors.line_strong
                        })
                        .flex()
                        .items_center()
                        .justify_center()
                        .when(on, |d| {
                            d.child(div().size(self.px(8.)).rounded_full().bg(t.colors.accent))
                        });
                    col = col.child(self.choice_row(
                        &format!("{key}.r{i}"),
                        dot,
                        &o.label,
                        o.description.as_deref(),
                        o.disabled,
                        Role::RadioButton,
                        on,
                        move |this, _, cx| this.edit(&k, &binding, Json::String(v.clone()), cx),
                    ));
                }
                col.into_any_element()
            }
            Input::Switches { items, surface } => {
                self.switches(key, b, items, *surface, value.as_ref())
            }
            Input::Chips {
                options, multiple, ..
            } => {
                let chosen = chosen(value.as_ref());
                let mut row = div()
                    .w_full()
                    .min_w_0()
                    .flex()
                    .flex_row()
                    .flex_wrap()
                    .gap(self.px(8.));
                for (i, o) in options.iter().enumerate() {
                    let on = chosen.contains(&o.value);
                    let (k, binding, v, multiple) =
                        (key.to_string(), b.clone(), o.value.clone(), *multiple);
                    let pick = self.cx.listener(move |this, _, _, cx| {
                        let mut now = chosen_of(this.read(&binding).as_ref());
                        let value = if multiple {
                            if !now.iter().any(|x| x == &v) {
                                now.push(v.clone());
                            } else {
                                now.retain(|x| x != &v);
                            }
                            json!(now)
                        } else if now.first() == Some(&v) {
                            Json::Null
                        } else {
                            Json::String(v.clone())
                        };
                        this.edit(&k, &binding, value, cx);
                    });
                    let chip = pressable(self.id(&format!("{key}.chip{i}")), self.t)
                        .role(Role::CheckBox)
                        .accessibility_label(o.label.clone())
                        .disabled(o.disabled)
                        .selected(on)
                        .focus_on_press(false)
                        .flex_none()
                        .max_w_full()
                        .min_w_0()
                        .h(self.px(32.))
                        .px(self.px(12.))
                        .gap(self.px(6.))
                        .rounded_full()
                        .border_1()
                        .border_color(if on {
                            t.colors.accent
                        } else {
                            t.colors.line_strong
                        })
                        .when(on, |c| c.bg(t.colors.raised))
                        .when(!on && !o.disabled, |c| {
                            c.hover(|s| s.bg(t.colors.highlight))
                        })
                        .when(o.disabled, |c| c.opacity(0.45))
                        .debug_selector({
                            let sel = self.selector(&format!("chip:{}", o.label));
                            move || sel
                        })
                        .when_some(o.icon.as_ref(), |c, i| {
                            c.child(self.icon_element(&i.name, 14., t.colors.text))
                        })
                        .child(self.line(
                            o.label.clone(),
                            t.text.sm,
                            t.colors.text,
                            self.regular(),
                        ));
                    row = row.child(self.on_press(chip, pick));
                }
                row.into_any_element()
            }
            Input::OptionCards {
                options, multiple, ..
            } => self.option_cards(key, b, options, *multiple, value.as_ref()),
        }
    }

    /// A row a person presses to choose: the mark (a box, a dot), a label and a description.
    /// `role` is a check box, a radio button or a switch, and `checked` its state.
    #[allow(clippy::too_many_arguments)]
    fn choice_row(
        &mut self,
        id: &str,
        mark: Div,
        label: &str,
        description: Option<&str>,
        disabled: bool,
        role: Role,
        checked: bool,
        on: impl Fn(&mut CardView, &mut Window, &mut Context<CardView>) + 'static,
    ) -> AnyElement {
        let t = self.t;
        let choose = self
            .cx
            .listener(move |this, _, window, cx| on(this, window, cx));
        let press = choice_press(self.id(id), t, role, label, checked, disabled)
            .w_full()
            .min_w_0()
            .justify_start()
            .items_start()
            .gap(self.px(10.))
            .rounded(self.px(t.radii.sm))
            .when(disabled, |b| b.opacity(0.45))
            .debug_selector({
                let sel = self.selector(&format!("choice:{label}"));
                move || sel
            })
            .child(mark)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .line_height(self.px(t.text.sm * t.text.body_leading))
                    .child(self.text(label.to_string(), t.text.sm, t.colors.text, self.regular()))
                    .when_some(description, |d, s| {
                        d.child(self.text(
                            s.to_string(),
                            t.text.sm,
                            t.colors.text_muted,
                            self.regular(),
                        ))
                    }),
            );
        self.on_press(press, choose).into_any_element()
    }

    fn checkboxes(
        &mut self,
        key: &str,
        b: &Binding,
        items: &[Toggle],
        value: Option<&Json>,
    ) -> AnyElement {
        let t = self.t;
        let mut col = div().w_full().min_w_0().flex().flex_col().gap(self.px(10.));
        for (i, it) in items.iter().enumerate() {
            let on = toggled(value, it);
            let (k, binding, name) = (key.to_string(), b.clone(), it.name.clone());
            let items: Vec<Toggle> = items.to_vec();
            let mark = div()
                .flex_none()
                .size(self.px(16.))
                .mt(self.px(3.))
                .rounded(self.px(4.))
                .border_1()
                .border_color(if on {
                    t.colors.accent
                } else {
                    t.colors.line_strong
                })
                .when(on, |d| d.bg(t.colors.accent))
                .flex()
                .items_center()
                .justify_center()
                .when(on, |d| {
                    d.child(self.glyph(Glyph::Check, 12., t.colors.on_accent))
                });
            let label = it.label.clone().unwrap_or_else(|| it.name.clone());
            col = col.child(self.choice_row(
                &format!("{key}.x{i}"),
                mark,
                &label,
                it.description.as_deref(),
                false,
                Role::CheckBox,
                on,
                move |this, _, cx| {
                    let now = this.read(&binding);
                    let mut agg = serde_json::Map::new();
                    for it in &items {
                        agg.insert(it.name.clone(), Json::Bool(toggled(now.as_ref(), it)));
                    }
                    let flipped = !agg.get(&name).and_then(Json::as_bool).unwrap_or(false);
                    agg.insert(name.clone(), Json::Bool(flipped));
                    this.edit(&k, &binding, Json::Object(agg), cx);
                },
            ));
        }
        col.into_any_element()
    }

    fn switches(
        &mut self,
        key: &str,
        b: &Binding,
        items: &[Toggle],
        surface: Surface,
        value: Option<&Json>,
    ) -> AnyElement {
        let t = self.t;
        let mut col = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(if surface == Surface::Clear { 10. } else { 8. }));
        for (i, it) in items.iter().enumerate() {
            let on = toggled(value, it);
            let (k, binding, name) = (key.to_string(), b.clone(), it.name.clone());
            let items: Vec<Toggle> = items.to_vec();
            let track = div()
                .flex_none()
                .w(self.px(32.))
                .h(self.px(18.))
                .mt(self.px(2.))
                .rounded_full()
                .bg(if on {
                    t.colors.accent
                } else {
                    t.colors.line_strong
                })
                .p(self.px(2.))
                .flex()
                .when(on, |d| d.justify_end())
                .child(div().size(self.px(14.)).rounded_full().bg(if on {
                    t.colors.on_accent
                } else {
                    t.colors.text
                }));
            let label = it.label.clone().unwrap_or_else(|| it.name.clone());
            let row = self.choice_row(
                &format!("{key}.w{i}"),
                track,
                &label,
                it.description.as_deref(),
                false,
                Role::Switch,
                on,
                move |this, _, cx| {
                    let now = this.read(&binding);
                    let mut agg = serde_json::Map::new();
                    for it in &items {
                        agg.insert(it.name.clone(), Json::Bool(toggled(now.as_ref(), it)));
                    }
                    let flipped = !agg.get(&name).and_then(Json::as_bool).unwrap_or(false);
                    agg.insert(name.clone(), Json::Bool(flipped));
                    this.edit(&k, &binding, Json::Object(agg), cx);
                },
            );
            col = col.child(match surface {
                Surface::Clear => div().w_full().min_w_0().child(row),
                _ => div()
                    .w_full()
                    .min_w_0()
                    .p(self.px(12.))
                    .rounded(self.px(t.radii.lg))
                    .border_1()
                    .border_color(t.colors.line)
                    .when(surface == Surface::Sunk, |d| d.bg(t.colors.sunk))
                    .child(row),
            });
        }
        col.into_any_element()
    }

    fn option_cards(
        &mut self,
        key: &str,
        b: &Binding,
        options: &[Choice],
        multiple: bool,
        value: Option<&Json>,
    ) -> AnyElement {
        let t = self.t;
        let chosen = chosen(value);
        let mut grid = div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_row()
            .flex_wrap()
            .gap(self.px(8.));
        for (i, o) in options.iter().enumerate() {
            let on = chosen.contains(&o.value);
            let (k, binding, v) = (key.to_string(), b.clone(), o.value.clone());
            let top = match (&o.image, &o.icon) {
                (Some(im), _) => match self.picture(im.src.as_ref()) {
                    Some(img) => Some(
                        div()
                            .w_full()
                            .h(self.px(72.))
                            .rounded(self.px(t.radii.sm))
                            .overflow_hidden()
                            .child(
                                super::content::img_from(img)
                                    .size_full()
                                    .object_fit(ObjectFit::Cover)
                                    .rounded(self.px(t.radii.sm)),
                            )
                            .into_any_element(),
                    ),
                    None => Some(
                        self.glyph(Glyph::Picture, 20., t.colors.text_muted)
                            .into_any_element(),
                    ),
                },
                (None, Some(icon)) => Some(self.icon_element(&icon.name, 20., t.colors.text)),
                _ => None,
            };
            let card = pressable(self.id(&format!("{key}.card{i}")), self.t)
                .role(if multiple {
                    Role::CheckBox
                } else {
                    Role::RadioButton
                })
                .accessibility_label(o.label.clone())
                .disabled(o.disabled)
                .selected(on)
                .focus_on_press(false)
                .flex_basis(self.px(140.))
                .flex_grow(1.)
                .flex_shrink(1.)
                .max_w_full()
                .min_w_0()
                .flex_col()
                .items_start()
                .justify_start()
                .gap(self.px(6.))
                .p(self.px(12.))
                .rounded(self.px(t.radii.lg))
                .border_1()
                .border_color(if on { t.colors.accent } else { t.colors.line })
                .when(on, |c| c.bg(t.colors.raised))
                .when(!on, |c| c.hover(|s| s.bg(t.colors.highlight)))
                .debug_selector({
                    let sel = self.selector(&format!("option:{}", o.label));
                    move || sel
                })
                .children(top)
                .child(
                    self.text(o.label.clone(), t.text.sm, t.colors.text, t.text.medium)
                        .w_full(),
                )
                .when_some(o.description.clone(), |c, d| {
                    c.child(
                        self.text(d, t.text.sm, t.colors.text_muted, self.regular())
                            .w_full(),
                    )
                })
                .on_click(self.cx.listener(move |this, _, _, cx| {
                    let mut now = chosen_of(this.read(&binding).as_ref());
                    let value = if multiple {
                        if !now.iter().any(|x| x == &v) {
                            now.push(v.clone());
                        } else {
                            now.retain(|x| x != &v);
                        }
                        json!(now)
                    } else {
                        Json::String(v.clone())
                    };
                    this.edit(&k, &binding, value, cx);
                }));
            grid = grid.child(card);
        }
        grid.into_any_element()
    }

    /// A button showing the chosen option, with the options in a list under it while open.
    /// A text field or text area: gpui-core's input with no look of its own, drawn in the
    /// card's colours as the web's (`input.tsx`, `textarea.tsx`): a 1-pixel edge (red when its
    /// value was refused), filled on a dark card, 12 pixels in from the sides, a one-line field
    /// as tall as a select, and the edge and ring of the focus while it has it.
    pub fn text_field(
        &mut self,
        state: &Entity<InputState>,
        area: bool,
        invalid: bool,
    ) -> gpui_core::Input {
        let t = self.t;
        let focused = state.read(self.cx).is_focused(self.window);
        let edge = if invalid {
            t.colors.danger.emphasis
        } else if focused {
            t.colors.accent
        } else {
            t.colors.line_strong
        };
        gpui_core::Input::new(state)
            .appearance(false)
            .w_full()
            .min_w_0()
            .px(self.px(12.))
            .py(self.px(if area { 8. } else { 4. }))
            .when(!area, |i| i.min_h(self.px(36.)).items_center())
            .rounded(self.px(t.radii.md))
            .border_1()
            .border_color(edge)
            .when(t.dark, |i| i.bg(t.colors.sunk))
            .when(focused, |i| {
                i.shadow(vec![BoxShadow {
                    color: t.colors.focus_ring,
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(0.),
                    spread_radius: px(3.),
                    inset: false,
                }])
            })
            .font_family(t.fonts.body.clone())
            .text_size(self.px(t.text.sm))
            .line_height(self.px(t.text.sm * t.text.body_leading))
            .text_color(t.colors.text)
            .caret_color(t.colors.text)
            .selection_color(selection(t))
            .placeholder_color(t.colors.text_muted)
    }

    pub fn dropdown(
        &mut self,
        key: &str,
        options: &[(String, String)],
        current: Option<&str>,
        placeholder: &str,
        pick: Pick,
    ) -> AnyElement {
        let t = self.t;
        let open = self.v.ui.select_open.as_deref() == Some(key);
        let label = current
            .and_then(|c| options.iter().find(|(v, _)| v == c))
            .map(|(_, l)| l.clone());
        let shown = label.clone().unwrap_or_else(|| placeholder.to_string());
        let k = key.to_string();
        let toggle = self.cx.listener(move |this, _, _, cx| {
            this.ui.select_open = if this.ui.select_open.as_deref() == Some(k.as_str()) {
                None
            } else {
                Some(k.clone())
            };
            cx.notify();
        });
        let trigger = pressable(self.id(&format!("{key}.select")), self.t)
            .role(Role::ComboBox)
            .accessibility_label(shown.clone())
            .w_full()
            .min_w_0()
            .h(self.px(36.))
            .px(self.px(12.))
            .justify_between()
            .gap(self.px(8.))
            .rounded(self.px(t.radii.md))
            .border_1()
            .border_color(if open {
                t.colors.accent
            } else {
                t.colors.line_strong
            })
            .hover(|s| s.bg(t.colors.highlight))
            .debug_selector({
                let sel = self.selector(&format!("select:{key}"));
                move || sel
            })
            .child(
                self.line(
                    shown,
                    t.text.sm,
                    if label.is_some() {
                        t.colors.text
                    } else {
                        t.colors.text_faint
                    },
                    self.regular(),
                )
                .flex_1(),
            )
            .child(self.glyph(Glyph::ChevronDown, 14., t.colors.text_muted));
        let trigger = self.on_press(trigger, toggle);
        let mut el = div().relative().w_full().min_w_0().child(trigger);
        if open {
            let mut list = div()
                .id(self.id(&format!("{key}.options")))
                .occlude()
                .w_full()
                .max_h(self.px(240.))
                .overflow_y_scroll()
                .mt(self.px(4.))
                .p(self.px(4.))
                .flex()
                .flex_col()
                .rounded(self.px(t.radii.md))
                .bg(t.colors.popover)
                .border_1()
                .border_color(t.colors.line_strong)
                .shadow_md()
                .font_family(t.fonts.body.clone())
                .on_mouse_down_out(self.cx.listener(|this, _, _, cx| {
                    this.ui.select_open = None;
                    cx.notify();
                }));
            for (i, (value, label)) in options.iter().enumerate() {
                let on = current == Some(value.as_str());
                let (pick, v) = (pick.clone(), value.clone());
                let choose = self
                    .cx
                    .listener(move |this, _, window, cx| pick(this, v.clone(), window, cx));
                let option = pressable(self.id(&format!("{key}.opt{i}")), self.t)
                    .role(Role::ListBoxOption)
                    .accessibility_label(label.clone())
                    .selected(on)
                    .w_full()
                    .min_w_0()
                    .h(self.px(32.))
                    .px(self.px(8.))
                    .justify_between()
                    .rounded(self.px(t.radii.sm))
                    .hover(|s| s.bg(t.colors.highlight))
                    .debug_selector({
                        let sel = self.selector(&format!("pick:{label}"));
                        move || sel
                    })
                    .child(
                        self.line(label.clone(), t.text.sm, t.colors.text, self.regular())
                            .flex_1(),
                    )
                    .when(on, |b| {
                        b.child(self.glyph(Glyph::Check, 14., t.colors.text))
                    });
                // No touch square: the options touch, so it would only take the edge of the
                // one above or below.
                list = list.child(option.on_click(choose));
            }
            el = el.child(
                deferred(
                    anchored().snap_to_window_with_margin(px(8.)).child(
                        div()
                            .w(self.window.viewport_size().width.min(self.pixels(320.)))
                            .child(list),
                    ),
                )
                .with_priority(1),
            );
        }
        el.into_any_element()
    }

    /// A button. In a form, a primary one checks the form's rules first.
    pub fn button(
        &mut self,
        b: &Button,
        path: &str,
        form: Option<&FormRef>,
        _: Option<()>,
    ) -> Stateful<Div> {
        let t = self.t;
        let (font, vpad, hpad, radius) = match b.size {
            Size::ExtraSmall | Size::Xs => (t.text.sm, 4.25, 8., 6.),
            Size::Small | Size::Sm | Size::S => (t.text.sm, 6.25, 10., 7.),
            Size::Large | Size::Lg | Size::L | Size::Xl => (t.text.md, 9., 12., 10.),
            _ => (t.text.md, 7., 10., 8.),
        };
        let kind = if b.variant == ButtonVariant::Ghost {
            ButtonVariant::Tertiary
        } else {
            b.variant
        };
        let (bg, fg, edge) = match (kind, b.destructive) {
            (ButtonVariant::Primary, false) => {
                (t.colors.accent, t.colors.on_accent, t.colors.accent)
            }
            (ButtonVariant::Primary, true) => (
                t.colors.destructive,
                t.colors.on_destructive,
                t.colors.destructive,
            ),
            (ButtonVariant::Secondary, false) => {
                (transparent_black(), t.colors.text, t.colors.line_strong)
            }
            (ButtonVariant::Secondary, true) => (
                t.colors.danger.background,
                t.colors.danger.text,
                t.colors.danger.background,
            ),
            (_, false) => (transparent_black(), t.colors.text, transparent_black()),
            (_, true) => (
                transparent_black(),
                t.colors.danger.text,
                transparent_black(),
            ),
        };
        let primary = kind == ButtonVariant::Primary;
        let check = form
            .filter(|_| b.variant == ButtonVariant::Primary)
            .map(|f| f.fields.as_ref().clone());
        let (action, words, form_name) = (
            b.action.clone(),
            b.label.clone(),
            form.map(|f| f.name.clone()),
        );
        let label = div()
            .min_w_0()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_size(self.px(font))
            .line_height(self.px(font * t.text.label_leading))
            .text_color(fg)
            .child(b.label.clone());
        let go = self.cx.listener(move |this, _, window, cx| {
            this.press(
                &action,
                &words,
                form_name.as_deref(),
                check.clone(),
                window,
                cx,
            )
        });
        let press = pressable(self.id(path), self.t)
            .accessibility_label(b.label.clone())
            .flex_none()
            .max_w_full()
            .min_w_0()
            .py(self.px(vpad))
            .px(self.px(hpad))
            .rounded(self.px(radius))
            .bg(bg)
            .border_1()
            .border_color(edge)
            .hover(move |s| {
                if primary {
                    s.opacity(0.85)
                } else {
                    s.bg(t.colors.highlight)
                }
            })
            .debug_selector({
                let sel = self.selector(&format!("button:{}", b.label));
                move || sel
            })
            .child(label);
        let press = self.on_press(press, go);
        div()
            .id(self.id(&format!("{path}.wrap")))
            .flex_none()
            .max_w_full()
            .min_w_0()
            .flex()
            .child(press)
    }

    pub fn buttons(
        &mut self,
        bs: &Buttons,
        path: &str,
        form: Option<&FormRef>,
        _: Option<()>,
    ) -> AnyElement {
        let t = self.t;
        let mut row = div().w_full().min_w_0().flex().gap(self.px(t.space.m));
        row = if bs.direction == Direction::Column {
            row.flex_col().items_start()
        } else {
            row.flex_row().flex_wrap()
        };
        for (i, b) in bs.buttons.iter().enumerate() {
            row = row.child(self.button(b, &format!("{path}{i}"), form, None));
        }
        row.into_any_element()
    }

    pub fn icon_button(&mut self, b: &IconButton, path: &str) -> AnyElement {
        let t = self.t;
        let side = match b.size {
            Size::ExtraSmall | Size::Xs => 28.,
            Size::Small | Size::Sm | Size::S => 32.,
            Size::Large | Size::Lg | Size::L | Size::Xl => 40.,
            _ => 36.,
        };
        let (bg, fg, edge) = match b.variant {
            ButtonVariant::Primary => (t.colors.accent, t.colors.on_accent, t.colors.accent),
            ButtonVariant::Secondary => (transparent_black(), t.colors.text, t.colors.line_strong),
            _ => (transparent_black(), t.colors.text, transparent_black()),
        };
        let (action, words) = (b.action.clone(), b.name.clone());
        let go = self.cx.listener(move |this, _, window, cx| {
            this.press(&action, &words, None, None, window, cx)
        });
        let press = pressable(self.id(path), self.t)
            .accessibility_label(b.name.clone())
            .flex_none()
            .size(self.px(side))
            .bg(bg)
            .border_1()
            .border_color(edge)
            .when(b.circle, |p| p.rounded_full())
            .when(!b.circle, |p| p.rounded(self.px(t.radii.sm)))
            .hover(|s| s.opacity(0.85))
            .debug_selector({
                let sel = self.selector(&format!("button:{}", b.name));
                move || sel
            })
            .child(self.icon_element(&b.icon.name, 16., fg));
        let press = self.on_press(press, go);
        // Its name, for the people the icon says nothing to: on hover with a mouse, and with a
        // long press on touch (GPUI shows an element's tooltip for one).
        let name = b.name.clone();
        let (bg, text, border) = (t.colors.popover, t.colors.text, t.colors.line_strong);
        div()
            .id(self.id(&format!("{path}.tip")))
            .flex_none()
            .tooltip(move |window, cx| {
                Tooltip::new(name.clone())
                    .background(bg)
                    .text_color(text)
                    .border_color(border)
                    .build(window, cx)
            })
            .child(press)
            .into_any_element()
    }
}

/// Selected text's highlight in a text field: the accent at 30%, as gpui-core draws its own
/// (its ring at 30%), so the text stays readable over it on a light card or a dark one.
fn selection(t: &crate::theme::CardTheme) -> Hsla {
    t.colors.accent.opacity(0.3)
}

/// Whether a toggle is on: the stored map's entry, else its default.
fn toggled(value: Option<&Json>, it: &Toggle) -> bool {
    value
        .and_then(|v| v.get(&it.name))
        .and_then(Json::as_bool)
        .unwrap_or(it.checked)
}

/// The values chosen in a chips or option cards field.
fn chosen(value: Option<&Json>) -> Vec<String> {
    chosen_of(value)
}

fn chosen_of(value: Option<&Json>) -> Vec<String> {
    match value {
        Some(Json::Array(a)) => a
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect(),
        Some(Json::String(s)) if !s.is_empty() => vec![s.clone()],
        _ => Vec::new(),
    }
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use crate::theme::CardTheme;
    // The built-in #[test] (gpui_kit::test expands to it), not GPUI's, which gpui_kit::*
    // brings in.
    use std::prelude::v1::test;

    /// The accessibility node a choice row's press writes, as GPUI builds it for a screen
    /// reader. (GPUI's test window never turns its accessibility tree on, so the press is
    /// rendered and asked directly.)
    fn node(role: Role, checked: bool, cx: &mut TestAppContext) -> accesskit::Node {
        let vcx = cx.add_empty_window();
        vcx.update(|window, cx| {
            let t = CardTheme::dark();
            // A focus handle of its own: outside a frame the press can't keep one in the window.
            let focus = cx.focus_handle();
            let press = choice_press("row".into(), &t, role, "Email me", checked, false)
                .track_focus(&focus);
            let el = RenderOnce::render(press, window, cx).into_element();
            let role = Element::a11y_role(&el).expect("a choice row has a role");
            let mut node = accesskit::Node::new(role);
            Element::write_a11y_info(&el, &mut node);
            node
        })
    }

    #[gpui_kit::test]
    fn choice_rows_report_their_role_and_whether_they_are_checked(cx: &mut TestAppContext) {
        for role in [Role::CheckBox, Role::RadioButton, Role::Switch] {
            let on = node(role, true, cx);
            assert_eq!(on.role(), role);
            assert_eq!(on.label(), Some("Email me"));
            assert_eq!(
                on.toggled(),
                Some(accesskit::Toggled::True),
                "{role:?} checked"
            );
            let off = node(role, false, cx);
            assert_eq!(
                off.toggled(),
                Some(accesskit::Toggled::False),
                "{role:?} unchecked"
            );
        }
    }
}
