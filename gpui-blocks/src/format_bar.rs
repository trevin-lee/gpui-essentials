//! The format bar: a text selection made with the pointer shows, above it, Turn into (the slash
//! menu's blocks, for the block the selection starts in), bold, italic, underline,
//! strikethrough, code and link, as Notion's does. A mark's button is pressed while the whole selection has it, and a
//! press toggles it over the selection; Link asks the app for the link, as ⌘K does
//! ([`EditorEvent::EditLinkRequested`](crate::EditorEvent::EditLinkRequested)).
//!
//! It shows once the selection is made (not while the pointer is still dragging it out), and
//! goes with the selection. On touch the marks and Link are a second row of the long press's
//! edit menu instead, whose Block… has Turn into.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::{ActiveTheme as _, Icon, IconName, Popover, PopupMenu, PopupMenuItem};
use markdown_editor::actions::EditLink;
use markdown_editor::{BlockSelection, EditorCommand, MarkKind, MarkState};

use crate::Editor;
use crate::block_menu::turn_into_titles;

/// The marks the bar toggles, with their labels.
const MARKS: [(MarkKind, &str); 5] = [
    (MarkKind::Bold, "Bold"),
    (MarkKind::Italic, "Italic"),
    (MarkKind::Underline, "Underline"),
    (MarkKind::Strikethrough, "Strikethrough"),
    (MarkKind::Code, "Code"),
];

impl Editor {
    /// Whether the bar shows: a text selection, made with the pointer and finished, in an
    /// editable document that has the keyboard, with no other menu open.
    fn format_bar_shows(&self, window: &Window, cx: &App) -> bool {
        let state = self.state.read(cx);
        let BlockSelection::Text { anchor, head } = state.model().selection() else {
            return false;
        };
        anchor != head
            && !state.is_readonly()
            && !state.is_selecting()
            && !state.is_composing()
            && !window.last_input_was_touch()
            // Turn into's menu has the keyboard while it's open.
            && (self.format_turn || state.focus_handle(cx).is_focused(window))
            && self.slash.is_none()
            && self.mention.is_none()
            && self.block_menu.is_none()
            && self.block_sheet.is_none()
    }

    /// Toggles `kind` over the selection.
    pub(crate) fn toggle_format(&mut self, kind: MarkKind, cx: &mut Context<Self>) {
        self.state.update(cx, |state, cx| {
            let _ = state.run(EditorCommand::ToggleMark(kind), cx);
        });
        cx.notify();
    }

