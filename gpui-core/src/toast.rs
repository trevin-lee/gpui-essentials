//! Toasts: a short note at the bottom of the window ("Saved.", "Copied the password.", an
//! error), as the web's are. They stack, newest at the bottom, and go by themselves.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::Toast;
//! # fn saved(window: &mut Window, cx: &mut App) {
//! Toast::success("Saved.").show(window, cx);
//! Toast::error("The policy didn't save: line 3 isn't a rule.")
//!     .action("Show it", |_, _| {})
//!     .show(window, cx);
//! # }
//! ```

use std::rc::Rc;
use std::time::Duration;

use gpui::{
    AnyElement, App, AppContext as _, Context, InteractiveElement as _, IntoElement,
    ParentElement as _, Render, Role, SharedString, StatefulInteractiveElement as _, Styled as _,
    WeakEntity, Window, div, prelude::FluentBuilder as _, rems,
};

use crate::base::{
    ActiveTheme as _, Icon, IconName, OVERLAY_PRIORITY, Press, Root, Sizable as _, Theme,
};
use crate::button::{Button, ButtonVariants as _};

/// How long a toast shows: long enough to read a line.
pub const TOAST_DURATION: Duration = Duration::from_secs(4);
/// An error's, or one with an action: long enough to read it and act.
pub const TOAST_DURATION_LONG: Duration = Duration::from_secs(8);
/// The most shown at once: the oldest go first.
const MOST: usize = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Error,
}

type OnAction = Rc<dyn Fn(&mut Window, &mut App)>;

/// A note to show: a line, optionally a second, and optionally an action.
#[derive(Clone)]
pub struct Toast {
    id: Option<SharedString>,
    kind: ToastKind,
    title: SharedString,
    description: Option<SharedString>,
    action: Option<(SharedString, OnAction)>,
    duration: Option<Duration>,
}

impl Toast {
    pub fn new(title: impl Into<SharedString>) -> Self {
        Self {
            id: None,
            kind: ToastKind::Info,
            title: title.into(),
            description: None,
            action: None,
            duration: None,
        }
    }

    /// With a tick: something worked.
    pub fn success(title: impl Into<SharedString>) -> Self {
        Self {
            kind: ToastKind::Success,
            ..Self::new(title)
        }
    }

    /// In the error colour, told to assistive tech at once (an alert), and shown longer.
    pub fn error(title: impl Into<SharedString>) -> Self {
        Self {
            kind: ToastKind::Error,
            ..Self::new(title)
        }
    }

    /// One shown with the same id is replaced rather than another added ("Copied" each time
    /// the button is pressed).
    pub fn id(mut self, id: impl Into<SharedString>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// A second line, muted.
    pub fn description(mut self, text: impl Into<SharedString>) -> Self {
        self.description = Some(text.into());
        self
    }

    /// A button in it ("Undo"): pressing it runs `f` and closes the toast.
    pub fn action(
        mut self,
        label: impl Into<SharedString>,
        f: impl Fn(&mut Window, &mut App) + 'static,
    ) -> Self {
        self.action = Some((label.into(), Rc::new(f)));
        self
    }

    /// How long it shows ([`TOAST_DURATION`] unless given; [`TOAST_DURATION_LONG`] for an error
    /// or one with an action).
    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    fn shown_for(&self) -> Duration {
        self.duration
            .unwrap_or(if self.kind == ToastKind::Error || self.action.is_some() {
                TOAST_DURATION_LONG
            } else {
                TOAST_DURATION
            })
    }

    /// Shows it at the bottom of the window. The window's root must be a [`Root`]; if it isn't,
    /// nothing shows.
    pub fn show(self, window: &mut Window, cx: &mut App) {
        let Some(Some(root)) = window.root::<Root>() else {
            return;
        };
        let after = self.shown_for();
        let stack = root.update(cx, |r, cx| {
            r.overlay::<Toasts>().unwrap_or_else(|| {
                let stack = cx.new(|_| Toasts::default());
                r.set_overlay(stack.clone(), cx);
                stack
            })
        });
        let seq = stack.update(cx, |s, cx| {
            s.next += 1;
            let seq = s.next;
            if let Some(id) = &self.id {
                s.shown.retain(|(_, t)| t.id.as_ref() != Some(id));
            }
            s.shown.push((seq, self));
            while s.shown.len() > MOST {
                s.shown.remove(0);
            }
            cx.notify();
            seq
        });
        // It goes by itself: one timer, the only thing that wakes the window.
        let stack = stack.downgrade();
        window
            .spawn(cx, async move |cx| {
                cx.background_executor().timer(after).await;
                let _ = stack.update(cx, |s, cx| dismiss(s, seq, cx));
            })
            .detach();
    }
}

/// A window's toasts: its [`Root`]'s overlay, set the first time one shows.
#[derive(Default)]
struct Toasts {
    /// The toasts shown, each with its place in the order they came.
    shown: Vec<(u64, Toast)>,
    next: u64,
}

impl Render for Toasts {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        stack(&self.shown, cx.entity().downgrade(), &theme)
            .unwrap_or_else(|| gpui::Empty.into_any_element())
    }
}

