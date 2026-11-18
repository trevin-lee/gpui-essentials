//! The format bar: a text selection made with the pointer shows, above it, Turn into (the slash
//! menu's blocks, for each top-level block the selection runs through), bold, italic,
//! underline, strikethrough, code, link and Color, as Notion's does. A mark's button is pressed
//! while the whole selection has it, and a press toggles it over the selection; Link asks the
//! app for the link, as ⌘K does
//! ([`EditorEvent::EditLinkRequested`](crate::EditorEvent::EditLinkRequested)).
//!
//! It shows once the selection is made (not while the pointer is still dragging it out), and
//! goes with the selection. On touch the marks and Link are a second row of the long press's
//! edit menu instead, whose Block… has Turn into.
//!
//! With [`Options::comments`](crate::Options::comments), Comment comes first: it sends
//! [`EditorEvent::CommentRequested`](crate::EditorEvent::CommentRequested) with the
//! selection's anchor, for the app to start a thread on.
//!
//! Color gives the selection one of Notion's text colours ([`TEXT_COLORS`]), stored as
//! `<span style="color:#…">`, or background colours ([`BACKGROUNDS`]), stored as
//! `<mark color="#…">`, or takes either off; ⌘⇧H gives the last one picked again. A dark theme
//! draws each with a twin made for it.

use gpui::prelude::FluentBuilder as _;
use gpui::*;
use gpui_core::{ActiveTheme as _, Icon, IconName, Popover, PopupMenu, PopupMenuItem, Sheet};
use markdown_editor::actions::EditLink;
use markdown_editor::{
    BACKGROUNDS, BlockSelection, EditorCommand, MarkChange, MarkKind, MarkState, PaletteColor,
    TEXT_COLORS, background_of, text_color_of,
};

use crate::Editor;
use crate::block_menu::turn_into_titles;