    /// The marks' buttons and Link's, `size` square: the bar's, and the touch edit menu's row.
    pub(crate) fn format_buttons(&self, size: Pixels, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let theme = cx.theme().clone();
        let state = self.state.read(cx);
        let on = |kind| state.model().mark_state(kind) == MarkState::On;
        let pressed: Vec<bool> = MARKS.iter().map(|(kind, _)| on(*kind)).collect();
        let link_on = on(MarkKind::Link);
        let glyph = size * 0.5;
        let button = |label: &'static str, on: bool, content: AnyElement| {
            div()
                .id(SharedString::from(format!("blocks-format-{label}")))
                .debug_selector(move || format!("blocks-format-{label}"))
                .role(Role::Button)
                .aria_label(label)
                .aria_toggled(if on { Toggled::True } else { Toggled::False })
                .size(size)
                .flex_none()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .cursor_pointer()
                .hover(|s| s.bg(theme.accent))
                .when(on, |d| d.bg(theme.accent).text_color(theme.primary))
                .child(content)
        };
        let icon = |name: IconName, on: bool| {
            Icon::new(name)
                .size(glyph)
                .color(if on {
                    theme.primary
                } else {
                    theme.popover_foreground
                })
                .into_any_element()
        };
        let mut out: Vec<AnyElement> = MARKS
            .iter()
            .zip(pressed)
            .map(|(&(kind, label), on)| {
                let content = match kind {
                    // Lucide has no bold glyph: a bold B, as editors draw it.
                    MarkKind::Bold => div()
                        .text_size(glyph * 1.1)
                        .font_weight(FontWeight::BOLD)
                        .child("B")
                        .into_any_element(),
                    MarkKind::Italic => icon(IconName::Italic, on),
                    MarkKind::Underline => icon(IconName::Underline, on),
                    MarkKind::Strikethrough => icon(IconName::Strikethrough, on),
                    _ => icon(IconName::Code, on),
                };
                button(label, on, content)
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.toggle_format(kind, cx);
                        }),
                    )
                    .into_any_element()
            })
            .collect();
        out.push(
            button("Link", link_on, icon(IconName::Link, link_on))
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    cx.stop_propagation();
                    window.dispatch_action(Box::new(EditLink), cx);
                })
                .into_any_element(),
        );
        out
    }

    /// The bar above the selection, when it shows.
    pub(crate) fn format_bar(&self, window: &Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.format_bar_shows(window, cx) {
            return None;
        }
        let (first, last) = {
            let state = self.state.read(cx);
            let BlockSelection::Text { anchor, head } = *state.model().selection() else {
                return None;
            };
            let (a, h) = (state.position_bounds(anchor)?, state.position_bounds(head)?);
            if (a.top(), a.left()) <= (h.top(), h.left()) {
                (a, h)
            } else {
                (h, a)
            }
        };
        // Centred over the selection on one row, else over its start.
        let x = if (first.top() - last.top()).abs() < px(1.) {
            (first.left() + last.left()) / 2.
        } else {
            first.left()
        };
        let at = point(x, first.top() - px(8.));
        let block = self.selection_top_block(cx);
        let buttons = self.format_buttons(px(28.), cx);
        let theme = cx.theme().clone();
        let this = cx.entity().downgrade();
        let turn = block.map(|block| {
            let trigger = div()
                .id("blocks-format-turn")
                .debug_selector(|| "blocks-format-turn".into())
                .role(Role::Button)
                .aria_label("Turn into")
                .aria_expanded(self.format_turn)
                .h(px(28.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_sm()
                .whitespace_nowrap()
                .hover(|s| s.bg(theme.accent))
                .child("Turn into")
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size(px(14.))
                        .color(theme.muted_foreground),
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        this.format_turn = !this.format_turn;
                        cx.notify();
                    }),
                )
                .into_any_element();
            let pick = this.clone();
            let dismiss = this.clone();
            let menu = turn_into_titles().into_iter().enumerate().fold(
                PopupMenu::new("blocks-format-turn-menu"),
                |menu, (ix, title)| menu.item(PopupMenuItem::new(format!("turn-{ix}"), title)),
            );
            Popover::new("blocks-format-turn-popover")
                .open(self.format_turn)
                .on_dismiss(move |_, _, cx| {
                    let _ = dismiss.update(cx, |this, cx| {
                        this.format_turn = false;
                        cx.notify();
                    });
                })
                .trigger(trigger)
                .p(px(4.))
                .child(menu.on_select(move |picked, window, cx| {
                    let _ = pick.update(cx, |this, cx| {
                        this.format_turn = false;
                        this.block_menu_pick(block, &picked.id, window, cx);
                    });
                }))
                .into_any_element()
        });
        let divider = || {
            div()
                .w(px(1.))
                .h(px(18.))
                .mx(px(2.))
                .bg(theme.border)
                .into_any_element()
        };
        Some(
            deferred(
                anchored()
                    .position(at)
                    .anchor(Anchor::BottomCenter)
                    .snap_to_window_with_margin(px(8.))
                    .child(
                        div()
                            .id("blocks-format-bar")
                            .debug_selector(|| "blocks-format-bar".into())
                            .role(Role::Toolbar)
                            .aria_label("Format")
                            .flex()
                            .items_center()
                            .gap(px(2.))
                            .p(px(2.))
                            .rounded(px(8.))
                            .border_1()
                            .border_color(theme.border)
                            .bg(theme.popover)
                            .text_color(theme.popover_foreground)
                            .shadow_md()
                            // A press on the bar is the bar's, not the editor's selection.
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .when_some(turn, |bar, turn| bar.child(turn).child(divider()))
                            .children(buttons),
                    ),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}
