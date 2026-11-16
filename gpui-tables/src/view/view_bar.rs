//! Drawing the bar of views: the tabs (scrolling sideways when they don't fit), the + that adds
//! a view, a summary of the shown view's sort and filter, and its settings button; and the
//! views' menus in gpui-core's Popover, hanging from them.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, ClickEvent, Context, ElementId, FontWeight, Hsla,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Pixels, Point,
    Render, Role, SharedString, StatefulInteractiveElement, Styled, Window, canvas, div, px,
};
use gpui_core::{
    DismissReason, IconName, MenuSelect, Popover as CorePopover, PopoverColors, PopupMenuItem,
    Press, Selectable as _,
};

use super::render::{icon, line};
use super::table::{Popover, TableView};
use super::views::{VIEW_TYPES, ViewAction, ViewAnchor, ViewPage, view_icon};
use crate::model::ViewType;

/// What GPUI carries while a mouse drags a tab.
#[derive(Clone, Debug)]
pub(crate) struct DraggedTab {
    pub view_id: String,
}

/// The copy of a tab that follows the pointer while it's dragged.
pub(crate) struct TabGhost {
    label: SharedString,
    kind: ViewType,
    text: Hsla,
    muted: Hsla,
    fill: Hsla,
    border: Hsla,
    height: Pixels,
}

impl Render for TabGhost {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .h(self.height)
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(6.))
            .border_1()
            .border_color(self.border)
            .bg(self.fill)
            .text_color(self.text)
            .opacity(0.9)
            .child(icon(view_icon(&self.kind), gpui::rems(0.9), self.muted))
            .child(line(self.label.clone()).max_w(px(180.)))
    }
}

