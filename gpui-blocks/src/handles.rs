//! The keyboard on the editor's handles, as ARIA's separators work: a columns block's width
//! handles take ← and → (5% of the row, 1% with Shift), an embed's height handle ↑ and ↓
//! (24 points, 8 with Shift), and Home and End go to the least and the most. A run of steps,
//! until the key is let go, is one undo step. The block menu's Resize puts the keyboard on
//! them, and Esc or Enter takes it back to the page.

use gpui::*;
use markdown_editor::{BlockId, BlockKind, EditorCommand};

use crate::Editor;
use crate::docs::{self, HandleKey, ResizeStart};

/// The key context of a focused column width handle.
pub(crate) const COLUMN_HANDLE: &str = "BlocksColumnHandle";

/// The key context of a focused embed height handle.
pub(crate) const EMBED_HANDLE: &str = "BlocksEmbedHandle";

/// What a handle does with a key.
pub(crate) type OnHandle = std::rc::Rc<dyn Fn(HandleKey, &mut Window, &mut App)>;

/// A handle's keys on `element`, in `context`: each a [`HandleKey`] for `on`, the steps `step`
/// and `fine` (with Shift); letting go of a key ends the run.
pub(crate) fn handle_keys<E: InteractiveElement>(
    element: E,
    context: &'static str,
    step: i32,
    fine: i32,
    on: OnHandle,
) -> E {
    use crate::actions::*;
    let key = |key: HandleKey| {
        let on = on.clone();
        move |window: &mut Window, cx: &mut App| on(key, window, cx)
    };
    let (less, more, less_fine, more_fine, least, most, leave, up) = (
        key(HandleKey::Step(-step)),
        key(HandleKey::Step(step)),
        key(HandleKey::Step(-fine)),
        key(HandleKey::Step(fine)),
        key(HandleKey::Least),
        key(HandleKey::Most),
        key(HandleKey::Leave),
        key(HandleKey::Released),
    );
    element
        .key_context(context)
        .on_action(move |_: &HandleLess, w, cx| less(w, cx))
        .on_action(move |_: &HandleMore, w, cx| more(w, cx))
        .on_action(move |_: &HandleLessFine, w, cx| less_fine(w, cx))
        .on_action(move |_: &HandleMoreFine, w, cx| more_fine(w, cx))
        .on_action(move |_: &HandleLeast, w, cx| least(w, cx))
        .on_action(move |_: &HandleMost, w, cx| most(w, cx))
        .on_action(move |_: &HandleLeave, w, cx| leave(w, cx))
        .on_key_up(move |_, w, cx| up(w, cx))
}

/// The least share of the two columns' room either keeps (as a drag leaves it).
const LEAST_SHARE: f32 = 0.15;

/// Steps made by keys since one went down, undone together into one: what they changed and
/// the document's revision after the last.
#[derive(Clone, Debug)]
pub(crate) struct KeyRun {
    target: RunTarget,
    revision: u64,
}

#[derive(Clone, Debug, PartialEq)]
enum RunTarget {
    /// An embed, by its block's lasting id, with its Markdown and height before the run.
    Embed {
        id: String,
        markdown: String,
        height: u32,
    },
    /// A columns block's handle, with the shares before the run.
    Columns {
        columns: BlockId,
        at: usize,
        widths: Vec<f32>,
    },
}

impl Editor {
    /// The focus of the width handle between column `at` and the next of `columns`.
    pub(crate) fn column_focus(&self, columns: BlockId, at: usize, cx: &mut App) -> FocusHandle {
        self.column_focus
            .borrow_mut()
            .entry((columns, at))
            .or_insert_with(|| cx.focus_handle())
            .clone()
    }