/// A colour the Color menu gives: the text's own, or the fill behind it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Paint {
    Text(&'static PaletteColor),
    Background(&'static PaletteColor),
}

/// What the Color menu shows checked: the selection's text and background colours, and
/// whether it has none of each.
type Checked = (
    Option<&'static PaletteColor>,
    Option<&'static PaletteColor>,
    bool,
    bool,
);

/// The Color menu's entries, after `menu`'s: Default text and each text colour, then Default
/// background and each background colour, with `checked`'s checked. The format bar's menu and
/// the block menu's Color.
pub(crate) fn color_entries(
    menu: PopupMenu,
    theme: &gpui_core::Theme,
    checked: Option<Checked>,
) -> PopupMenu {
    let dark = theme.mode.is_dark();
    let swatch = |ink: Hsla, fill: Hsla| {
        div()
            .size(px(20.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .border_1()
            .border_color(theme.border)
            .bg(fill)
            .text_xs()
            .font_weight(FontWeight::MEDIUM)
            .text_color(ink)
            .child("A")
    };
    let check = |item: PopupMenuItem, on: bool| match checked {
        Some(_) => item.checked(on),
        None => item,
    };
    let (text, background, no_text, no_background) = checked.unwrap_or((None, None, false, false));
    let ink = theme.popover_foreground;
    let mut menu = menu.heading("Text color").item(check(
        PopupMenuItem::new("text-none", "Default text").chip(swatch(ink, transparent_black())),
        no_text,
    ));
    for (ix, color) in TEXT_COLORS.iter().enumerate() {
        menu = menu.item(check(
            PopupMenuItem::new(format!("text-{ix}"), format!("{} text", color.name))
                .chip(swatch(color.fill(dark), transparent_black())),
            text == Some(color),
        ));
    }
    menu = menu.heading("Background color").item(check(
        PopupMenuItem::new("background-none", "Default background")
            .chip(swatch(ink, transparent_black())),
        no_background,
    ));
    for (ix, color) in BACKGROUNDS.iter().enumerate() {
        menu = menu.item(check(
            PopupMenuItem::new(
                format!("background-{ix}"),
                format!("{} background", color.name),
            )
            .chip(swatch(ink, color.fill(dark))),
            background == Some(color),
        ));
    }
    menu
}

impl Paint {
    /// The colour a Color menu entry gives, and whether it gives it (`false`: its Default,
    /// which takes the colour of its kind off).
    pub(crate) fn of_menu_item(id: &str) -> Option<(Self, bool)> {
        let (kind, rest) = id.split_once('-')?;
        let (palette, paint): (&'static [PaletteColor], fn(&'static PaletteColor) -> Self) =
            match kind {
                "text" => (&TEXT_COLORS, Self::Text),
                "background" => (&BACKGROUNDS, Self::Background),
                _ => return None,
            };
        if rest == "none" {
            return Some((paint(&palette[0]), false));
        }
        let color = palette.get(rest.parse::<usize>().ok()?)?;
        Some((paint(color), true))
    }
}

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
            // Turn into's and Color's menus have the keyboard while they're open.
            && (self.format_turn
                || self.format_color
                || state.focus_handle(cx).is_focused(window))
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

    /// Gives the selection `paint`'s colour, or takes the colour of `paint`'s kind off when
    /// `on` is false.
    pub(crate) fn set_paint(&mut self, paint: Paint, on: bool, cx: &mut Context<Self>) {
        let change = match (paint, on) {
            (Paint::Text(color), true) => MarkChange::new().with_color(color.color()),
            (Paint::Text(_), false) => MarkChange::new().clear(MarkKind::Color),
            (Paint::Background(color), true) => MarkChange::new().with_highlight(color.color()),
            (Paint::Background(_), false) => MarkChange::new().clear(MarkKind::Highlight),
        };
        self.state.update(cx, |state, cx| {
            let _ = state.run(EditorCommand::ApplyMark(change), cx);
        });
        if on {
            self.last_paint = Some(paint);
        }
        cx.notify();
    }

    /// The palette colours all of the selection has: its text's and its background's.
    pub(crate) fn selection_paints(
        &self,
        cx: &App,
    ) -> (Option<&'static PaletteColor>, Option<&'static PaletteColor>) {
        let model = self.state.read(cx).model();
        (
            model.selection_color().and_then(text_color_of),
            model.selection_highlight().and_then(background_of),
        )
    }

    /// ⌘⇧H: the last colour picked again (a yellow background before one is), or off when the
    /// selection has it.
    pub(crate) fn highlight_again(&mut self, cx: &mut Context<Self>) {
        let paint = self
            .last_paint
            .unwrap_or(Paint::Background(&BACKGROUNDS[3]));
        let (text, background) = self.selection_paints(cx);
        let has = match paint {
            Paint::Text(color) => text == Some(color),
            Paint::Background(color) => background == Some(color),
        };
        self.set_paint(paint, !has, cx);
    }

    /// Runs the Color menu's entry `id` and closes the menu.
    pub(crate) fn color_menu_pick(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.format_color = false;
        if let Some((paint, on)) = Paint::of_menu_item(id) {
            self.set_paint(paint, on, cx);
        }
        self.state.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    /// The touch edit menu's Color: a sheet of the colours, for the selection. Drawn closed
    /// too, so the focus goes back to the page when it closes.
    pub(crate) fn color_sheet(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let menu = self.color_sheet.then(|| {
            let (text, background) = self.selection_paints(cx);
            let off = |kind| self.state.read(cx).model().mark_state(kind) == MarkState::Off;
            let checked = (
                text,
                background,
                off(MarkKind::Color),
                off(MarkKind::Highlight),
            );
            let this = cx.entity().downgrade();
            color_entries(
                PopupMenu::new("blocks-color-sheet-menu").touch(true),
                &theme,
                Some(checked),
            )
            .on_select(move |picked, window, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.color_sheet = false;
                    this.color_menu_pick(&picked.id, window, cx);
                });
            })
        });
        let dismiss = cx.entity().downgrade();
        Sheet::new("blocks-color-sheet")
            .open(self.color_sheet)
            .on_dismiss(move |_, _, cx| {
                let _ = dismiss.update(cx, |this, cx| {
                    this.color_sheet = false;
                    cx.notify();
                });
            })
            .accessibility_label("Color")
            .children(menu)
            .into_any_element()
    }

    /// The Color button and its menu, as Notion's: Default text and each text colour, then
    /// Default background and each background colour, the selection's own checked.
    fn color_menu(&self, cx: &mut Context<Self>) -> AnyElement {
        let theme = cx.theme().clone();
        let dark = theme.mode.is_dark();
        let (text, background) = self.selection_paints(cx);
        let off = |kind| self.state.read(cx).model().mark_state(kind) == MarkState::Off;
        let (no_text, no_background) = (off(MarkKind::Color), off(MarkKind::Highlight));
        let menu = color_entries(
            PopupMenu::new("blocks-format-color-menu"),
            &theme,
            Some((text, background, no_text, no_background)),
        );
        let trigger = div()
            .id("blocks-format-color")
            .debug_selector(|| "blocks-format-color".into())
            .role(Role::Button)
            .aria_label("Color")
            .aria_expanded(self.format_color)
            .h(px(28.))
            .px(px(6.))
            .flex()
            .items_center()
            .gap(px(2.))
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(theme.accent))
            .child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .px(px(3.))
                    .rounded(px(3.))
                    .map(|d| match self.last_paint {
                        Some(Paint::Text(color)) => d.text_color(color.fill(dark)),
                        Some(Paint::Background(color)) => d.bg(color.fill(dark)),
                        None => d,
                    })
                    .child("A"),
            )
            .child(
                Icon::new(IconName::ChevronDown)
                    .size(px(14.))
                    .color(theme.muted_foreground),
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.format_color = !this.format_color;
                    this.format_turn = false;
                    cx.notify();
                }),
            );
        let this = cx.entity().downgrade();
        let pick = this.clone();
        Popover::new("blocks-format-color-popover")
            .open(self.format_color)
            .on_dismiss(move |_, _, cx| {
                let _ = this.update(cx, |this, cx| {
                    this.format_color = false;
                    cx.notify();
                });
            })
            .trigger(trigger)
            .p(px(4.))
            .child(menu.on_select(move |picked, window, cx| {
                let _ = pick.update(cx, |this, cx| this.color_menu_pick(&picked.id, window, cx));
            }))
            .into_any_element()
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
        let color = self.color_menu(cx);
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
                        this.format_color = false;
                        cx.notify();
                    }),
                )
                .into_any_element();
            let pick = this.clone();
            let dismiss = this.clone();
            let menu = turn_into_titles().into_iter().enumerate().fold(
                PopupMenu::new("blocks-format-turn-menu"),
                |menu, (ix, title)| {
                    let keys = crate::slash::turn_into_keys(&title);
                    let item = PopupMenuItem::new(format!("turn-{ix}"), title);
                    menu.item(match keys {
                        Some(keys) => item.shortcut(keys),
                        None => item,
                    })
                },
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
        let comment = self.comments.then(|| {
            div()
                .id("blocks-format-comment")
                .debug_selector(|| "blocks-format-comment".into())
                .role(Role::Button)
                .aria_label("Comment")
                .h(px(28.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(6.))
                .cursor_pointer()
                .text_sm()
                .whitespace_nowrap()
                .hover(|s| s.bg(theme.accent))
                .child(
                    Icon::new(IconName::MessageSquare)
                        .size(px(14.))
                        .color(theme.popover_foreground),
                )
                .child("Comment")
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        cx.stop_propagation();
                        if let Some((block, quote)) = this.selection_anchor(cx) {
                            cx.emit(crate::EditorEvent::CommentRequested { block, quote });
                        }
                    }),
                )
                .into_any_element()
        });
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
                            .when_some(comment, |bar, comment| bar.child(comment).child(divider()))
                            .when_some(turn, |bar, turn| bar.child(turn).child(divider()))
                            .children(buttons)
                            .child(divider())
                            .child(color),
                    ),
            )
            .with_priority(3)
            .into_any_element(),
        )
    }
}
