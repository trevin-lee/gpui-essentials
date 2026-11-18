//! The time grid's scroll bar: a thin thumb at its right edge, as macOS draws an overlay
//! scroll bar. It shows for a second after the hours scroll, and wider while the pointer is
//! over its track or drags it; at rest it's hidden, so it never covers the events. Its thumb
//! drags the hours, and a press on its track pages them toward the press. On touch it shows
//! as the hours scroll, and isn't dragged.

use std::time::Duration;

use gpui::*;

use super::Calendar;
use crate::paint::{self, Geo, ScrollBar};
use crate::time;

/// How long the bar stays after the hours stop scrolling.
const SHOWN_FOR: Duration = Duration::from_millis(1000);

impl Calendar {
    /// Where the time grid's scroll bar thumb was drawn in the last frame, or None while it's
    /// hidden (it shows after a scroll, and while the pointer is on it).
    pub fn scroll_bar(&self) -> Option<Bounds<Pixels>> {
        self.shared.bar.get().and_then(|bar| bar.thumb)
    }

    /// Whether the bar is shown this frame, and whether wide (hovered or dragged).
    pub(super) fn scrollbar_shown(&self, cx: &App) -> Option<bool> {
        if !self.view.is_time_grid() {
            return None;
        }
        let wide = !self.options.touch && (self.bar_hover || self.bar_drag.is_some());
        let recent = self
            .bar_until
            .is_some_and(|until| cx.background_executor().now() < until);
        (wide || recent).then_some(wide)
    }

    /// The hours scrolled: the bar shows, and hides again once they've been still a while.
    pub(super) fn flash_scrollbar(&mut self, cx: &mut Context<Self>) {
        let executor = cx.background_executor().clone();
        self.bar_until = Some(executor.now() + SHOWN_FOR);
        self._bar_hide = Some(cx.spawn(async move |this, cx| {
            executor.timer(SHOWN_FOR).await;
            this.update(cx, |_, cx| cx.notify()).ok();
        }));
    }

    /// A press on the bar: on its thumb, a drag of it; elsewhere on its track, a page toward
    /// the press. Whether the bar took it (it takes presses only while it's drawn).
    pub(super) fn press_scrollbar(
        &mut self,
        p: Point<Pixels>,
        geo: &Geo,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.options.touch || geo.kind != paint::Kind::Grid {
            return false;
        }
        let Some(ScrollBar {
            track,
            thumb: Some(thumb),
        }) = self.shared.bar.get()
        else {
            return false;
        };
        if !track.contains(&p) {
            return false;
        }
        if thumb.contains(&p) {
            self.bar_drag = Some((p.y, self.scroll_minute()));
        } else {
            let page = geo.visible_minutes() * 0.9;
            let toward = if p.y < thumb.top() { -page } else { page };
            self.scroll_by(toward, geo);
            self.flash_scrollbar(cx);
        }
        cx.notify();
        true
    }

    /// The pointer moved: it drags the thumb, if one is held, and shows the bar while it's
    /// over its track. Whether the bar took the move.
    pub(super) fn move_scrollbar(
        &mut self,
        event: &MouseMoveEvent,
        geo: &Geo,
        cx: &mut Context<Self>,
    ) -> bool {
        if let Some((from_y, from_minute)) = self.bar_drag {
            // Let go outside the window.
            if event.pressed_button != Some(MouseButton::Left) {
                self.bar_drag = None;
                self.flash_scrollbar(cx);
                cx.notify();
                return true;
            }
            if let Some(ScrollBar {
                track,
                thumb: Some(thumb),
            }) = self.shared.bar.get()
            {
                let room = time::DAY_MINUTES - geo.visible_minutes();
                let travel = f32::from(track.size.height - thumb.size.height).max(1.);
                let moved = f32::from(event.position.y - from_y) / travel * room;
                self.shared
                    .scroll
                    .set(Some((from_minute + moved).clamp(0., room.max(0.))));
                cx.notify();
            }
            return true;
        }
        if self.options.touch || geo.kind != paint::Kind::Grid {
            return false;
        }
        let over = self
            .shared
            .bar
            .get()
            .is_some_and(|bar| bar.track.contains(&event.position));
        if over != self.bar_hover {
            self.bar_hover = over;
            if !over {
                self.flash_scrollbar(cx);
            }
            cx.notify();
        }
        false
    }
}