    /// Puts the keyboard on the width handles of `columns`, the first one first.
    pub(crate) fn resize_columns_by_key(
        &mut self,
        columns: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.column_keys = Some(columns);
        let focus = self.column_focus(columns, 0, cx);
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Puts the keyboard on the height handle of the embed `block`.
    pub(crate) fn resize_embed_by_key(
        &mut self,
        block: BlockId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = self.lock_ids().id_of(block);
        let focus = self
            .embed_focus
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .entry(id)
            .or_insert_with(|| cx.focus_handle())
            .clone();
        window.focus(&focus, cx);
        cx.notify();
    }

    /// Back to the page from a handle.
    fn leave_handle(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.key_run = None;
        self.column_keys = None;
        self.state.update(cx, |state, cx| state.focus(window, cx));
        cx.notify();
    }

    /// Undoes the run's last step when it's still the last edit, so the next step and it are
    /// one undo step. Whether a run goes on.
    fn continue_run(&mut self, target: &RunTarget, cx: &mut Context<Self>) -> bool {
        let Some(run) = &self.key_run else {
            return false;
        };
        let same = match (&run.target, target) {
            (RunTarget::Embed { id: a, .. }, RunTarget::Embed { id: b, .. }) => a == b,
            (
                RunTarget::Columns {
                    columns: a, at: x, ..
                },
                RunTarget::Columns {
                    columns: b, at: y, ..
                },
            ) => a == b && x == y,
            _ => false,
        };
        if !same || self.state.read(cx).revision() != run.revision {
            self.key_run = None;
            return false;
        }
        self.state.update(cx, |state, cx| state.undo(cx));
        true
    }

    /// A key on the width handle between column `at` and the next of `columns`.
    pub(crate) fn column_key(
        &mut self,
        columns: BlockId,
        at: usize,
        key: HandleKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let step = match key {
            HandleKey::Released => {
                self.key_run = None;
                return;
            }
            HandleKey::Leave => return self.leave_handle(window, cx),
            step => step,
        };
        if self.state.read(cx).is_readonly() {
            return;
        }
        self.column_keys = Some(columns);
        let now: Vec<f32> = {
            let state = self.state.read(cx);
            let Some(block) = state.model().document().block(columns) else {
                return;
            };
            if block.kind() != BlockKind::Columns {
                return;
            }
            block
                .children()
                .map(|c| c.column_width().unwrap_or(1.))
                .collect()
        };
        if at + 1 >= now.len() {
            return;
        }
        let pair = now[at] + now[at + 1];
        let left = match step {
            HandleKey::Step(by) => now[at] + by as f32 / 100.,
            HandleKey::Least => 0.,
            HandleKey::Most => pair,
            _ => return,
        }
        .clamp(pair * LEAST_SHARE, pair * (1. - LEAST_SHARE));
        let mut widths = now.clone();
        widths[at] = left;
        widths[at + 1] = pair - left;
        let target = RunTarget::Columns {
            columns,
            at,
            widths: now,
        };
        let target = if self.continue_run(&target, cx) {
            self.key_run.take().map_or(target, |run| run.target)
        } else {
            target
        };
        let done = self.state.update(cx, |state, cx| {
            let done = state
                .run(EditorCommand::SetColumnWidths { columns, widths }, cx)
                .is_ok();
            // The handle's own edit: the page stays where it is.
            state.keep_view();
            done
        });
        if done {
            self.key_run = Some(KeyRun {
                target,
                revision: self.state.read(cx).revision(),
            });
        }
        cx.notify();
    }

    /// A key on an embed's height handle: `start` is the embed as drawn.
    pub(crate) fn embed_key(
        &mut self,
        start: ResizeStart,
        key: HandleKey,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let step = match key {
            HandleKey::Released => {
                self.key_run = None;
                return;
            }
            HandleKey::Leave => return self.leave_handle(window, cx),
            step => step,
        };
        if self.state.read(cx).is_readonly() {
            return;
        }
        let (least, most) = (*docs::HEIGHTS.start(), *docs::HEIGHTS.end());
        let now = start.embed.height;
        let height = match step {
            HandleKey::Step(by) => (now as i64 + by as i64).clamp(least as i64, most as i64) as u32,
            HandleKey::Least => least,
            HandleKey::Most => most,
            _ => return,
        };
        if height == now && self.key_run.is_none() {
            return;
        }
        let Some(block) = self.embed_block_of(&start.markdown, cx) else {
            return;
        };
        let id = self.lock_ids().id_of(block);
        let target = RunTarget::Embed {
            id,
            markdown: start.markdown.clone(),
            height: now,
        };
        // Within a run, back to before it, then the run's steps as one edit.
        let (target, markdown_now) = if self.continue_run(&target, cx) {
            let target = self.key_run.take().map_or(target, |run| run.target);
            let before = match &target {
                RunTarget::Embed { markdown, .. } => markdown.clone(),
                _ => start.markdown.clone(),
            };
            (target, before)
        } else {
            (target, start.markdown.clone())
        };
        let markdown = docs::embed(&start.embed.doc, &start.title, &start.embed.view, height);
        let Some(block) = self.embed_block_of(&markdown_now, cx) else {
            return;
        };
        let ids = self.ids.clone();
        let done = self.state.update(cx, |state, cx| {
            let done = crate::editor::replace_keeping_id(state, &ids, block, markdown, cx);
            state.keep_view();
            done
        });
        if done {
            self.key_run = Some(KeyRun {
                target,
                revision: self.state.read(cx).revision(),
            });
        }
        cx.notify();
    }

    /// The custom block written as `markdown`.
    fn embed_block_of(&self, markdown: &str, cx: &App) -> Option<BlockId> {
        fn walk(block: markdown_editor::BlockRef<'_>, markdown: &str) -> Option<BlockId> {
            if block.kind() == BlockKind::Custom && block.markdown() == markdown {
                return Some(block.id());
            }
            block.children().find_map(|child| walk(child, markdown))
        }
        walk(self.state.read(cx).model().document().root(), markdown)
    }
}

#[cfg(test)]
mod tests {
    use gpui::{
        Entity, KeyDownEvent, KeyUpEvent, Keystroke, Modifiers, PlatformInput, TestAppContext,
        VisualTestContext,
    };
    use markdown_editor::BlockKind;
    use std::prelude::rust_2024::test;

