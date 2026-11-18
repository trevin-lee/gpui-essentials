// Changed in Trevin Lee's fork (2026): block motion, the dragged copy, toggle chevron clicks,
// touch input; every block drawn on gpui-core's paragraph layer.
//! `BlockEditor` view (design §7).
//!
//! `render` draws the visible top-level blocks itself (`draw.rs`), on gpui-core's paragraph
//! layer, from the snapshot of the document's structure (`projection.rs`). Each text leaf reports
//! where it laid out while the [`EditorRoot`] element prepaints its children; the root then builds
//! the frame from those leaves (`frame.rs`) and paints the caret and the composition underline
//! from it. Paint never updates the editor entity.

use std::{cell::Cell, collections::HashMap, ops::Range, rc::Rc};

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Bounds, CursorStyle, DefiniteLength, DispatchPhase, Edges, Element, ElementId,
    ElementInputHandler, Entity, GlobalElementId, Hitbox, HitboxBehavior, Hsla, InspectorElementId,
    InteractiveElement, IntoElement, LayoutId, LongPressEvent, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement, Pixels, Refineable, RenderOnce, Role,
    StatefulInteractiveElement, Style, StyleRefinement, Styled, TextStyle, Window, canvas,
    deferred, div, fill, point, px, size,
};

use super::{cull::Measured, id::BlockId, projection::Snapshot, state::*, style::BlockEditorStyle};
use crate::support::secondary_modifier;

/// Items per cull unit of a long top-level list.
const LIST_CHUNK: usize = 32;

/// A block-based WYSIWYG Markdown editor (spike). Hold an
/// `Entity<BlockEditorState>` and pass it here in `render`. Instance styling
/// applies to the native input root; it installs no padding or height defaults.
#[derive(IntoElement)]
pub struct BlockEditor {
    state: Entity<BlockEditorState>,
    style: StyleRefinement,
}

impl BlockEditor {
    pub fn new(state: &Entity<BlockEditorState>) -> Self {
        Self {
            state: state.clone(),
            style: StyleRefinement::default(),
        }
    }
}

// Presentation is applied inside EditorRoot so padding is part of its
// native hitbox and input bounds, not an unfocusable outer wrapper.
impl Styled for BlockEditor {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

/// The accessible role (and heading level) of a top-level block (Phase 5,
/// per-block accessibility). `None` for blocks that are not exposed on
/// their own (definitions, breaks).
pub(crate) fn block_role(kind: &super::kind::BlockKindData) -> Option<(Role, Option<usize>)> {
    use super::kind::BlockKindData as K;
    Some(match kind {
        K::Paragraph => (Role::Paragraph, None),
        K::Heading { level } => (Role::Heading, Some(*level as usize)),
        K::Blockquote => (Role::Blockquote, None),
        K::List { .. } => (Role::List, None),
        K::CodeBlock { .. } => (Role::Code, None),
        K::Table { .. } => (Role::Table, None),
        K::HorizontalRule => (Role::Splitter, None),
        K::Custom => (Role::Group, None),
        K::ListItem { .. } => (Role::ListItem, None),
        _ => return None,
    })
}

impl RenderOnce for BlockEditor {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // The editor's look: gpui-core's theme, which the host sets.
        let look = gpui_core::Theme::global(cx).clone();
        // Pictures load through the host's loader when it gives one.
        let image_source = self.state.read(cx).image_source();
        let mut style = self.state.read(cx).style.resolved(&look);
        if style.selection_is_default() {
            style = style.with_selection(look.selection());
        }
        // Read the prior frame before request_layout can reset shared text
        // layouts. Manual scrolling must not become a reflow reveal request.
        let caret_was_visible = {
            let state = self.state.read(cx);
            state
                .scroll_handle
                .as_ref()
                .zip(state.model.caret())
                .is_some_and(|(handle, pos)| {
                    state
                        .frame
                        .borrow()
                        .caret_bounds(pos, style.caret_width())
                        .is_some_and(|caret| handle.bounds().contains(&caret.center()))
                })
        };
        let (focus, snapshot, backgrounds, keep, list_items) = self.state.update(cx, |state, _| {
            let snapshot = state.snapshot();
            let mut backgrounds: HashMap<u64, Vec<(Range<usize>, Hsla)>> = HashMap::new();
            // The host's tints first, then found text, so the selection draws over both.
            let tint = look.callout_warning.opacity(0.3);
            for (block, range) in state.tints() {
                let Some(len) = state.model.doc.leaf_len(*block) else {
                    continue;
                };
                let range = range.start.min(len)..range.end.min(len);
                if !range.is_empty() {
                    backgrounds
                        .entry(block.as_u64())
                        .or_default()
                        .push((range, tint));
                }
            }
            let found = look.attention.opacity(0.35);
            for m in state.find_matches().0 {
                backgrounds
                    .entry(m.block.as_u64())
                    .or_default()
                    .push((m.range.clone(), found));
            }
            for (block, range) in state.selection_ranges() {
                // Atoms paint whole (O(1) lookup, not a scan of the leaves).
                let range = match state.model.doc.get(block).map(|e| &e.content) {
                    Some(super::document::BlockContent::Atom(_)) => {
                        0..state.model.doc.leaf_len(block).unwrap_or(range.end)
                    }
                    _ => range,
                };
                backgrounds
                    .entry(block.as_u64())
                    .or_default()
                    .push((range, style.selection()));
            }
            (
                state.focus_handle.clone(),
                snapshot,
                backgrounds,
                state.pinned_blocks(),
                // Only needed to rebuild the cached cull units.
                (!state.cull.borrow().index.is_current(state.model.revision())).then(|| {
                    let doc = &state.model.doc;
                    doc.children(doc.root)
                        .iter()
                        .filter(|id| {
                            matches!(
                                doc.kind(**id),
                                Some(super::kind::BlockKindData::List { .. })
                            ) && doc.children(**id).len() > LIST_CHUNK
                        })
                        .map(|id| (*id, Rc::<[BlockId]>::from(doc.children(*id))))
                        .collect::<HashMap<_, _>>()
                }),
            )
        });