fn dismiss(toasts: &mut Toasts, seq: u64, cx: &mut Context<Toasts>) {
    let before = toasts.shown.len();
    toasts.shown.retain(|(s, _)| *s != seq);
    if toasts.shown.len() != before {
        cx.notify();
    }
}

/// The toasts shown, drawn at the bottom of the window over everything.
fn stack(toasts: &[(u64, Toast)], owner: WeakEntity<Toasts>, theme: &Theme) -> Option<AnyElement> {
    if toasts.is_empty() {
        return None;
    }
    let css = |v: f32| rems(v / 15.);
    let cards = toasts.iter().map(|(seq, t)| {
        let seq = *seq;
        let (icon, tint) = match t.kind {
            ToastKind::Info => (IconName::Info, theme.muted_foreground),
            ToastKind::Success => (IconName::CircleCheck, theme.foreground),
            ToastKind::Error => (IconName::CircleX, theme.destructive),
        };
        let close = owner.clone();
        let act = owner.clone();
        let selector = t.id.clone().unwrap_or_else(|| seq.to_string().into());
        div()
            .id(("gpui-core-toast", seq as usize))
            .role(if t.kind == ToastKind::Error {
                Role::Alert
            } else {
                Role::Status
            })
            .aria_label(match &t.description {
                Some(d) => SharedString::from(format!("{} {d}", t.title)),
                None => t.title.clone(),
            })
            .debug_selector(move || format!("toast-{selector}"))
            .occlude()
            .w(css(356.))
            .max_w_full()
            .flex()
            .items_start()
            .gap(css(10.))
            .p(css(14.))
            .rounded(theme.radius_lg())
            .border_1()
            .border_color(theme.border)
            .bg(theme.popover)
            .text_color(theme.popover_foreground)
            .text_size(theme.text.sm)
            .shadow_lg()
            .child(
                div()
                    .flex_none()
                    .pt(css(1.))
                    .child(Icon::new(icon).color(tint)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(css(2.))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .when(t.kind == ToastKind::Error, |d| {
                                d.text_color(theme.destructive)
                            })
                            .child(t.title.clone()),
                    )
                    .when_some(t.description.clone(), |d, text| {
                        d.child(div().text_color(theme.muted_foreground).child(text))
                    }),
            )
            .when_some(t.action.clone(), |d, (label, f)| {
                d.child(
                    Button::new(("gpui-core-toast-action", seq as usize))
                        .label(label)
                        .outline()
                        .small()
                        .on_click(move |_, window, cx| {
                            f(window, cx);
                            let _ = act.update(cx, |r, cx| dismiss(r, seq, cx));
                        }),
                )
            })
            .child(
                Press::new(("gpui-core-toast-close", seq as usize))
                    .accessibility_label("Close")
                    .debug_selector(move || format!("toast-close-{seq}"))
                    .flex_none()
                    .rounded(theme.radius_sm())
                    .child(Icon::new(IconName::X).color(theme.muted_foreground))
                    .on_click(move |_, _, cx| {
                        let _ = close.update(cx, |r, cx| dismiss(r, seq, cx));
                    }),
            )
    });
    Some(
        gpui::deferred(
            div()
                .absolute()
                .left_0()
                .right_0()
                .bottom(css(16.))
                .px(css(16.))
                .flex()
                .flex_col()
                .items_center()
                .gap(css(8.))
                .children(cards),
        )
        // Over popovers and sheets.
        .with_priority(OVERLAY_PRIORITY + 1)
        .into_any_element(),
    )
}
