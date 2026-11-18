//! A columns block's width handles, as Notion's: while the pointer is over the block, a thin
//! bar sits at the left of each gap between its columns (the right of the gap holds the grips
//! of the next column's blocks); dragging one moves the line between the two columns
//! beside it, shown as the pointer goes, and letting go gives them their new widths as one undo
//! step. Columns stacked one under another (a narrow row) have none.

use gpui::*;
use gpui_core::ActiveTheme as _;
use markdown_editor::Control as _;
use markdown_editor::{BlockId, BlockKind, EditorCommand};

use crate::Editor;

/// The least share of the two columns' room either keeps.
const LEAST_SHARE: f32 = 0.15;

/// Each column of a columns block and where it was drawn.
type ColumnPlaces = Vec<(BlockId, Bounds<Pixels>)>;

/// A handle being dragged.
#[derive(Clone, Debug)]
pub(crate) struct ColumnDrag {
    columns: BlockId,
    /// The handle's place: between column `at` and the one after it.
    at: usize,
    /// Where the pointer is, in window coordinates.
    x: Pixels,
}

impl Editor {
    /// The columns block under the pointer, or whose handle is being dragged, and its columns'
    /// bounds, when they're side by side.
    fn handled_columns(&self, cx: &App) -> Option<(BlockId, ColumnPlaces)> {
        let columns = self
            .column_drag
            .as_ref()
            .map(|d| d.columns)
            .or(self.hovered)?;
        let state = self.state.read(cx);
        if state.is_readonly()
            || state.model().document().block(columns)?.kind() != BlockKind::Columns
        {
            return None;
        }
        let bounds = state.column_bounds(columns);
        let side_by_side = bounds.len() >= 2
            && bounds.windows(2).all(|w| {
                (w[0].1.top() - w[1].1.top()).abs() < px(4.) && w[1].1.left() > w[0].1.right()
            });
        side_by_side.then_some((columns, bounds))
    }

    /// The widths the columns get when the line between column `at` and the next is moved to
    /// `x`: those two share their room at `x`, and the others keep theirs.
    pub(crate) fn dragged_widths(
        &self,
        columns: BlockId,
        bounds: &[(BlockId, Bounds<Pixels>)],
        at: usize,
        x: Pixels,
        cx: &App,
    ) -> Option<Vec<f32>> {
        let state = self.state.read(cx);
        let block = state.model().document().block(columns)?;
        let mut widths: Vec<f32> = block
            .children()
            .map(|c| c.column_width().unwrap_or(1.))
            .collect();
        let (left, right) = (bounds.get(at)?.1, bounds.get(at + 1)?.1);
        let span = f32::from(right.right() - left.left());
        if span <= 0. || widths.len() != bounds.len() {
            return None;
        }
        let share = (f32::from(x - left.left()) / span).clamp(LEAST_SHARE, 1. - LEAST_SHARE);
        let pair = widths[at] + widths[at + 1];
        widths[at] = pair * share;
        widths[at + 1] = pair * (1. - share);
        Some(widths)
    }

    /// Lets go of a handle at `x`: the columns get their new widths.
    pub(crate) fn finish_column_drag(&mut self, cx: &mut Context<Self>) {
        let Some(drag) = self.column_drag.take() else {
            return;
        };
        let bounds = self.state.read(cx).column_bounds(drag.columns);
        if let Some(widths) = self.dragged_widths(drag.columns, &bounds, drag.at, drag.x, cx) {
            self.state.update(cx, |state, cx| {
                let _ = state.run(
                    EditorCommand::SetColumnWidths {
                        columns: drag.columns,
                        widths,
                    },
                    cx,
                );
            });
        }
        cx.notify();
    }

    /// The handles over the columns block under the pointer, and the pointer's watch while one
    /// is dragged.
    pub(crate) fn column_handles(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (columns, bounds) = self.handled_columns(cx)?;
        let theme = cx.theme().clone();
        let top = bounds.iter().map(|(_, b)| b.top()).reduce(Pixels::min)?;
        let bottom = bounds.iter().map(|(_, b)| b.bottom()).reduce(Pixels::max)?;
        let dragging = self.column_drag.as_ref().map(|d| (d.at, d.x));
        let handles = bounds.windows(2).enumerate().map(|(at, pair)| {
            // At the left of the gap: its right is the grip of the blocks in the next column.
            let x = match dragging {
                Some((dragged, x)) if dragged == at => x,
                _ => pair[0].1.right() + px(3.),
            };
            let active = dragging.is_some_and(|(dragged, _)| dragged == at);
            let this = cx.entity().downgrade();
            anchored()
                .position(point(x - px(4.), top))
                .child(
                    div()
                        .id(("blocks-column-handle", at))
                        .debug_selector(move || format!("blocks-column-handle-{at}"))
                        .control(
                            module_path!(),
                            Role::Splitter,
                            format!("Width of columns {} and {}", at + 1, at + 2),
                        )
                        .w(px(8.))
                        .h(bottom - top)
                        .flex()
                        .justify_center()
                        .cursor(CursorStyle::ResizeLeftRight)
                        .child(
                            div()
                                .w(px(if active { 3. } else { 2. }))
                                .h_full()
                                .rounded(px(1.))
                                .bg(if active { theme.primary } else { theme.border }),
                        )
                        .on_mouse_down(MouseButton::Left, move |event, _, cx| {
                            cx.stop_propagation();
                            let _ = this.update(cx, |this, cx| {
                                this.column_drag = Some(ColumnDrag {
                                    columns,
                                    at,
                                    x: event.position.x,
                                });
                                cx.notify();
                            });
                        }),
                )
                .into_any_element()
        });
        let watch = self.column_drag.is_some().then(|| {
            let this = cx.entity().downgrade();
            canvas(
                |_, _, _| {},
                move |_, _, window, _| {
                    let moving = this.clone();
                    window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = moving.update(cx, |this, cx| {
                                if let Some(drag) = this.column_drag.as_mut() {
                                    drag.x = event.position.x;
                                    cx.notify();
                                }
                            });
                        }
                    });
                    let up = this.clone();
                    window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                        if phase == DispatchPhase::Capture {
                            let _ = up.update(cx, |this, cx| this.finish_column_drag(cx));
                        }
                    });
                },
            )
            .size_0()
        });
        Some(
            deferred(div().children(handles).children(watch))
                .with_priority(1)
                .into_any_element(),
        )
    }
}
