//! A one-time code field, as shadcn draws it (its `input-otp.tsx`): a row of single-character
//! slots, six by default, optionally in groups with a dash between. It's one hidden
//! [`InputState`] underneath, so the platform's text input (the iPhone's keyboard and its
//! suggested codes, input methods, dictation, the clipboard) works as in any field.
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{InputOtp, InputOtpState};
//! # struct SignIn { code: Entity<InputOtpState> }
//! # impl Render for SignIn {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     InputOtp::new(&self.code)
//!         .groups([3, 3])
//!         .on_complete(cx.listener(|this, code: &SharedString, _, cx| {
//!             // Check the code.
//!         }))
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, AppContext as _, Autocapitalize, Context, ElementId, Entity, EventEmitter, FocusHandle,
    Focusable, InteractiveElement as _, IntoElement, MouseButton, ParentElement as _, Rems,
    RenderOnce, SharedString, StyleRefinement, Styled, Subscription, TextInputAction,
    TextInputConfiguration, Window, div, prelude::FluentBuilder as _, rems,
};

use crate::base::{
    ActiveTheme as _, ControlSize, Icon, IconName, Input, InputEvent, InputState, Paste, Sizable,
    focus_ring,
};

fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// What a code field tells its host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputOtpEvent {
    /// The code changed: what's in the slots now.
    Change(SharedString),
    /// Every slot is filled: the code. Told once per code; editing it and filling it again
    /// tells it again.
    Complete(SharedString),
}

type OnComplete = Rc<dyn Fn(&SharedString, &mut Window, &mut App)>;

/// A code field's code and settings. Make it an entity, draw it with [`InputOtp`], and
/// subscribe to its [`InputOtpEvent`]s (or give the [`InputOtp`] an
/// [`on_complete`](InputOtp::on_complete)).
pub struct InputOtpState {
    input: Entity<InputState>,
    length: usize,
    digits_only: bool,
    /// The code as last told, and the full code last told complete.
    told: String,
    completed: Option<String>,
    on_complete: Option<OnComplete>,
    _subscription: Subscription,
}

impl EventEmitter<InputOtpEvent> for InputOtpState {}

impl Focusable for InputOtpState {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle(cx)
    }
}

impl InputOtpState {
    /// Six digits.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self::with_length(6, window, cx)
    }

    /// `length` slots (at least one).
    pub fn with_length(length: usize, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let length = length.max(1);
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .max_chars(length)
                // No corrections, capitals or suggested words for a code. (GPUI has no
                // one-time-code content type or number pad to ask the platform for yet.)
                .keyboard(TextInputConfiguration {
                    autocorrect: false,
                    autocapitalize: Autocapitalize::None,
                    suggestions: false,
                    input_action: TextInputAction::Done,
                })
        });
        let subscription = cx.subscribe_in(&input, window, |this, _, e, window, cx| {
            if *e == InputEvent::Change {
                this.changed(window, cx);
            }
        });
        Self {
            input,
            length,
            digits_only: true,
            told: String::new(),
            completed: None,
            on_complete: None,
            _subscription: subscription,
        }
    }

    /// Whether only the digits 0 to 9 are taken (they are unless told): anything else typed or
    /// pasted is dropped. Off, any character but white space is.
    pub fn digits_only(mut self, digits: bool) -> Self {
        self.digits_only = digits;
        self
    }

    /// How many slots.
    pub fn length(&self) -> usize {
        self.length
    }

    /// The code so far.
    pub fn value(&self, cx: &App) -> SharedString {
        self.input.read(cx).value()
    }

    /// Replaces the code (cut to what it takes), as a paste does, telling the host.
    pub fn set_value(&mut self, code: &str, window: &mut Window, cx: &mut Context<Self>) {
        let code = self.keep(code);
        self.input
            .update(cx, |i, cx| i.set_value(code.clone(), window, cx));
        self.changed(window, cx);
    }

    /// Gives it the keyboard.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        let focus = self.input.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
    }

    /// What of `text` it takes: allowed characters, up to its length.
    fn keep(&self, text: &str) -> String {
        text.chars()
            .filter(|c| {
                if self.digits_only {
                    c.is_ascii_digit()
                } else {
                    !c.is_whitespace() && !c.is_control()
                }
            })
            .take(self.length)
            .collect()
    }

    /// After the text changed: drops what it doesn't take, and tells the host.
    fn changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        let kept = self.keep(&text);
        if kept != text {
            self.input.update(cx, |i, cx| {
                // The caret where it was, less what's dropped before it.
                let at = i.selection().start;
                let before = self.keep(&text[..at.min(text.len())]).len();
                i.set_value(kept.clone(), window, cx);
                i.select(before..before, cx);
            });
        }
        if kept == self.told {
            return;
        }
        self.told = kept.clone();
        cx.emit(InputOtpEvent::Change(kept.clone().into()));
        if kept.chars().count() == self.length {
            if self.completed.as_ref() != Some(&kept) {
                self.completed = Some(kept.clone());
                let code: SharedString = kept.into();
                cx.emit(InputOtpEvent::Complete(code.clone()));
                if let Some(f) = self.on_complete.clone() {
                    f(&code, window, cx);
                }
            }
        } else {
            self.completed = None;
        }
        cx.notify();
    }

    /// A paste: the clipboard's text, what it takes of it, in place of the code.
    fn paste(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.set_value(&text, window, cx);
        }
    }
}