        // Match the native root's effective inherited/instance typography
        // before deriving body-relative node metrics and culling geometry.
        let mut typography = window.text_style();
        typography.refine(&self.style.text);
        let (cull, revision) = {
            let state = self.state.read(cx);
            (state.cull.clone(), state.model.revision())
        };
        // Blocks the engine draws itself (see `draw.rs`): their leaves, and how their text is set.
        let own_leaves: super::draw::OwnLeaves = Rc::default();
        let rem = window.rem_size();
        let body_size = typography.font_size.to_pixels(rem);
        let body_line = super::draw::device_line(typography.line_height_in_pixels(rem), window);
        let text_colors = super::draw::Colors {
            muted: look.muted_foreground,
            link: look.primary,
            code_background: look.muted,
            mono: look.mono_font_family.clone(),
            border: look.border,
        };
        // The gap below a block, as GPUI Kit's Markdown leaves.
        let paragraph_gap = gpui::rems(0.75).to_pixels(rem);
        // The theme's sizes are in rems of its own text size.
        let in_px = |r: gpui::Rems| look.font_size * r.0;
        let code_look = super::draw::CodeLook {
            size: in_px(look.text.sm),
            label_family: look.font_family.clone(),
            label_size: in_px(look.text.xs),
            refinement: {
                let radius: gpui::AbsoluteLength = in_px(look.radius_md()).into();
                let mut corners = gpui::StyleRefinement::default();
                corners.corner_radii.top_left = Some(radius);
                corners.corner_radii.top_right = Some(radius);
                corners.corner_radii.bottom_left = Some(radius);
                corners.corner_radii.bottom_right = Some(radius);
                corners
            },
        };
        // Code blocks are the engine's to draw when the host gave a highlighter; their
        // highlights are kept for blocks still in the document.
        let highlighting = {
            let state = self.state.read(cx);
            let cache = state.highlights.clone();
            cache
                .borrow_mut()
                .retain(|block, _| state.model.doc.get(*block).is_some());
            state
                .code_highlighter()
                .map(|highlighter| super::draw::Highlighting { highlighter, cache })
        };
        let own_extensions = self.state.read(cx).model.doc.ext.clone();
        let draw_cx = super::draw::DrawCx {
            base: &typography,
            body_size,
            body_line,
            paragraph_gap,
            colors: &text_colors,
            dark: look.mode.is_dark(),
            bands: &backgrounds,
            leaves: super::draw::LeafContext {
                extensions: &own_extensions,
                leaves: &own_leaves,
                images: image_source.as_ref(),
            },
            code: &code_look,
            radius: in_px(look.radius_md()),
            width: self
                .state
                .read(cx)
                .frame
                .borrow()
                .content
                .map(|c| c.size.width)
                .filter(|w| *w > px(0.)),
        };
        let mut children: Vec<AnyElement> = Vec::new();
        {
            let mut cull_state = cull.borrow_mut();
            cull_state.measured.clear();
            cull_state.drawn.clear();
            let window_span = cull_state.visible.map(|(top, bottom)| {
                // Overscan two viewports each way, so a fast scroll stays
                // within laid-out blocks until the next frame catches up.
                let margin = ((bottom - top) * 2.).max(px(600.));
                (top - margin, bottom + margin)
            });
            let cull_state = &mut *cull_state;
            if let Some(list_items) = list_items {
                // Cull units: each top-level node, except that a long
                // top-level list is split into chunks of `LIST_CHUNK` items,
                // each rendered as a list of its own (continuing the
                // numbering, no gap between chunks). Rebuilt per revision.
                let doc = &self.state.read(cx).model.doc;
                let mut units = Vec::with_capacity(snapshot.blocks.len());
                for (block_ix, block) in snapshot.blocks.iter().enumerate() {
                    let kind = doc.kind(*block);
                    let (look, role) = (
                        match kind {
                            Some(super::kind::BlockKindData::Heading { level }) => {
                                super::draw::Prev::Heading(*level)
                            }
                            Some(super::kind::BlockKindData::HorizontalRule) => {
                                super::draw::Prev::Rule
                            }
                            _ => super::draw::Prev::Other,
                        },
                        kind.and_then(block_role),
                    );
                    let unit = |id, items| super::cull::UnitDesc {
                        id,
                        block: block_ix,
                        items,
                        kind: look,
                        role,
                    };
                    // A long list's items, in chunks (`list_items` has only long lists).
                    match list_items.get(block) {
                        Some(ids) => {
                            for start in (0..ids.len()).step_by(LIST_CHUNK) {
                                let range = start..(start + LIST_CHUNK).min(ids.len());
                                units.push(unit(ids[start], Some((range, ids.clone()))));
                            }
                        }
                        None => units.push(unit(*block, None)),
                    }
                }
                cull_state.index.rebuild(
                    revision,
                    units.iter().map(|u| {
                        let covered: &[BlockId] = match &u.items {
                            Some((range, ids)) => &ids[range.clone()],
                            None => &[],
                        };
                        (u.id, covered)
                    }),
                    &cull_state.heights,
                );
                cull_state.units = units;
                cull_state.node_count = snapshot.blocks.len();
            }
            let units = &cull_state.units;
            let count = cull_state.node_count;
            let index = &cull_state.index;
            let n = units.len();
            // Where each top-level block lays out (a long list's chunks are one block), and how
            // the blocks changing places move from there (`reorder`).
            let mut slots: Vec<(BlockId, super::reorder::Slot)> = Vec::with_capacity(n);
            for (ix, unit) in units.iter().enumerate() {
                let id = snapshot.blocks[unit.block];
                let (top, height) = (f32::from(index.prefix(ix)), f32::from(index.height(ix)));
                match slots.last_mut() {
                    Some((last, slot)) if *last == id => slot.pitch += height,
                    _ => slots.push((id, super::reorder::Slot { top, pitch: height })),
                }
            }
            let reduce_motion = cx.reduce_motion();
            let (offsets, moving, held, by_touch) = self.state.update(cx, |state, _| {
                let top = state.content_top();
                let dragged = state.live_block_drag().filter(|d| d.active && !d.nested);
                let (offsets, moving) = state.reorder.frame(
                    slots,
                    dragged.map(|d| d.dragged(top)),
                    std::time::Instant::now(),
                    reduce_motion,
                );
                let held = dragged
                    .map(|d| state.dragged_blocks(&d))
                    .unwrap_or_default();
                (offsets, moving, held, dragged.is_some_and(|d| d.by_touch))
            });
            if moving {
                window.request_animation_frame();
            }
            let mut ghost_drawn = false;
            // Units to lay out: the visible range (binary search over the
            // prefix sums) plus pinned units (selection ends, IME).
            let (first, last) = match window_span {
                None => (0, n),
                Some((top, bottom)) => {
                    let first = index.find(top.max(px(0.))).min(n);
                    (first, (index.find(bottom) + 1).min(n).max(first))
                }
            };
            let mut wanted: Vec<usize> = keep
                .iter()
                .filter_map(|id| index.unit_of.get(id).copied())
                .filter(|ix| *ix < first || *ix >= last)
                .collect();
            wanted.extend(first..last);
            wanted.sort_unstable();
            wanted.dedup();
            let mut cursor = px(0.);
            let mut rendered = 0;
            for unit_ix in wanted {
                let unit = &units[unit_ix];
                let node_ix = unit.block;
                let top = index.prefix(unit_ix);
                if top > cursor {
                    children.push(div().h(top - cursor).into_any_element());
                }
                cursor = top + index.height(unit_ix);
                rendered += 1;
                let is_last_node = node_ix + 1 == count;
                let element = match &unit.items {
                    Some((range, ids)) => {
                        let last_chunk = range.end == ids.len();
                        let block_id = snapshot.blocks[unit.block];
                        let tree = super::draw::list_tree(
                            &self.state.read(cx).model.doc,
                            block_id,
                            Some(range.clone()),
                            highlighting.as_ref(),
                        );
                        match tree {
                            Some(tree) => super::draw::draw(
                                &super::draw::Tree::List(tree),
                                super::draw::Place {
                                    first: node_ix == 0 && range.start == 0,
                                    // Inner chunks drop the list's bottom gap.
                                    last: !last_chunk || is_last_node,
                                    prev: None,
                                    depth: 0,
                                },
                                None,
                                &draw_cx,
                                window,
                                cx,
                            ),
                            None => div().into_any_element(),
                        }
                    }
                    None => {
                        let block_id = snapshot.blocks[unit.block];
                        let prev = unit_ix.checked_sub(1).map(|ix| units[ix].kind);
                        let own = super::draw::tree(
                            &self.state.read(cx).model.doc,
                            block_id,
                            highlighting.as_ref(),
                        );
                        match own {
                            Some(tree) => super::draw::draw(
                                &tree,
                                super::draw::Place {
                                    first: node_ix == 0,
                                    last: is_last_node,
                                    prev,
                                    depth: 0,
                                },
                                None,
                                &draw_cx,
                                window,
                                cx,
                            ),
                            None => div().into_any_element(),
                        }
                    }
                };
                // Per-block accessibility node around the rendered block.
                let element = match unit.role {
                    Some((role, level)) => div()
                        .id(("block", unit.id.as_u64() as usize))
                        .role(role)
                        .when_some(level, |this, level| this.aria_level(level))
                        .child(element)
                        .into_any_element(),
                    None => element,
                };
                let block_id = snapshot.blocks[unit.block];
                let offset = offsets.get(&block_id).copied().unwrap_or(0.);
                let element = if let Some(held_ix) = held.iter().position(|id| *id == block_id) {
                    // Under the pointer: a copy drawn over every other block, while the block
                    // itself keeps its place and its geometry, unseen. The copy is painted
                    // after the editor's own geometry is collected, so it reports none. A run
                    // of blocks is one copy, drawn from the first of them laid out here.
                    let ghost = (!ghost_drawn).then(|| {
                        ghost_drawn = true;
                        // Without the margins it has among other blocks, and reporting no
                        // geometry and showing no selection: the block keeps both in its place.
                        let ghost_leaves: super::draw::OwnLeaves = Rc::default();
                        let no_bands = HashMap::new();
                        let ghost_cx = super::draw::DrawCx {
                            bands: &no_bands,
                            leaves: super::draw::LeafContext {
                                leaves: &ghost_leaves,
                                ..draw_cx.leaves
                            },
                            ..draw_cx
                        };
                        let trees: Vec<super::draw::Tree> = held[held_ix..]
                            .iter()
                            .filter_map(|id| {
                                super::draw::tree(
                                    &self.state.read(cx).model.doc,
                                    *id,
                                    highlighting.as_ref(),
                                )
                            })
                            .collect();
                        let copy = div().w_full().min_w_0().children(super::draw::children_of(
                            &trees, 0, None, &ghost_cx, window, cx,
                        ));
                        let theme = gpui_core::Theme::global(cx);
                        // Lifted by a finger, as iOS lifts things: a little bigger, and higher
                        // off the page.
                        let lift = if by_touch { px(4.) } else { px(0.) };
                        deferred(
                            div()
                                .id(("dragged-block", block_id.as_u64() as usize))
                                .absolute()
                                .top(px(offset) - lift)
                                .left(px(-12.) - lift)
                                .right(px(-12.) - lift)
                                .px(px(12.) + lift)
                                .py(px(6.) + lift)
                                .rounded(px(8.))
                                .bg(theme.background)
                                .border_1()
                                .border_color(theme.border)
                                .when(by_touch, |d| d.shadow_xl())
                                .when(!by_touch, |d| d.shadow_lg())
                                .child(copy),
                        )
                        .with_priority(1)
                    });
                    div()
                        .relative()
                        .child(div().opacity(0.).child(element))
                        .children(ghost)
                        .into_any_element()
                } else if offset != 0. {
                    div()
                        .relative()
                        .top(px(offset))
                        .child(element)
                        .into_any_element()
                } else {
                    element
                };
                children.push(
                    Measured {
                        id: unit.id,
                        index: unit_ix,
                        state: cull.clone(),
                        child: Some(element),
                    }
                    .into_any_element(),
                );
            }
            let total = index.prefix(n);
            if total > cursor {
                children.push(div().h(total - cursor).into_any_element());
            }
            cull_state.rendered = rendered;
        }

