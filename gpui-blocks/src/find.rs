//! Find and replace on the page: the bar ⌘F opens (Ctrl+F on Linux and Windows) over the page's
//! top right corner. The finding itself is the editor's (`BlockEditorState::find` and the rest).

use gpui::*;
use gpui_core::{
    ActiveTheme as _, Button, ButtonVariants as _, IconName, Input, InputEvent, InputState,
    Selectable as _, Sizable as _,
};
use markdown_editor::BlockSelection;

use crate::Editor;

/// The key context of the find bar.
pub(crate) const CONTEXT: &str = "BlocksFind";

/// The open find bar.
pub(crate) struct FindBar {
    query: Entity<InputState>,
    pub(crate) replace: Entity<InputState>,
    /// Whether the replace row shows.
    replacing: bool,
    case_sensitive: bool,
    _subscriptions: [Subscription; 2],
}

impl Editor {
    /// Opens the find bar, or focuses it when it's open, with the selected text as the query when
    /// it's on one line. `replacing` shows the replace row too.
    pub(crate) fn open_find(
        &mut self,
        replacing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let selected = self.selected_line(cx);
        if self.find.is_none() {
            let query = cx.new(|cx| InputState::new(window, cx).placeholder("Find"));
            let replace = cx.new(|cx| InputState::new(window, cx).placeholder("Replace with"));
            let on_query =
                cx.subscribe_in(
                    &query,
                    window,
                    |this, _, event: &InputEvent, _, cx| match event {
                        InputEvent::Change => this.run_find(cx),
                        InputEvent::PressEnter { secondary } => this.step_find(!secondary, cx),
                        _ => {}
                    },
                );
            let on_replace =
                cx.subscribe_in(&replace, window, |this, _, event: &InputEvent, _, cx| {
                    if let InputEvent::PressEnter { .. } = event {
                        this.replace_one(cx);
                    }
                });
            self.find = Some(FindBar {
                query,
                replace,
                replacing,
                case_sensitive: false,
                _subscriptions: [on_query, on_replace],
            });
        }
        let Some(bar) = &mut self.find else {
            return;
        };
        bar.replacing |= replacing;
        let query = bar.query.clone();
        if let Some(text) = selected {
            query.update(cx, |q, cx| q.set_value(text, window, cx));
        }
        query.update(cx, |q, cx| q.select_all(window, cx));
        query.update(cx, |q, cx| q.focus(window, cx));
        self.run_find(cx);
        cx.notify();
    }