/// Draws an [`InputOtpState`]: its slots in a row, joined edge to edge within each group, the
/// one the next character goes in ringed while it has the keyboard. Typing fills the slots in
/// turn, Backspace empties the last, and a paste fills them all (what it takes of the
/// clipboard). A press anywhere on it gives it the keyboard.
#[derive(IntoElement)]
pub struct InputOtp {
    state: Entity<InputOtpState>,
    groups: Vec<usize>,
    label: Option<SharedString>,
    on_complete: Option<OnComplete>,
    invalid: bool,
    size: ControlSize,
    style: StyleRefinement,
}

impl InputOtp {
    pub fn new(state: &Entity<InputOtpState>) -> Self {
        Self {
            state: state.clone(),
            groups: Vec::new(),
            label: None,
            on_complete: None,
            invalid: false,
            size: ControlSize::Medium,
            style: StyleRefinement::default(),
        }
    }

    /// The slots in groups of these sizes, a dash between each (`[3, 3]`: `123–456`). Slots
    /// past the groups' total form one more group; by default they're all one.
    pub fn groups(mut self, sizes: impl IntoIterator<Item = usize>) -> Self {
        self.groups = sizes.into_iter().filter(|n| *n > 0).collect();
        self
    }

    /// Called with the code when every slot is filled.
    pub fn on_complete(
        mut self,
        f: impl Fn(&SharedString, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_complete = Some(Rc::new(f));
        self
    }

    /// What it's for, to assistive technology ("Verification code" unless told).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Drawn with the destructive colour's border: a code that was wrong.
    pub fn invalid(mut self, invalid: bool) -> Self {
        self.invalid = invalid;
        self
    }
}

impl Sizable for InputOtp {
    /// Only [`ControlSize::Touch`] changes it: 44 point slots rather than 36.
    fn with_size(mut self, size: ControlSize) -> Self {
        self.size = size;
        self
    }
}

impl Styled for InputOtp {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for InputOtp {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let dark = theme.mode.is_dark();
        if let Some(f) = self.on_complete.clone() {
            self.state.update(cx, |s, _| s.on_complete = Some(f));
        }
        let (input, length) = {
            let s = self.state.read(cx);
            (s.input.clone(), s.length)
        };
        let (code, caret, focused, focus) = {
            let i = input.read(cx);
            let text = i.value();
            let at = i.selection().start.min(text.len());
            let caret = text[..at].chars().count();
            (text, caret, i.is_focused(window), i.focus_handle(cx))
        };
        let chars: Vec<char> = code.chars().collect();
        // The slot the next character goes in: the caret's, or the last when they're full.
        let active = caret.min(length - 1);
        let side = if self.size == ControlSize::Touch {
            gpui::DefiniteLength::from(theme.touch_target)
        } else {
            gpui::DefiniteLength::from(css(36.))
        };
        let id = self.state.entity_id().as_u64();
        let border = if self.invalid {
            if dark {
                theme.destructive.opacity(0.5)
            } else {
                theme.destructive
            }
        } else {
            theme.input
        };
        let radius = theme.radius_md();