        let content_bounds = Rc::new(Cell::new(Bounds::default()));
        let measured_content = content_bounds.clone();
        let mut root_style = Style::default();
        root_style.refine(&self.style);
        let s = &self.state;
        div()
            .id("block-editor")
            // D8: one multi-line text input over the document; the value is
            // built only while accessibility is active (cached per revision).
            .role(Role::MultilineTextInput)
            .when(window.is_a11y_active() || cfg!(test), |this| {
                this.aria_value(self.state.read(cx).accessible_value())
            })
            .key_context(self.state.read(cx).key_context())
            .track_focus(&focus)
            .cursor(if self.state.read(cx).grip_hover {
                CursorStyle::OpenHand
            } else if self.state.read(cx).pointer_hand {
                CursorStyle::PointingHand
            } else {
                CursorStyle::IBeam
            })
            .w_full()
            .on_action(window.listener_for(s, BlockEditorState::on_backspace))
            .on_action(window.listener_for(s, BlockEditorState::on_delete))
            .on_action(window.listener_for(s, BlockEditorState::on_delete_word_left))
            .on_action(window.listener_for(s, BlockEditorState::on_delete_word_right))
            .on_action(window.listener_for(s, BlockEditorState::on_delete_to_line_start))
            .on_action(window.listener_for(s, BlockEditorState::on_delete_to_line_end))
            .on_action(window.listener_for(s, BlockEditorState::on_move_paragraph_start))
            .on_action(window.listener_for(s, BlockEditorState::on_move_paragraph_end))
            .on_action(window.listener_for(s, BlockEditorState::on_select_paragraph_start))
            .on_action(window.listener_for(s, BlockEditorState::on_select_paragraph_end))
            .on_action(window.listener_for(s, BlockEditorState::on_enter))
            .on_action(window.listener_for(s, BlockEditorState::on_structural_enter))
            .on_action(window.listener_for(s, BlockEditorState::on_soft_break))
            .on_action(window.listener_for(s, BlockEditorState::on_exit_block))
            .on_action(window.listener_for(s, BlockEditorState::on_move_left))
            .on_action(window.listener_for(s, BlockEditorState::on_move_right))
            .on_action(window.listener_for(s, BlockEditorState::on_move_up))
            .on_action(window.listener_for(s, BlockEditorState::on_move_down))
            .on_action(window.listener_for(s, BlockEditorState::on_move_home))
            .on_action(window.listener_for(s, BlockEditorState::on_move_end))
            .on_action(window.listener_for(s, BlockEditorState::on_select_left))
            .on_action(window.listener_for(s, BlockEditorState::on_move_word_left))
            .on_action(window.listener_for(s, BlockEditorState::on_move_word_right))
            .on_action(window.listener_for(s, BlockEditorState::on_select_word_left))
            .on_action(window.listener_for(s, BlockEditorState::on_select_word_right))
            .on_action(window.listener_for(s, BlockEditorState::on_select_right))
            .on_action(window.listener_for(s, BlockEditorState::on_select_up))
            .on_action(window.listener_for(s, BlockEditorState::on_select_down))
            .on_action(window.listener_for(s, BlockEditorState::on_select_home))
            .on_action(window.listener_for(s, BlockEditorState::on_select_end))
            .on_action(window.listener_for(s, BlockEditorState::on_move_doc_start))
            .on_action(window.listener_for(s, BlockEditorState::on_move_doc_end))
            .on_action(window.listener_for(s, BlockEditorState::on_select_doc_start))
            .on_action(window.listener_for(s, BlockEditorState::on_select_doc_end))
            .on_action(window.listener_for(s, BlockEditorState::on_select_all))
            .on_action(window.listener_for(s, BlockEditorState::on_undo))
            .on_action(window.listener_for(s, BlockEditorState::on_redo))
            .on_action(window.listener_for(s, BlockEditorState::on_normal_text))
            .on_action(window.listener_for(s, BlockEditorState::on_heading_1))
            .on_action(window.listener_for(s, BlockEditorState::on_heading_2))
            .on_action(window.listener_for(s, BlockEditorState::on_heading_3))
            .on_action(window.listener_for(s, BlockEditorState::on_todo_list))
            .on_action(window.listener_for(s, BlockEditorState::on_bulleted_list))
            .on_action(window.listener_for(s, BlockEditorState::on_numbered_list))
            .on_action(window.listener_for(s, BlockEditorState::on_toggle_block))
            .on_action(window.listener_for(s, BlockEditorState::on_code_block))
            .on_action(window.listener_for(s, BlockEditorState::on_table_insert_column_left))
            .on_action(window.listener_for(s, BlockEditorState::on_table_insert_column_right))
            .on_action(window.listener_for(s, BlockEditorState::on_table_insert_row_above))
            .on_action(window.listener_for(s, BlockEditorState::on_table_insert_row_below))
            .on_action(window.listener_for(s, BlockEditorState::on_table_delete_line))
            .on_action(window.listener_for(s, BlockEditorState::on_close_table_menu))
            .on_action(window.listener_for(s, BlockEditorState::on_toggle_bold))
            .on_action(window.listener_for(s, BlockEditorState::on_toggle_italic))
            .on_action(window.listener_for(s, BlockEditorState::on_edit_link))
            .on_action(window.listener_for(s, BlockEditorState::on_toggle_code))
            .on_action(window.listener_for(s, BlockEditorState::on_toggle_underline))
            .on_action(window.listener_for(s, BlockEditorState::on_toggle_strikethrough))
            .on_action(window.listener_for(s, BlockEditorState::on_indent))
            .on_action(window.listener_for(s, BlockEditorState::on_outdent))
            .on_action(window.listener_for(s, BlockEditorState::on_copy))
            .on_action(window.listener_for(s, BlockEditorState::on_cut))
            .on_action(window.listener_for(s, BlockEditorState::on_paste))
            .on_action(window.listener_for(s, BlockEditorState::on_paste_plain))
            .on_action(window.listener_for(s, BlockEditorState::on_duplicate))
            .children(super::table_ui::render_menu(&self.state, cx))
            .child(EditorRoot {
                state: self.state.clone(),
                child: Some(
                    {
                        let mut root = div().w_full();
                        root.style().refine(&self.style);
                        root
                    }
                    .child(
                        div().relative().w_full().children(children).child(
                            canvas(
                                move |bounds, _, _| measured_content.set(bounds),
                                |_, _, _, _| {},
                            )
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full(),
                        ),
                    )
                    .into_any_element(),
                ),
                content_bounds,
                padding: root_style.padding,
                typography,
                caret_was_visible,
                fresh: true,
                own_leaves,
                snapshot,
                style,
            })
    }
}