    /// Closes the find bar: nothing is highlighted any more, and the page has the focus again.
    pub(crate) fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.take().is_none() {
            return;
        }
        self.state.update(cx, |state, cx| {
            state.clear_find(cx);
            state.focus(window, cx);
        });
        cx.notify();
    }

    /// The next match (`forward`) or the previous one.
    pub(crate) fn step_find(&mut self, forward: bool, cx: &mut Context<Self>) {
        if self.find.is_none() {
            return;
        }
        self.state.update(cx, |state, cx| {
            if forward {
                state.find_next(cx);
            } else {
                state.find_previous(cx);
            }
        });
        cx.notify();
    }

    /// Finds the query again, after it or the case changed.
    fn run_find(&mut self, cx: &mut Context<Self>) {
        let Some(bar) = &self.find else {
            return;
        };
        let (query, case_sensitive) = (bar.query.read(cx).value(), bar.case_sensitive);
        self.state
            .update(cx, |state, cx| state.find(&query, case_sensitive, cx));
        cx.notify();
    }

    fn replace_one(&mut self, cx: &mut Context<Self>) {
        let Some(bar) = &self.find else {
            return;
        };
        let with = bar.replace.read(cx).value();
        self.state
            .update(cx, |state, cx| state.replace_match(&with, cx));
        cx.notify();
    }

    fn replace_all(&mut self, cx: &mut Context<Self>) {
        let Some(bar) = &self.find else {
            return;
        };
        let with = bar.replace.read(cx).value();
        self.state
            .update(cx, |state, cx| state.replace_all_matches(&with, cx));
        cx.notify();
    }

    /// The selected text, when it's within one line of one block.
    fn selected_line(&self, cx: &App) -> Option<String> {
        let state = self.state.read(cx);
        let BlockSelection::Text { anchor, head } = state.model().selection().clone() else {
            return None;
        };
        if anchor.block != head.block || anchor.offset == head.offset {
            return None;
        }
        let text = state.model().document().block(anchor.block)?.text()?;
        let (from, to) = (
            anchor.offset.min(head.offset),
            anchor.offset.max(head.offset),
        );
        let line = text.get(from..to)?;
        (!line.contains(['\n', '\u{FFFC}'])).then(|| line.to_string())
    }

    /// The bar, pinned to the page's top right corner, when it's open.
    pub(crate) fn find_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let bar = self.find.as_ref()?;
        let theme = cx.theme().clone();
        let (matches, current) = {
            let (matches, current) = self.state.read(cx).find_matches();
            (matches.len(), current)
        };
        let count = match (matches, current) {
            (0, _) if bar.query.read(cx).value().is_empty() => String::new(),
            (0, _) => "No results".to_string(),
            (n, Some(i)) => format!("{} of {n}", i + 1),
            (n, None) => format!("{n}"),
        };
        let this = cx.entity().downgrade();
        let act = move |f: fn(&mut Editor, &mut Window, &mut Context<Editor>)| {
            let this = this.clone();
            move |_: &ClickEvent, window: &mut Window, cx: &mut App| {
                let _ = this.update(cx, |editor, cx| f(editor, window, cx));
            }
        };
        let icon = |id: &'static str, icon: IconName, label: &'static str| {
            Button::new(id)
                .icon(icon)
                .ghost()
                .small()
                .accessibility_label(label)
        };
        let find_row = div()
            .flex()
            .items_center()
            .gap_1()
            .child(div().w(px(220.)).child(Input::new(&bar.query)))
            .child(
                div()
                    .min_w(px(72.))
                    .pl_1()
                    .text_sm()
                    .text_color(theme.muted_foreground)
                    .child(count),
            )
            .child(
                icon("find-previous", IconName::ChevronUp, "Previous match")
                    .on_click(act(|e, _, cx| e.step_find(false, cx))),
            )
            .child(
                icon("find-next", IconName::ChevronDown, "Next match")
                    .on_click(act(|e, _, cx| e.step_find(true, cx))),
            )
            .child(
                icon("find-case", IconName::CaseSensitive, "Match case")
                    .selected(bar.case_sensitive)
                    .on_click(act(|e, _, cx| {
                        if let Some(bar) = &mut e.find {
                            bar.case_sensitive = !bar.case_sensitive;
                        }
                        e.run_find(cx);
                    })),
            )
            .child(
                icon("find-replace", IconName::Replace, "Replace")
                    .selected(bar.replacing)
                    .on_click(act(|e, _, cx| {
                        if let Some(bar) = &mut e.find {
                            bar.replacing = !bar.replacing;
                        }
                        cx.notify();
                    })),
            )
            .child(
                icon("find-close", IconName::X, "Close")
                    .on_click(act(|e, window, cx| e.close_find(window, cx))),
            );
        let replace_row = bar.replacing.then(|| {
            div()
                .flex()
                .items_center()
                .gap_1()
                .child(div().w(px(220.)).child(Input::new(&bar.replace)))
                .child(
                    Button::new("replace-one")
                        .label("Replace")
                        .outline()
                        .small()
                        .on_click(act(|e, _, cx| e.replace_one(cx))),
                )
                .child(
                    Button::new("replace-all")
                        .label("All")
                        .outline()
                        .small()
                        .on_click(act(|e, _, cx| e.replace_all(cx))),
                )
        });
        Some(
            div()
                .id("find-bar")
                .key_context(CONTEXT)
                .absolute()
                .top(px(12.))
                .right(px(16.))
                .p_2()
                .flex()
                .flex_col()
                .gap_2()
                .bg(theme.popover)
                .text_color(theme.popover_foreground)
                .border_1()
                .border_color(theme.border)
                .rounded(theme.radius_lg())
                .shadow_lg()
                .child(find_row)
                .children(replace_row)
                .into_any_element(),
        )
    }
}