        // The groups: the host's sizes, then one of what's left.
        let mut sizes = Vec::new();
        let mut left = length;
        for n in &self.groups {
            if left == 0 {
                break;
            }
            let n = (*n).min(left);
            sizes.push(n);
            left -= n;
        }
        if left > 0 {
            sizes.push(left);
        }

        let mut row = div()
            .flex()
            .items_center()
            .gap(css(8.))
            .min_w_0()
            .max_w_full();
        let mut at = 0;
        for (g, n) in sizes.iter().enumerate() {
            if g > 0 {
                row = row.child(
                    div()
                        .flex_none()
                        .debug_selector(move || format!("otp-separator-{id}-{g}"))
                        .child(
                            Icon::new(IconName::Minus)
                                .size(css(16.))
                                .color(theme.muted_foreground),
                        ),
                );
            }
            let group = div()
                .flex()
                .items_center()
                .min_w_0()
                .children((0..*n).map(|k| {
                    let i = at + k;
                    let ch = chars.get(i).copied();
                    let on = focused && i == active;
                    let first = k == 0;
                    let last = k + 1 == *n;
                    div()
                        .debug_selector(move || format!("otp-slot-{id}-{i}"))
                        .relative()
                        .flex()
                        .items_center()
                        .justify_center()
                        // In a narrow space the slots narrow alike rather than run out of it.
                        .flex_shrink_1()
                        .min_w_0()
                        .w(side)
                        .h(side)
                        .overflow_hidden()
                        .border_y_1()
                        .border_r_1()
                        .when(first, |d| d.border_l_1().rounded_l(radius))
                        .when(last, |d| d.rounded_r(radius))
                        .border_color(if on { theme.ring } else { border })
                        .when(dark, |d| d.bg(theme.input.opacity(0.3)))
                        .when(on, |d| {
                            d.shadow(vec![crate::base::ring_inside(focus_ring(cx))])
                        })
                        .text_size(theme.text.sm)
                        .when_some(ch, |d, c| d.child(c.to_string()))
                        // Where the next character goes, while empty: a caret that doesn't blink
                        // (nothing redraws while nothing changes).
                        .when(on && ch.is_none(), |d| {
                            d.child(div().w(gpui::px(1.)).h(css(16.)).bg(theme.foreground))
                        })
                }));
            at += n;
            row = row.child(group);
        }

        let state = self.state.clone();
        let label = self
            .label
            .unwrap_or_else(|| SharedString::from("Verification code"));
        let mut el = div()
            .id(ElementId::NamedInteger("input-otp".into(), id))
            .debug_selector(move || format!("otp-{id}"))
            .relative()
            .flex()
            .min_w_0()
            .max_w_full()
            .cursor_text()
            // A paste fills every slot, from the start, with what it takes of the clipboard.
            .capture_action(move |_: &Paste, window, cx| {
                cx.stop_propagation();
                state.update(cx, |s, cx| s.paste(window, cx));
            })
            // The field itself, hidden under the slots: it takes the typing.
            .child(
                div()
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .opacity(0.)
                    .overflow_hidden()
                    .child(
                        Input::new(&input)
                            .appearance(false)
                            .accessibility_label(label)
                            .size_full(),
                    ),
            )
            .child(row)
            // Over everything: a press gives the field the keyboard with the caret at the end,
            // rather than wherever the hidden text happens to be under the pointer.
            .child(
                div()
                    .id("input-otp-cover")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .occlude()
                    .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                        cx.stop_propagation();
                        window.focus(&focus, cx);
                        input.update(cx, |i, cx| {
                            let end = i.value().len();
                            i.select(end..end, cx);
                        });
                    }),
            );
        gpui::Refineable::refine(el.style(), &self.style);
        el
    }
}