struct EditorRoot {
    state: Entity<BlockEditorState>,
    child: Option<AnyElement>,
    /// Whether this element's prepaint hasn't built the frame yet.
    fresh: bool,
    snapshot: Rc<Snapshot>,
    style: BlockEditorStyle,
    content_bounds: Rc<Cell<Bounds<Pixels>>>,
    padding: Edges<DefiniteLength>,
    typography: TextStyle,
    caret_was_visible: bool,
    /// The leaves the engine drew itself this frame.
    own_leaves: super::draw::OwnLeaves,
}

impl IntoElement for EditorRoot {
    type Element = Self;
    fn into_element(self) -> Self::Element {
        self
    }
}

pub(crate) struct Prepaint {
    hitbox: Hitbox,
}

impl EditorRoot {
    /// Scroll the caret into view in the owner's scroll container, once its
    /// block has been laid out (a culled block is pinned, so it is laid out
    /// in the frame after the caret moved there).
    fn reveal_caret(&self, bounds: Bounds<Pixels>, window: &mut Window, cx: &mut App) {
        let state = self.state.read(cx);
        if !state.reveal.get() {
            return;
        }
        let (Some(handle), Some(pos)) = (state.scroll_handle.clone(), state.model.caret()) else {
            state.reveal.set(false);
            return;
        };
        let Some(caret) = state
            .frame
            .borrow()
            .caret_bounds(pos, self.style.caret_width())
        else {
            return;
        };
        state.reveal.set(false);
        // The scroll owner stays at the region edge; text and the revealed
        // caret stay inside the root's presentation inset at either scroll end.
        let padding = self
            .padding
            .to_pixels(bounds.size.map(Into::into), window.rem_size());
        let mut view = handle.bounds();
        view.origin.y += padding.top;
        view.size.height = (view.size.height - padding.top - padding.bottom).max(Pixels::ZERO);
        let mut offset = handle.offset();
        if caret.top() < view.top() {
            offset.y += view.top() - caret.top();
        } else if caret.bottom() > view.bottom() {
            offset.y -= caret.bottom() - view.bottom();
        } else {
            return;
        }
        handle.set_offset(offset);
        window.refresh();
    }
}