impl TableView {
    /// The table, board or list under the bar of views (when it's on), and the bar's menus.
    pub(crate) fn render_frame(
        &mut self,
        body: AnyElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        // The menus hang from where the bar's controls were drawn last frame, so they're laid
        // out before the bar forgets that.
        let popover = self.render_view_popover(window, cx);
        let bar = self.bar.on.then(|| self.render_bar(cx));
        div()
            .size_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .children(bar)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .w_full()
                    .flex()
                    .flex_col()
                    .child(body),
            )
            .child(popover)
            .into_any_element()
    }

    /// The bar's tab height in window pixels: 28 points, or the touch minimum.
    pub(crate) fn tab_height(&self) -> Pixels {
        self.theme.hit(28., self.rem)
    }

    fn render_bar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let t = self.theme.clone();
        let views = self.bar_views();
        let shown = self.view().map(|v| v.id.clone());
        for v in &views {
            if !self.bar.tabs.contains_key(&v.id) {
                self.bar.tabs.insert(v.id.clone(), cx.focus_handle());
            }
        }
        self.bar
            .tabs
            .retain(|id, _| views.iter().any(|v| v.id == *id));
        let add = self
            .bar
            .add
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        let settings = self
            .bar
            .settings
            .get_or_insert_with(|| cx.focus_handle())
            .clone();
        // The shown tab is scrolled into view when the view shown changes.
        if self.bar.scrolled_to != shown {
            if let Some(i) = views.iter().position(|v| Some(&v.id) == shown.as_ref()) {
                self.bar.scroll.scroll_to_item(i);
            }
            self.bar.scrolled_to = shown.clone();
        }
        let geometry = self.bar.geometry.clone();
        *geometry.borrow_mut() = Default::default();
        let h = self.tab_height();
        let icon_size = t.px(t.sizes.icon);
        let small = t.px(t.sizes.small);

        let tabs: Vec<AnyElement> = views
            .iter()
            .map(|v| {
                let on = shown.as_deref() == Some(v.id.as_str());
                let id = v.id.clone();
                let name = if v.name.trim().is_empty() {
                    "Untitled".to_owned()
                } else {
                    v.name.clone()
                };
                let selector = format!("view-tab:{name}");
                let g = geometry.clone();
                let probe_id = id.clone();
                let probe = canvas(
                    move |b, _, _| g.borrow_mut().tabs.push((probe_id, b)),
                    |_, _, _, _| {},
                )
                .absolute()
                .top_0()
                .left_0()
                .size_full();
                let ghost = TabGhost {
                    label: name.clone().into(),
                    kind: v.kind.clone(),
                    text: t.colors.text,
                    muted: t.colors.text_muted,
                    fill: t.colors.popover,
                    border: t.colors.popover_border,
                    height: h,
                };
                let ghost_label = ghost.label.clone();
                let ghost_kind = ghost.kind.clone();
                let (gt, gm, gf, gb) = (ghost.text, ghost.muted, ghost.fill, ghost.border);
                let click_id = id.clone();
                let menu_id = id.clone();
                let drop_id = id.clone();
                let handle = self.bar.tabs.get(&id).cloned();
                let mut tab = Press::new(ElementId::Name(format!("view-tab-{id}").into()))
                    .role(Role::Tab)
                    .selected(on)
                    .accessibility_label(name.clone())
                    .focus_on_press(false)
                    .focus_ring_color(t.colors.focus)
                    .debug_selector(move || selector)
                    .relative()
                    .flex_none()
                    .max_w(t.px(200.))
                    .h(h)
                    .px(t.px(8.))
                    .flex()
                    .items_center()
                    .gap(t.px(6.))
                    .rounded(t.px(t.sizes.chip_radius))
                    .text_color(if on {
                        t.colors.text
                    } else {
                        t.colors.text_muted
                    })
                    .when(on, |d| d.font_weight(FontWeight(t.sizes.medium)))
                    .hover(|s| s.bg(t.colors.hover))
                    .child(icon(
                        view_icon(&v.kind),
                        icon_size,
                        if on {
                            t.colors.text
                        } else {
                            t.colors.text_muted
                        },
                    ))
                    .child(line(name.clone()))
                    // The shown view's tab opens its menu: a chevron says so.
                    .when(on, |d| {
                        d.child(div().flex_none().child(icon(
                            IconName::ChevronDown,
                            small,
                            t.colors.text_muted,
                        )))
                    })
                    // The shown view's mark: a line under its tab.
                    .when(on, |d| {
                        d.child(
                            div()
                                .absolute()
                                .left(t.px(6.))
                                .right(t.px(6.))
                                .bottom(px(-1.))
                                .h(px(2.))
                                .rounded(px(1.))
                                .bg(t.colors.text),
                        )
                    })
                    .child(probe)
                    .on_click(cx.listener(move |this, e: &ClickEvent, window, cx| {
                        this.press_tab(click_id.clone(), e.is_keyboard(), window, cx)
                    }))
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_view_menu(&menu_id, super::ViewMenu::Menu, window, cx);
                        }),
                    )
                    .on_drag(
                        DraggedTab {
                            view_id: id.clone(),
                        },
                        move |_: &DraggedTab, _: Point<Pixels>, _, cx| {
                            cx.new(|_| TabGhost {
                                label: ghost_label.clone(),
                                kind: ghost_kind.clone(),
                                text: gt,
                                muted: gm,
                                fill: gf,
                                border: gb,
                                height: h,
                            })
                        },
                    )
                    .drag_over::<DraggedTab>(move |s, _, _, _| s.bg(t.colors.selection))
                    .on_drop(cx.listener(move |this, d: &DraggedTab, window, cx| {
                        this.drop_tab(&d.view_id, &drop_id, window, cx)
                    }));
                if let Some(handle) = handle {
                    tab = tab.track_focus(&handle);
                }
                tab.into_any_element()
            })
            .collect();

        let mut scroller = div()
            .id("gpui-tables-view-tabs")
            .debug_selector(|| "view-tabs".into())
            .flex_shrink_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .gap(t.px(2.))
            .overflow_x_scroll()
            .track_scroll(&self.bar.scroll)
            .children(tabs);
        scroller.interactivity().base_style.restrict_scroll_to_axis = Some(true);

        let square = |id: &'static str, label: &'static str, name: IconName| {
            Press::new(id)
                .accessibility_label(label)
                .focus_on_press(false)
                .focus_ring_color(t.colors.focus)
                .debug_selector(move || id.into())
                .relative()
                .flex_none()
                .size(h)
                .flex()
                .items_center()
                .justify_center()
                .rounded(t.px(t.sizes.chip_radius))
                .hover(|s| s.bg(t.colors.hover))
                .child(icon(name, icon_size, t.colors.text_muted))
        };
        let g = geometry.clone();
        let add_probe = canvas(move |b, _, _| g.borrow_mut().add = Some(b), |_, _, _, _| {})
            .absolute()
            .top_0()
            .left_0()
            .size_full();
        let add_button = square("add-view", "Add a view", IconName::Plus)
            .track_focus(&add)
            .child(add_probe)
            .on_click(
                cx.listener(|this, _: &ClickEvent, window, cx| this.open_add_view_menu(window, cx)),
            );
        let g = geometry.clone();
        let settings_probe = canvas(
            move |b, _, _| g.borrow_mut().settings = Some(b),
            |_, _, _, _| {},
        )
        .absolute()
        .top_0()
        .left_0()
        .size_full();
        let open_settings = |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
            if let Some(id) = this.view().map(|v| v.id.clone()) {
                this.remember_bar_focus(window, cx);
                this.open_view_page(id, ViewPage::Settings, ViewAnchor::Settings, window, cx);
            }
        };
        let settings_button = square(
            "view-settings",
            "View settings",
            IconName::SlidersHorizontal,
        )
        .track_focus(&settings)
        .child(settings_probe)
        .on_click(
            cx.listener(move |this, _: &ClickEvent, window, cx| open_settings(this, window, cx)),
        );

        // The shown view's sort and filter, in a few words: pressing one opens its settings.
        let summary = |id: &'static str, name: IconName, text: String, label: String| {
            Press::new(id)
                .accessibility_label(label)
                .focus_on_press(false)
                .focus_ring_color(t.colors.focus)
                .debug_selector(move || id.into())
                .flex_shrink_1()
                .min_w_0()
                .max_w(t.px(180.))
                .h(h)
                .px(t.px(6.))
                .flex()
                .items_center()
                .gap(t.px(4.))
                .rounded(t.px(t.sizes.chip_radius))
                .text_size(small)
                .text_color(t.colors.accent)
                .hover(|s| s.bg(t.colors.hover))
                .child(div().flex_none().child(icon(name, small, t.colors.accent)))
                .child(line(text))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    open_settings(this, window, cx)
                }))
        };
        let (sort, filter) = match self.shown.view.clone() {
            Some(v) => {
                let s = (!v.sort.is_empty()).then(|| {
                    let text = self.sort_summary(&v);
                    summary(
                        "view-sort",
                        IconName::ArrowUpDown,
                        text.clone(),
                        format!("Sorted by {text}"),
                    )
                });
                let f = v.filter.is_some().then(|| {
                    let text = self.filter_summary(&v);
                    summary(
                        "view-filter",
                        IconName::Funnel,
                        text.clone(),
                        format!("Filtered by {text}"),
                    )
                });
                (s, f)
            }
            None => (None, None),
        };

        div()
            .id("gpui-tables-view-bar")
            .debug_selector(|| "view-bar".into())
            .key_context(super::VIEW_BAR)
            .role(Role::TabList)
            .aria_label(SharedString::from(
                if self.database.title.trim().is_empty() {
                    "Views".to_owned()
                } else {
                    format!("{} views", self.database.title)
                },
            ))
            .on_action(
                cx.listener(|this, _: &super::Left, window, cx| this.bar_step(-1, window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &super::Right, window, cx| this.bar_step(1, window, cx)),
            )
            .on_action(cx.listener(|this, _: &super::Down, window, cx| this.leave_bar(window, cx)))
            .on_action(
                cx.listener(|this, _: &super::Cancel, window, cx| this.leave_bar(window, cx)),
            )
            .on_action(
                cx.listener(|this, _: &super::TabMenu, window, cx| this.bar_menu(window, cx)),
            )
            .flex_none()
            .w_full()
            .h(h + t.pixels(8., self.rem))
            .px(t.px(8.))
            .flex()
            .items_center()
            .gap(t.px(4.))
            .overflow_hidden()
            .border_b_1()
            .border_color(t.colors.line)
            .bg(t.colors.background)
            .text_color(t.colors.text)
            .font_family(t.fonts.body.clone())
            .text_size(t.px(t.sizes.text))
            .child(scroller)
            .child(add_button)
            .child(div().flex_1().min_w(px(0.)))
            .children(sort)
            .children(filter)
            .child(settings_button)
            .into_any_element()
    }

    /// A view's menu, the + menu or a new board's question, in gpui-core's Popover, hanging from
    /// its tab, the + or the settings button (or the view's top left without the bar). Drawn
    /// closed while none is open, so it hands the keyboard back when one closes.
    fn render_view_popover(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let back_to = self
            .bar
            .return_to
            .clone()
            .unwrap_or_else(|| self.focus.clone());
        let popover = CorePopover::new("gpui-tables-view-popover")
            .return_focus(&back_to)
            .close_when_scrolled_away(false)
            .colors(PopoverColors {
                background: t.colors.popover,
                foreground: t.colors.text,
                border: t.colors.popover_border,
            })
            .max_height(t.px(520.))
            .on_dismiss(cx.listener(|this, reason: &DismissReason, window, cx| {
                this.dismiss(*reason, window, cx)
            }));
        let Some(open) = self.popover.clone().filter(|p| p.is_view_menu()) else {
            return popover.into_any_element();
        };
        let geometry = self.bar.geometry.borrow().clone();
        let fallback = {
            let b = self.bounds.get();
            gpui::Bounds::new(b.origin, gpui::size(px(1.), px(1.)))
        };
        let target = match &open {
            Popover::View {
                view_id,
                anchor: ViewAnchor::Tab,
                ..
            } if self.bar.on => geometry
                .tabs
                .iter()
                .find(|(id, _)| id == view_id)
                .map(|(_, b)| *b),
            Popover::View {
                anchor: ViewAnchor::Settings,
                ..
            } if self.bar.on => geometry.settings,
            Popover::AddView | Popover::NewBoard if self.bar.on => geometry.add,
            _ => None,
        }
        .unwrap_or(fallback);
        let room = (window.viewport_size().width - px(26.)).max(px(0.));
        let width = t.pixels(t.sizes.menu_width, self.rem).min(room);
        let body = div()
            .debug_selector(|| "menu".into())
            .font_family(t.fonts.body.clone())
            .text_size(t.px(t.sizes.text))
            .text_color(t.colors.text);
        let popover = popover.open(true).anchor(target).match_trigger_width(false);
        match open {
            // A name is typed: the picker's field has the keyboard, and the popover leaves it
            // there.
            Popover::AddView => popover
                .take_focus(false)
                .accessibility_label("Add a view")
                .p(t.px(4.))
                .child(
                    body.w(width)
                        .children(self.render_note(cx))
                        .child(self.add_view_picker(cx)),
                )
                .into_any_element(),
            Popover::View { page, view_id, .. } if page.is_field() => popover
                .take_focus(false)
                .accessibility_label("Rename view")
                .p(t.px(4.))
                .child(body.w(width).child(self.render_view_field(&view_id, cx)))
                .into_any_element(),
            _ => {
                let label = match &open {
                    Popover::NewBoard => "New board".to_owned(),
                    _ => self
                        .menu_view()
                        .map_or_else(|| "View".to_owned(), |v| v.name.clone()),
                };
                popover
                    .accessibility_label(label)
                    .p(t.px(4.))
                    .child(
                        body.w(width)
                            .children(self.render_note(cx))
                            .child(self.popup_menu(cx)),
                    )
                    .into_any_element()
            }
        }
    }

    /// The + menu as gpui-core's Picker, naming rather than filtering: the new view's name typed
    /// in its field over its types. Up and Down move through the types while typing stays in
    /// the field; Enter makes the highlighted one, a press any, named as typed.
    fn add_view_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let full = self
            .database
            .database
            .as_ref()
            .is_some_and(|d| d.views.len() >= self.host.limits().views);
        let label = |k: &ViewType| super::ops::view_type_label(k).to_owned();
        let types = VIEW_TYPES.iter().map(|k| {
            PopupMenuItem::new(label(k), label(k))
                .icon(view_icon(k))
                .disabled(full)
        });
        let pick = cx.listener(|this, pick: &MenuSelect, window, cx| {
            if let Some(k) = VIEW_TYPES
                .iter()
                .find(|k| super::ops::view_type_label(k) == pick.id.as_ref())
            {
                let name = pick.query.as_deref().unwrap_or_default();
                this.add_view(k.clone(), name, window, cx);
            }
        });
        self.naming_picker("gpui-tables-add-view", "View name", types, pick)
    }

    /// A view's name, typed: the field, a note, and Save and Back for a pointer or a finger
    /// (Enter saves, Escape closes).
    fn render_view_field(&self, view_id: &str, cx: &mut Context<Self>) -> AnyElement {
        let t = &self.theme;
        let Some(f) = &self.field else {
            return div().into_any_element();
        };
        let id = view_id.to_owned();
        let save = cx.listener(move |this, pick: &MenuSelect, window, cx| {
            if pick.id.as_ref() == "Save" {
                this.rename_view(&id, window, cx);
            } else {
                this.activate_view(ViewAction::Back, window, cx);
            }
        });
        div()
            .flex()
            .flex_col()
            .gap(t.px(4.))
            .child(
                line("Rename view")
                    .px(t.px(8.))
                    .pt(t.px(4.))
                    .text_size(t.px(t.sizes.small))
                    .text_color(t.colors.text_muted),
            )
            .child(self.field_box(&f.state, "View name".into()))
            .children(self.render_note(cx))
            .child(
                self.small_menu("gpui-tables-view-field-menu")
                    .item(PopupMenuItem::new("Save", "Save").icon(IconName::Check))
                    .item(PopupMenuItem::new("Back", "Back").icon(IconName::ArrowLeft))
                    .on_select(save),
            )
            .into_any_element()
    }
}