    use crate::Editor;

    fn open<'a>(
        cx: &'a mut TestAppContext,
        source: &'static str,
    ) -> (Entity<Editor>, &'a mut VisualTestContext) {
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let (editor, cx) = cx.add_window_view(move |window, cx| Editor::new(source, window, cx));
        cx.update(|window, cx| editor.update(cx, |e, cx| e.focus(window, cx)));
        draw(cx);
        (editor, cx)
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }

    fn markdown(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> String {
        editor.update(cx, |e, cx| e.markdown(cx))
    }

    /// Keys held: each goes down, then all come up, as one run.
    fn run(cx: &mut VisualTestContext, keys: &[&'static str]) {
        for key in keys {
            let keystroke = Keystroke::parse(key).unwrap();
            cx.update(|window, cx| {
                window.dispatch_event(
                    PlatformInput::KeyDown(KeyDownEvent {
                        keystroke,
                        is_held: false,
                        prefer_character_input: false,
                    }),
                    cx,
                );
            });
            draw(cx);
        }
        let keystroke = Keystroke::parse(keys[keys.len() - 1]).unwrap();
        cx.update(|window, cx| {
            window.dispatch_event(PlatformInput::KeyUp(KeyUpEvent { keystroke }), cx);
        });
        draw(cx);
    }

    fn first(
        editor: &Entity<Editor>,
        cx: &mut VisualTestContext,
        kind: BlockKind,
    ) -> markdown_editor::BlockId {
        editor.update(cx, |e, cx| {
            let doc = e.state().read(cx).model().document();
            doc.root()
                .children()
                .find(|b| b.kind() == kind)
                .unwrap()
                .id()
        })
    }

    #[gpui::test]
    fn column_handles_take_the_arrows(cx: &mut TestAppContext) {
        let (editor, cx) = open(
            cx,
            "<!-- columns -->\n\n<!-- column 0.5 -->\n\nleft\n\n<!-- column 0.5 -->\n\nright\n\n<!-- /columns -->",
        );
        let columns = first(&editor, cx, BlockKind::Columns);
        editor.update_in(cx, |e, window, cx| {
            e.block_menu_pick(columns, "resize", window, cx)
        });
        draw(cx);
        // Two steps of 5%, held: one undo step.
        run(cx, &["right", "right"]);
        let md = markdown(&editor, cx);
        assert!(
            md.contains("<!-- column 0.6 -->") && md.contains("<!-- column 0.4 -->"),
            "{md}"
        );
        run(cx, &["shift-left"]);
        assert!(
            markdown(&editor, cx).contains("<!-- column 0.59 -->"),
            "{}",
            markdown(&editor, cx)
        );
        run(cx, &["home"]);
        assert!(
            markdown(&editor, cx).contains("<!-- column 0.15 -->"),
            "{}",
            markdown(&editor, cx)
        );
        run(cx, &["end"]);
        assert!(
            markdown(&editor, cx).contains("<!-- column 0.85 -->"),
            "{}",
            markdown(&editor, cx)
        );
        // Undo: back through the runs, the two held steps together.
        for _ in 0..3 {
            editor.update(cx, |e, cx| e.state().update(cx, |s, cx| s.undo(cx)));
        }
        assert!(
            markdown(&editor, cx).contains("<!-- column 0.6 -->"),
            "{}",
            markdown(&editor, cx)
        );
        editor.update(cx, |e, cx| e.state().update(cx, |s, cx| s.undo(cx)));
        assert!(
            markdown(&editor, cx).contains("<!-- column 0.5 -->"),
            "{}",
            markdown(&editor, cx)
        );
        // Esc: back to the page.
        run(cx, &["escape"]);
        assert!(editor.read_with(cx, |e, _| e.column_keys.is_none()));
    }

    #[gpui::test]
    fn an_embeds_height_handle_takes_the_arrows(cx: &mut TestAppContext) {
        let (editor, cx) = open(cx, "Intro\n\n[Roadmap](doc:r1 \"embed page 320\")");
        let embed = first(&editor, cx, BlockKind::Custom);
        editor.update_in(cx, |e, window, cx| {
            e.block_menu_pick(embed, "resize", window, cx)
        });
        draw(cx);
        run(cx, &["down", "down"]);
        assert!(
            markdown(&editor, cx).contains("\"embed page 368\""),
            "{}",
            markdown(&editor, cx)
        );
        run(cx, &["shift-up"]);
        assert!(
            markdown(&editor, cx).contains("\"embed page 360\""),
            "{}",
            markdown(&editor, cx)
        );
        run(cx, &["home"]);
        assert!(
            markdown(&editor, cx).contains("\"embed page 80\""),
            "{}",
            markdown(&editor, cx)
        );
        run(cx, &["end"]);
        assert!(
            markdown(&editor, cx).contains("\"embed page 1600\""),
            "{}",
            markdown(&editor, cx)
        );
        for _ in 0..3 {
            editor.update(cx, |e, cx| e.state().update(cx, |s, cx| s.undo(cx)));
        }
        assert!(
            markdown(&editor, cx).contains("\"embed page 368\""),
            "{}",
            markdown(&editor, cx)
        );
        editor.update(cx, |e, cx| e.state().update(cx, |s, cx| s.undo(cx)));
        assert!(
            markdown(&editor, cx).contains("\"embed page 320\""),
            "{}",
            markdown(&editor, cx)
        );
        let _ = Modifiers::none();
    }
}