impl Element for EditorRoot {
    type RequestLayoutState = AnyElement;
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, AnyElement) {
        let mut child = self
            .child
            .take()
            .unwrap_or_else(|| div().into_any_element());
        let layout_id = child.request_layout(window, cx);
        (layout_id, child)
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut AnyElement,
        window: &mut Window,
        cx: &mut App,
    ) -> Prepaint {
        if std::mem::take(&mut self.fresh) {
            child.prepaint(window, cx);
            // Swap in the complete frame; handlers read it until the next one.
            let own = std::mem::take(&mut *self.own_leaves.borrow_mut());
            let mut frame =
                super::frame::into_frame(own, self.snapshot.leaves.clone(), &self.snapshot.tables);
            frame.content = Some(self.content_bounds.get());
            frame.rem_size = window.rem_size();
            frame.units = std::mem::take(&mut self.state.read(cx).cull.borrow_mut().drawn);
            {
                let state = self.state.read(cx);
                let doc = &state.model.doc;
                let shapes: Vec<super::frame::TableShape> = self
                    .snapshot
                    .tables
                    .iter()
                    .map(|(table, _)| super::frame::TableShape {
                        table: *table,
                        grid: super::table::grid(doc, *table),
                    })
                    .collect();
                let pad = window.rem_size() * super::draw::CELL_PAD_REMS;
                frame.layout_tables(&shapes, pad);
            }
            // Scrolled during a drag: the dragged block follows the pointer in content y, so
            // draw again with the content's new top.
            let moved_top = {
                let state = self.state.read(cx);
                let before = state.frame.borrow().content.map(|c| c.top());
                state.block_drag.is_some_and(|d| d.active)
                    && before.is_some_and(|top| Some(top) != frame.content.map(|c| c.top()))
            };
            // A new width: rows of columns were drawn for the old one (whether they fit side by
            // side), so draw again.
            let resized = {
                let state = self.state.read(cx);
                let before = state.frame.borrow().content.map(|c| c.size.width);
                let doc = &state.model.doc;
                before != frame.content.map(|c| c.size.width)
                    && doc
                        .children(doc.root)
                        .iter()
                        .any(|c| doc.kind(*c) == Some(&super::kind::BlockKindData::Columns))
            };
            *self.state.read(cx).frame.borrow_mut() = frame;
            if moved_top || resized {
                let state = self.state.clone();
                window.on_next_frame(move |_, cx| state.update(cx, |_, cx| cx.notify()));
            }
            let mask = window.content_mask().bounds;
            let content = self.content_bounds.get();
            let cull = self.state.read(cx).cull.clone();
            let geometry_changed = cull.borrow_mut().set_geometry(
                content.size.width,
                window.rem_size(),
                &self.typography,
            );
            // Retain a visible caret or an existing reveal request, but do not
            // interrupt manual scrolling. Wait for fresh spacer positions.
            if geometry_changed {
                if self.caret_was_visible {
                    self.state.read(cx).reveal.set(true);
                }
            } else {
                self.reveal_caret(bounds, window, cx);
            }
            if geometry_changed || cull.borrow_mut().finish_frame(content, mask) {
                // Geometry changed or scrolling left the laid-out span.
                let state = self.state.clone();
                window.on_next_frame(move |_, cx| state.update(cx, |_, cx| cx.notify()));
            }
        } else {
            child.prepaint(window, cx);
        }
        Prepaint {
            hitbox: window.insert_hitbox(bounds, HitboxBehavior::Normal),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        child: &mut AnyElement,
        prepaint: &mut Prepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let state = self.state.read(cx);
        let focus = state.focus_handle.clone();
        let collapsed = state.selection_ranges().is_empty();
        let marked = state.marked_range();
        let caret = state.model.caret();
        let show_caret = state.show_caret(window, cx) && collapsed;
        let frame = state.frame.clone();
        let drop_line = state.drop_line();
        let menu_line = state.table_menu.as_ref().map(|m| (m.table, m.line));
        let grips = state.table_grips_at(window.mouse_position());
        // Whole-table corner handles to show: the hovered table's, and any
        // node-selected table's. `true` when the pointer is on the handle.
        let table_handles: Vec<(Bounds<Pixels>, bool)> = {
            let mouse = window.mouse_position();
            let on = state.table_handle_at(mouse);
            let selected: Vec<BlockId> = state
                .model
                .selected_blocks()
                .map(|(parent, range)| {
                    state
                        .model
                        .doc
                        .children(parent)
                        .get(range)
                        .unwrap_or_default()
                        .to_vec()
                })
                .unwrap_or_default();
            let frame = state.frame.borrow();
            state
                .table_handles()
                .into_iter()
                .filter(|(table, handle)| {
                    selected.contains(table)
                        || on == Some(*table)
                        || frame.grids.iter().any(|g| {
                            g.table == *table
                                && g.bounds
                                    .union(handle)
                                    .dilate(px(TABLE_HOVER_REACH))
                                    .contains(&mouse)
                        })
                })
                .map(|(table, handle)| (handle, on == Some(table) || selected.contains(&table)))
                .collect()
        };
        // Node selection (a table, code block, …): the blocks' laid-out area.
        // A dragged block's selection box would mark the place it left, which other blocks
        // slide over; the copy under the pointer shows it instead.
        let dragging = state.block_drag.is_some_and(|d| d.active);
        let selected_area = state
            .model
            .selected_blocks()
            .filter(|_| !dragging)
            .and_then(|(parent, range)| {
                let doc = &state.model.doc;
                let ids = doc.children(parent).get(range)?.to_vec();
                let frame = frame.borrow();
                frame
                    .leaves
                    .iter()
                    .filter(|leaf| {
                        let path = doc.path(leaf.info.block);
                        ids.iter()
                            .any(|id| *id == leaf.info.block || path.contains(id))
                    })
                    .map(|leaf| leaf.bounds())
                    .reduce(|a, b| a.union(&b))
            });

        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.state.clone()),
            cx,
        );
        child.paint(window, cx);

        let frame = frame.borrow();
        let style = &self.style;
        // Block handle: a grip in the left edge of the hovered table or code
        // block; clicking the edge node-selects it (`block_handle_at`).
        let mouse = window.mouse_position();
        // Tables use the corner handle below instead.
        let hovered = frame
            .leaves
            .iter()
            .filter(|l| state_plain(&self.state, l.info.block, cx))
            .map(|l| l.bounds())
            .find(|b| {
                mouse.y >= b.top()
                    && mouse.y < b.bottom()
                    && mouse.x >= b.left() - px(24.)
                    && mouse.x < b.right()
            });
        if let Some(b) = hovered {
            let grip = Bounds::new(
                point(b.left() - px(14.), b.top() + px(2.)),
                size(px(4.), px(16.).min(b.size.height)),
            );
            window.paint_quad(fill(grip, style.caret().opacity(0.35)).corner_radii(px(2.)));
        }
        // Whole-table handle: a small rounded square with a 2x2 grid glyph
        // at the frame's outer top-left corner, distinct from the thin
        // row / column grips.
        for (handle, active) in table_handles {
            let alpha = if active { 0.9 } else { 0.45 };
            window.paint_quad(gpui::quad(
                handle,
                px(3.),
                style.selection().opacity(if active { 0.45 } else { 0.0 }),
                px(1.),
                style.caret().opacity(alpha),
                gpui::BorderStyle::Solid,
            ));
            let dot = px(2.);
            let c = handle.center();
            for (dx, dy) in [(-2.5, -2.5), (0.5, -2.5), (-2.5, 0.5), (0.5, 0.5)] {
                window.paint_quad(fill(
                    Bounds::new(point(c.x + px(dx), c.y + px(dy)), size(dot, dot)),
                    style.caret().opacity(alpha),
                ));
            }
        }
        // Table grips (D20): only for the hovered column / row, or the
        // line whose menu is open.
        for (table, line, grip) in grips {
            let open = menu_line == Some((table, line));
            let alpha = if open { 0.8 } else { 0.35 };
            window.paint_quad(fill(grip, style.caret().opacity(alpha)).corner_radii(px(2.)));
        }
        if let Some(area) = selected_area {
            let area = area.dilate(px(4.));
            window.paint_quad(gpui::quad(
                area,
                px(4.),
                style.selection().opacity(0.35),
                px(2.),
                style.caret(),
                gpui::BorderStyle::Solid,
            ));
        }
        if let Some((block, range)) = marked
            && let Some(leaf) = frame.leaf(block)
        {
            let a = leaf.caret_point(range.start);
            let b = leaf.caret_point(range.end);
            let thickness = style.caret_width();
            let underline = Bounds::new(
                point(a.x, a.y + leaf.line_height() - thickness),
                size((b.x - a.x).max(thickness), thickness),
            );
            window.paint_quad(fill(underline, style.composition_underline()));
        }
        if let Some(line) = drop_line {
            window.paint_quad(fill(line, style.caret()).corner_radii(px(1.5)));
        }
        if show_caret
            && let Some(pos) = caret
            && let Some(b) = frame.caret_bounds(pos, style.caret_width())
        {
            // Whole device pixels, like the input caret: a 1px line at a
            // fractional x would blur over two pixels.
            let b = crate::support::snap_caret_to_device_pixels(b, window.scale_factor());
            window.paint_quad(fill(b, style.caret()));
        }
        drop(frame);

        // Mouse: handlers update the entity at event time, never in paint.
        let hitbox = prepaint.hitbox.clone();
        let entity = self.state.clone();
        window.on_mouse_event(move |event: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble
                || event.button != MouseButton::Left
                || !hitbox.is_hovered(window)
            {
                return;
            }
            // A table grip selects its row / column and opens the menu;
            // any other click closes an open menu.
            if !event.modifiers.shift
                && entity.update(cx, |state, cx| {
                    let hit = state.click_table_grip(event.position, cx);
                    if !hit {
                        state.close_table_menu(cx);
                    }
                    hit
                })
            {
                let focus = entity.read(cx).focus_handle.clone();
                window.focus(&focus, cx);
                cx.stop_propagation();
                return;
            }
            // A task checkbox or a toggle's chevron flips without moving the
            // caret or starting a drag; nothing else handles this mouse-down.
            if !event.modifiers.shift
                && entity.update(cx, |state, cx| {
                    state.click_checkbox(event.position, cx)
                        || state.click_toggle(event.position, cx)
                })
            {
                cx.stop_propagation();
                return;
            }
            entity.update(cx, |state, cx| {
                let focus = state.focus_handle.clone();
                window.focus(&focus, cx);
                // A tap asks for the on-screen keyboard; the platform only shows one in answer
                // to a gesture.
                if window.last_input_was_touch() && !state.is_readonly() {
                    window.request_virtual_keyboard();
                }
                if secondary_modifier().is_subset_of(&event.modifiers)
                    && state.follow_link_at(event.position, cx)
                {
                    return;
                }
                state.dragging = true;
                if !event.modifiers.shift {
                    state.arm_block_drag(event.position);
                }
                state.mouse_at(event.position, event.modifiers.shift, cx);
                if !event.modifiers.shift && event.click_count >= 2 {
                    state.select_unit(event.click_count >= 3, cx);
                }
            });
        });
        let entity = self.state.clone();
        window.on_mouse_event(move |event: &MouseMoveEvent, phase, _, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            if event.pressed_button.is_none() {
                entity.update(cx, |state, cx| {
                    state.hover_at(
                        event.position,
                        secondary_modifier().is_subset_of(&event.modifiers),
                        cx,
                    )
                });
                return;
            }
            if event.pressed_button != Some(MouseButton::Left) {
                return;
            }
            entity.update(cx, |state, cx| {
                if state.block_drag_to(event.position, cx) {
                    return;
                }
                if state.dragging {
                    state.mouse_at(event.position, true, cx);
                }
            });
        });
        // Touch: a long press selects the word under the finger, and moving before letting go
        // extends the selection; begun off the text (the gutter, a picture, past a line's end),
        // moving lifts the block instead (see `BlockEditorState::touch_moved`). Taps arrive as
        // mouse presses.
        let entity = self.state.clone();
        let hitbox = prepaint.hitbox.clone();
        window.on_mouse_event(move |event: &LongPressEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            match event.phase {
                gpui::TouchPhase::Started => {
                    if window.default_prevented() || !hitbox.bounds.contains(&event.start_position)
                    {
                        return;
                    }
                    window.prevent_default();
                    window.capture_long_press(&entity);
                    cx.stop_propagation();
                    entity.update(cx, |state, cx| {
                        let focus = state.focus_handle.clone();
                        window.focus(&focus, cx);
                        state.dragging = false;
                        state.touch_pressed(event.start_position);
                        state.mouse_at(event.start_position, false, cx);
                        state.select_unit(false, cx);
                    });
                }
                gpui::TouchPhase::Moved if window.has_long_press_capture(&entity) => {
                    cx.stop_propagation();
                    entity.update(cx, |state, cx| {
                        if !state.touch_moved(event.start_position, event.position, cx) {
                            state.mouse_at(event.position, true, cx)
                        }
                    });
                }
                gpui::TouchPhase::Ended | gpui::TouchPhase::Cancelled => {
                    let cancelled = event.phase == gpui::TouchPhase::Cancelled;
                    entity.update(cx, |state, cx| state.touch_ended(cancelled, cx));
                }
                _ => {}
            }
        });
        // A block lifted in place (the block sheet's Move) takes the next finger drag.
        let entity = self.state.clone();
        window.on_mouse_event(move |event: &gpui::TouchDragEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble {
                return;
            }
            let taken = entity.update(cx, |state, cx| {
                state.lifted_touch(event.phase, event.start_position, event.position, cx)
            });
            if taken {
                if event.phase == gpui::TouchPhase::Started {
                    window.prevent_default();
                }
                cx.stop_propagation();
            }
        });
        let entity = self.state.clone();
        window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
            if phase == DispatchPhase::Bubble {
                entity.update(cx, |state, cx| {
                    state.dragging = false;
                    state.finish_block_drag(cx);
                });
            }
        });
    }
}

/// Pointer reach around a table where its corner handle shows.
const TABLE_HOVER_REACH: f32 = 28.;

fn state_plain(state: &Entity<BlockEditorState>, block: BlockId, cx: &App) -> bool {
    state.read(cx).model.doc.plain(block).is_some()
}
