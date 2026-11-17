//! Text input: one line ([`InputState::new`]) or many ([`InputState::textarea`]), drawn by
//! [`Input`].
//!
//! Text comes in through GPUI's platform input handler, so the macOS and iOS input methods
//! (Japanese, Chinese, accents held down, dictation, the character palette) and secure entry
//! work as they do everywhere else. Selection, the clipboard, undo and the usual macOS keys are
//! the crate's. The caret doesn't blink: a blinking caret redraws twice a second for as long as
//! a field has the focus, and nothing here redraws while nothing changes.

use std::ops::Range;

use gpui::{
    App, Autocapitalize, AvailableSpace, Bounds, ClipboardItem, Context, CursorStyle,
    DispatchPhase, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    EventEmitter, FocusHandle, Focusable, GlobalElementId, Hsla, InspectorElementId,
    InteractiveElement as _, IntoElement, KeyBinding, LayoutId, MouseButton, MouseDownEvent,
    MouseMoveEvent, MouseUpEvent, ParentElement as _, Pixels, Point, Refineable as _, RenderOnce,
    Role, ScrollEvent, SharedString, StatefulInteractiveElement as _, Style, StyleRefinement,
    Styled, Subscription, TextAlign, TextInputAction, TextInputConfiguration, TextRun,
    UTF16Selection, UnderlineStyle, Window, WrappedLine, actions, div, fill, point,
    prelude::FluentBuilder as _, px, relative, rems, size,
};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::press::focus_ring;
use crate::theme::ActiveTheme as _;

actions!(
    gpui_core_input,
    [
        Backspace,
        Delete,
        DeleteWordLeft,
        DeleteWordRight,
        DeleteToLineStart,
        DeleteToLineEnd,
        Left,
        Right,
        Up,
        Down,
        WordLeft,
        WordRight,
        LineStart,
        LineEnd,
        DocStart,
        DocEnd,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectWordLeft,
        SelectWordRight,
        SelectLineStart,
        SelectLineEnd,
        SelectDocStart,
        SelectDocEnd,
        SelectAll,
        Copy,
        Cut,
        Paste,
        Undo,
        Redo,
        /// Enter: a new line in a textarea, otherwise [`InputEvent::PressEnter`].
        Enter,
        /// Shift-Enter: always a new line in a textarea.
        NewLine,
        ShowCharacterPalette,
    ]
);

const CONTEXT: &str = "GpuiCoreInput";

pub(crate) fn init(cx: &mut App) {
    let c = Some(CONTEXT);
    // The platform's command key: ⌘ on macOS and iOS (a hardware keyboard), Ctrl elsewhere;
    // and its word key: ⌥, or Ctrl.
    macro_rules! cmd {
        ($rest:literal) => {
            if cfg!(any(target_os = "macos", target_os = "ios")) {
                concat!("cmd-", $rest)
            } else {
                concat!("ctrl-", $rest)
            }
        };
    }
    macro_rules! alt {
        ($rest:literal) => {
            if cfg!(any(target_os = "macos", target_os = "ios")) {
                concat!("alt-", $rest)
            } else {
                concat!("ctrl-", $rest)
            }
        };
    }
    macro_rules! b {
        ($keys:expr, $action:expr) => {
            KeyBinding::new($keys, $action, c)
        };
    }
    let mut bindings = vec![
        b!("backspace", Backspace),
        b!("shift-backspace", Backspace),
        b!("delete", Delete),
        b!(alt!("backspace"), DeleteWordLeft),
        b!(alt!("delete"), DeleteWordRight),
        b!(cmd!("backspace"), DeleteToLineStart),
        b!("left", Left),
        b!("right", Right),
        b!("up", Up),
        b!("down", Down),
        b!(alt!("left"), WordLeft),
        b!(alt!("right"), WordRight),
        b!(cmd!("left"), LineStart),
        b!(cmd!("right"), LineEnd),
        b!("home", LineStart),
        b!("end", LineEnd),
        b!(cmd!("up"), DocStart),
        b!(cmd!("down"), DocEnd),
        b!("shift-left", SelectLeft),
        b!("shift-right", SelectRight),
        b!("shift-up", SelectUp),
        b!("shift-down", SelectDown),
        b!(alt!("shift-left"), SelectWordLeft),
        b!(alt!("shift-right"), SelectWordRight),
        b!(cmd!("shift-left"), SelectLineStart),
        b!(cmd!("shift-right"), SelectLineEnd),
        b!("shift-home", SelectLineStart),
        b!("shift-end", SelectLineEnd),
        b!(cmd!("shift-up"), SelectDocStart),
        b!(cmd!("shift-down"), SelectDocEnd),
        b!(cmd!("a"), SelectAll),
        b!(cmd!("c"), Copy),
        b!(cmd!("x"), Cut),
        b!(cmd!("v"), Paste),
        b!(cmd!("z"), Undo),
        b!(cmd!("shift-z"), Redo),
        b!("enter", Enter),
        b!("shift-enter", NewLine),
    ];
    if cfg!(any(target_os = "macos", target_os = "ios")) {
        // The Emacs keys every Cocoa text field takes.
        bindings.extend([
            b!("ctrl-a", LineStart),
            b!("ctrl-e", LineEnd),
            b!("ctrl-b", Left),
            b!("ctrl-f", Right),
            b!("ctrl-p", Up),
            b!("ctrl-n", Down),
            b!("ctrl-h", Backspace),
            b!("ctrl-d", Delete),
            b!("ctrl-k", DeleteToLineEnd),
            b!("ctrl-cmd-space", ShowCharacterPalette),
        ]);
    } else {
        bindings.push(b!("ctrl-y", Redo));
    }
    cx.bind_keys(bindings);
}

/// What a field tells its host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputEvent {
    /// The text changed (typing, pasting, an input method, undo).
    Change,
    /// Enter in a one-line field, or in a textarea that submits on Enter. `secondary` when Shift
    /// was held (a one-line field only).
    PressEnter {
        secondary: bool,
    },
    Focus,
    Blur,
}

#[derive(Clone)]
struct Snapshot {
    text: String,
    selected: Range<usize>,
}

/// Room past a one-line field's text for the caret.
const CARET_ROOM: Pixels = px(2.);

/// The text a typing run started from, so one undo takes back the whole run.
#[derive(Clone, Copy, PartialEq, Eq)]
enum EditKind {
    Typing,
    Other,
}

/// The layout of the last draw, for the pointer, the input method and the arrows.
#[derive(Default)]
struct Layout {
    /// One shaped (and wrapped) line per line of the shown text, with where it starts in it.
    lines: Vec<(usize, WrappedLine)>,
    line_height: Pixels,
    /// The text area's bounds on screen.
    bounds: Bounds<Pixels>,
    /// How far a one-line field's text is moved right by its alignment.
    dx: Pixels,
}

/// A text field's text, selection and settings. Make it an entity, draw it with [`Input`], and
/// subscribe to its [`InputEvent`]s.
pub struct InputState {
    focus: FocusHandle,
    text: String,
    selected: Range<usize>,
    reversed: bool,
    marked: Option<Range<usize>>,
    placeholder: SharedString,
    masked: bool,
    multiline: bool,
    rows: (usize, usize),
    submit_on_enter: bool,
    /// What the platform keyboard is told, over what the field works out for itself.
    keyboard: Option<TextInputConfiguration>,
    /// The most characters typing and pasting can make it.
    max_chars: Option<usize>,
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    last_edit: Option<EditKind>,
    /// Where the arrows keep to going up and down: the x the caret was at.
    goal_x: Option<Pixels>,
    selecting: bool,
    scroll: Point<Pixels>,
    layout: Layout,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<InputEvent> for InputState {}

impl Focusable for InputState {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl InputState {
    /// A one-line field.
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        crate::init(cx);
        let focus = cx.focus_handle().tab_stop(true);
        let subscriptions = vec![
            cx.on_focus(&focus, window, |_, _, cx| cx.emit(InputEvent::Focus)),
            cx.on_blur(&focus, window, |this, _, cx| {
                this.selecting = false;
                cx.emit(InputEvent::Blur)
            }),
        ];
        Self {
            focus,
            text: String::new(),
            selected: 0..0,
            reversed: false,
            marked: None,
            placeholder: SharedString::default(),
            masked: false,
            multiline: false,
            rows: (1, 1),
            submit_on_enter: false,
            keyboard: None,
            max_chars: None,
            undo: Vec::new(),
            redo: Vec::new(),
            last_edit: None,
            goal_x: None,
            selecting: false,
            scroll: Point::default(),
            layout: Layout::default(),
            _subscriptions: subscriptions,
        }
    }

    /// A field of many lines that wraps its text, two rows tall to begin with and as tall as
    /// its text up to ten (see [`auto_grow`](Self::auto_grow)).
    pub fn textarea(window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self {
            multiline: true,
            rows: (2, 10),
            ..Self::new(window, cx)
        }
    }

    /// What it shows while empty.
    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    /// Shows a dot for every character (a password), and won't copy or cut.
    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    /// Masks the text or shows it (a password's Reveal), keeping the text and the selection.
    /// Masked, it won't copy or cut, and the platform keyboard is told it's secure entry.
    pub fn set_masked(&mut self, masked: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.masked != masked {
            self.masked = masked;
            window.notify_text_input_changed(&self.focus);
            cx.notify();
        }
    }

    /// Selects all its text, as ⌘A does (the field's focus is the host's to give).
    pub fn select_all(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.selected = 0..self.text.len();
        self.reversed = false;
        cx.notify();
    }

    /// The text it starts with.
    pub fn default_value(mut self, text: impl Into<String>) -> Self {
        self.text = sanitize(text.into(), self.multiline);
        let end = self.text.len();
        self.selected = end..end;
        self
    }

    /// A textarea's height: as tall as its text, from `min` rows to `max` (then it scrolls).
    pub fn auto_grow(mut self, min: usize, max: usize) -> Self {
        let min = min.max(1);
        self.rows = (min, max.max(min));
        self
    }

    /// What the on-screen keyboard offers (autocorrect, capitals, suggestions, its return
    /// key), instead of what the field works out: a sentence-case keyboard with suggestions for
    /// text, none of that for a masked field; Enter on a textarea's return key, Send if it
    /// submits on Enter, Done on one line.
    pub fn keyboard(mut self, keyboard: TextInputConfiguration) -> Self {
        self.keyboard = Some(keyboard);
        self
    }

    /// Whether Enter in a textarea is [`InputEvent::PressEnter`] (Shift-Enter makes a new line).
    pub fn submit_on_enter(mut self, submit: bool) -> Self {
        self.submit_on_enter = submit;
        self
    }

    /// The most characters (Unicode scalar values) typing, pasting and the input method can
    /// make it, as the web's `maxlength`: past it, typing does nothing and a paste is cut to
    /// fit, at a whole character as the eye sees it. While the input method is still composing,
    /// its text isn't counted until it's committed. The host's own `set_value` isn't cut.
    pub fn max_chars(mut self, max: usize) -> Self {
        self.max_chars = Some(max);
        self
    }

    /// Changes the limit, or removes it with `None`. Text already longer stays.
    pub fn set_max_chars(&mut self, max: Option<usize>) {
        self.max_chars = max;
    }

    /// Its text.
    pub fn value(&self) -> SharedString {
        SharedString::from(self.text.clone())
    }

    /// Replaces its text and puts the caret at the end. This isn't an edit: it's not undone, and
    /// it isn't announced as [`InputEvent::Change`].
    pub fn set_value(
        &mut self,
        text: impl Into<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.text = sanitize(text.into(), self.multiline);
        let end = self.text.len();
        self.selected = end..end;
        self.marked = None;
        self.undo.clear();
        self.redo.clear();
        self.last_edit = None;
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }

    /// Changes what it shows while empty.
    pub fn set_placeholder(
        &mut self,
        placeholder: impl Into<SharedString>,
        cx: &mut Context<Self>,
    ) {
        self.placeholder = placeholder.into();
        cx.notify();
    }

    /// Gives it the focus.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus, cx);
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    /// The selection, as byte offsets into [`value`](Self::value).
    pub fn selection(&self) -> Range<usize> {
        self.selected.clone()
    }

    /// Selects `range` (byte offsets, put on character boundaries).
    pub fn select(&mut self, range: Range<usize>, cx: &mut Context<Self>) {
        let start = self.boundary(range.start.min(self.text.len()));
        let end = self.boundary(range.end.min(self.text.len()));
        self.selected = start.min(end)..start.max(end);
        self.reversed = end < start;
        cx.notify();
    }

    fn boundary(&self, mut i: usize) -> usize {
        while i > 0 && !self.text.is_char_boundary(i) {
            i -= 1;
        }
        i
    }

    // ---- The caret and the selection.

    fn cursor(&self) -> usize {
        if self.reversed {
            self.selected.start
        } else {
            self.selected.end
        }
    }

    fn move_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        self.selected = offset..offset;
        self.reversed = false;
        self.last_edit = None;
        cx.notify();
    }

    fn select_to(&mut self, offset: usize, cx: &mut Context<Self>) {
        if self.reversed {
            self.selected.start = offset;
        } else {
            self.selected.end = offset;
        }
        if self.selected.end < self.selected.start {
            self.reversed = !self.reversed;
            self.selected = self.selected.end..self.selected.start;
        }
        self.last_edit = None;
        cx.notify();
    }

    fn previous_grapheme(&self, offset: usize) -> usize {
        self.text[..offset]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    }

    fn next_grapheme(&self, offset: usize) -> usize {
        self.text[offset..]
            .grapheme_indices(true)
            .nth(1)
            .map_or(self.text.len(), |(i, _)| offset + i)
    }

    /// The start of the word before `offset` (skipping spaces and punctuation first), as macOS.
    fn previous_word(&self, offset: usize) -> usize {
        let mut at = offset;
        let mut seen_word = false;
        for (i, w) in self.text[..offset].split_word_bound_indices().rev() {
            let is_word = w.chars().any(char::is_alphanumeric);
            if is_word {
                seen_word = true;
                at = i;
            } else if seen_word {
                break;
            } else {
                at = i;
            }
        }
        at
    }

    /// The end of the word after `offset` (skipping spaces and punctuation first), as macOS.
    fn next_word(&self, offset: usize) -> usize {
        let mut at = offset;
        for (i, w) in self.text[offset..].split_word_bound_indices() {
            at = offset + i + w.len();
            if w.chars().any(char::is_alphanumeric) {
                break;
            }
        }
        at
    }

    fn word_at(&self, offset: usize) -> Range<usize> {
        for (i, w) in self.text.split_word_bound_indices() {
            if offset >= i && offset < i + w.len() {
                return i..i + w.len();
            }
        }
        self.text.len()..self.text.len()
    }

    fn line_start(&self, offset: usize) -> usize {
        if !self.multiline {
            return 0;
        }
        // The start of the row the caret is on, wrapped or not.
        if let Some(p) = self.position_for(offset)
            && let Some(i) = self.index_for(point(px(-1e6), p.y))
        {
            return i;
        }
        self.text[..offset].rfind('\n').map_or(0, |i| i + 1)
    }

    fn line_end(&self, offset: usize) -> usize {
        if !self.multiline {
            return self.text.len();
        }
        // A wrapped row ends before its trailing space, on the next row's start.
        if let Some(p) = self.position_for(offset)
            && let Some(i) = self.index_for(point(px(1e6), p.y))
        {
            return i;
        }
        self.text[offset..]
            .find('\n')
            .map_or(self.text.len(), |i| offset + i)
    }

    /// The caret one row up (-1) or down (+1), keeping to the column it came from.
    fn vertical(&mut self, dir: f32) -> Option<usize> {
        let at = self.position_for(self.cursor())?;
        let x = *self.goal_x.get_or_insert(at.x);
        let y = at.y + self.layout.line_height * (0.5 + dir);
        if y < px(0.) {
            return Some(0);
        }
        let total = self.content_height();
        if y >= total {
            return Some(self.text.len());
        }
        self.index_for(point(x, y))
    }

    // ---- Text, shown and stored.

    /// The shown text: a dot for each character when masked.
    fn shown(&self) -> String {
        if self.masked {
            "•".repeat(self.text.chars().count())
        } else {
            self.text.clone()
        }
    }

    /// An offset in the text to one in the shown text.
    fn to_shown(&self, offset: usize) -> usize {
        if self.masked {
            self.text[..offset].chars().count() * '•'.len_utf8()
        } else {
            offset
        }
    }

    fn text_offset(&self, offset: usize) -> usize {
        if self.masked {
            let n = offset / '•'.len_utf8();
            self.text
                .char_indices()
                .nth(n)
                .map_or(self.text.len(), |(i, _)| i)
        } else {
            offset.min(self.text.len())
        }
    }

    fn offset_from_utf16(&self, offset: usize) -> usize {
        let mut utf8 = 0;
        let mut utf16 = 0;
        for ch in self.text.chars() {
            if utf16 >= offset {
                break;
            }
            utf16 += ch.len_utf16();
            utf8 += ch.len_utf8();
        }
        utf8
    }

    fn offset_to_utf16(&self, offset: usize) -> usize {
        self.text[..offset].chars().map(char::len_utf16).sum()
    }

    fn range_to_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_to_utf16(r.start)..self.offset_to_utf16(r.end)
    }

    fn range_from_utf16(&self, r: &Range<usize>) -> Range<usize> {
        self.offset_from_utf16(r.start)..self.offset_from_utf16(r.end)
    }

    // ---- Editing.

    fn checkpoint(&mut self, kind: EditKind) {
        if kind == EditKind::Typing && self.last_edit == Some(EditKind::Typing) {
            return;
        }
        self.undo.push(Snapshot {
            text: self.text.clone(),
            selected: self.selected.clone(),
        });
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.last_edit = Some(kind);
    }

    /// Puts `new` in place of `range` and the caret after it.
    fn replace(
        &mut self,
        range: Range<usize>,
        new: &str,
        kind: EditKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut new = sanitize(new.to_string(), self.multiline);
        if let Some(max) = self.max_chars {
            let kept = self.text.chars().count() - self.text[range.clone()].chars().count();
            new = fit(&new, max.saturating_sub(kept));
        }
        if range.is_empty() && new.is_empty() {
            return;
        }
        let kind = if new.contains(' ') || new.contains('\n') {
            EditKind::Other
        } else {
            kind
        };
        self.checkpoint(kind);
        self.text.replace_range(range.clone(), &new);
        let end = range.start + new.len();
        self.selected = end..end;
        self.reversed = false;
        self.marked = None;
        self.goal_x = None;
        if kind == EditKind::Other {
            self.last_edit = None;
        }
        window.notify_text_input_changed(&self.focus);
        cx.emit(InputEvent::Change);
        cx.notify();
    }

    fn delete_to(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.selected.is_empty() {
            let c = self.cursor();
            let range = c.min(offset)..c.max(offset);
            self.replace(range, "", EditKind::Other, window, cx);
        } else {
            self.replace(self.selected.clone(), "", EditKind::Other, window, cx);
        }
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        let to = self.previous_grapheme(self.cursor());
        self.delete_to(to, window, cx);
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        let to = self.next_grapheme(self.cursor());
        self.delete_to(to, window, cx);
    }

    fn delete_word_left(
        &mut self,
        _: &DeleteWordLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = if self.masked {
            0
        } else {
            self.previous_word(self.cursor())
        };
        self.delete_to(to, window, cx);
    }

    fn delete_word_right(
        &mut self,
        _: &DeleteWordRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = if self.masked {
            self.text.len()
        } else {
            self.next_word(self.cursor())
        };
        self.delete_to(to, window, cx);
    }

    fn delete_to_line_start(
        &mut self,
        _: &DeleteToLineStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let to = self.line_start(self.cursor());
        self.delete_to(to, window, cx);
    }

    fn delete_to_line_end(
        &mut self,
        _: &DeleteToLineEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let c = self.cursor();
        let end = self.line_end(c);
        // At the end of a line, ⌃K takes the line break, as Cocoa's does.
        let to = if end == c { self.next_grapheme(c) } else { end };
        self.delete_to(to, window, cx);
    }

    fn left(&mut self, _: &Left, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        if self.selected.is_empty() {
            self.move_to(self.previous_grapheme(self.cursor()), cx);
        } else {
            self.move_to(self.selected.start, cx);
        }
    }

    fn right(&mut self, _: &Right, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        if self.selected.is_empty() {
            self.move_to(self.next_grapheme(self.cursor()), cx);
        } else {
            self.move_to(self.selected.end, cx);
        }
    }

    fn up(&mut self, _: &Up, _: &mut Window, cx: &mut Context<Self>) {
        if !self.multiline {
            return self.move_to(0, cx);
        }
        let to = self.vertical(-1.).unwrap_or(0);
        let goal = self.goal_x;
        self.move_to(to, cx);
        self.goal_x = goal;
    }

    fn down(&mut self, _: &Down, _: &mut Window, cx: &mut Context<Self>) {
        if !self.multiline {
            return self.move_to(self.text.len(), cx);
        }
        let to = self.vertical(1.).unwrap_or(self.text.len());
        let goal = self.goal_x;
        self.move_to(to, cx);
        self.goal_x = goal;
    }

    fn word_left(&mut self, _: &WordLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let to = if self.masked {
            0
        } else {
            self.previous_word(self.cursor())
        };
        self.move_to(to, cx);
    }

    fn word_right(&mut self, _: &WordRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        let to = if self.masked {
            self.text.len()
        } else {
            self.next_word(self.cursor())
        };
        self.move_to(to, cx);
    }

    fn line_start_action(&mut self, _: &LineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_to(self.line_start(self.cursor()), cx);
    }

    fn line_end_action(&mut self, _: &LineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_to(self.line_end(self.cursor()), cx);
    }

    fn doc_start(&mut self, _: &DocStart, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_to(0, cx);
    }

    fn doc_end(&mut self, _: &DocEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.move_to(self.text.len(), cx);
    }

    fn select_left(&mut self, _: &SelectLeft, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.select_to(self.previous_grapheme(self.cursor()), cx);
    }

    fn select_right(&mut self, _: &SelectRight, _: &mut Window, cx: &mut Context<Self>) {
        self.goal_x = None;
        self.select_to(self.next_grapheme(self.cursor()), cx);
    }

    fn select_up(&mut self, _: &SelectUp, _: &mut Window, cx: &mut Context<Self>) {
        let to = if self.multiline {
            self.vertical(-1.).unwrap_or(0)
        } else {
            0
        };
        let goal = self.goal_x;
        self.select_to(to, cx);
        self.goal_x = goal;
    }

    fn select_down(&mut self, _: &SelectDown, _: &mut Window, cx: &mut Context<Self>) {
        let end = self.text.len();
        let to = if self.multiline {
            self.vertical(1.).unwrap_or(end)
        } else {
            end
        };
        let goal = self.goal_x;
        self.select_to(to, cx);
        self.goal_x = goal;
    }

    fn select_word_left(&mut self, _: &SelectWordLeft, _: &mut Window, cx: &mut Context<Self>) {
        let to = if self.masked {
            0
        } else {
            self.previous_word(self.cursor())
        };
        self.select_to(to, cx);
    }

    fn select_word_right(&mut self, _: &SelectWordRight, _: &mut Window, cx: &mut Context<Self>) {
        let to = if self.masked {
            self.text.len()
        } else {
            self.next_word(self.cursor())
        };
        self.select_to(to, cx);
    }

    fn select_line_start(&mut self, _: &SelectLineStart, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.line_start(self.cursor()), cx);
    }

    fn select_line_end(&mut self, _: &SelectLineEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.line_end(self.cursor()), cx);
    }

    fn select_doc_start(&mut self, _: &SelectDocStart, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(0, cx);
    }

    fn select_doc_end(&mut self, _: &SelectDocEnd, _: &mut Window, cx: &mut Context<Self>) {
        self.select_to(self.text.len(), cx);
    }

    fn on_select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.select_all(window, cx);
    }

    fn copy(&mut self, _: &Copy, _: &mut Window, cx: &mut Context<Self>) {
        if !self.masked && !self.selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.text[self.selected.clone()].to_string(),
            ));
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if !self.masked && !self.selected.is_empty() {
            cx.write_to_clipboard(ClipboardItem::new_string(
                self.text[self.selected.clone()].to_string(),
            ));
            self.replace(self.selected.clone(), "", EditKind::Other, window, cx);
        }
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.replace(self.selected.clone(), &text, EditKind::Other, window, cx);
        }
    }

    fn undo_action(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.undo.pop() {
            self.redo.push(Snapshot {
                text: std::mem::take(&mut self.text),
                selected: self.selected.clone(),
            });
            self.text = s.text;
            self.selected = s.selected;
            self.reversed = false;
            self.marked = None;
            self.last_edit = None;
            window.notify_text_input_changed(&self.focus);
            cx.emit(InputEvent::Change);
            cx.notify();
        }
    }

    fn redo_action(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(s) = self.redo.pop() {
            self.undo.push(Snapshot {
                text: std::mem::take(&mut self.text),
                selected: self.selected.clone(),
            });
            self.text = s.text;
            self.selected = s.selected;
            self.reversed = false;
            self.marked = None;
            self.last_edit = None;
            window.notify_text_input_changed(&self.focus);
            cx.emit(InputEvent::Change);
            cx.notify();
        }
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline && !self.submit_on_enter {
            self.replace(self.selected.clone(), "\n", EditKind::Other, window, cx);
        } else {
            cx.emit(InputEvent::PressEnter { secondary: false });
        }
    }

    fn new_line(&mut self, _: &NewLine, window: &mut Window, cx: &mut Context<Self>) {
        if self.multiline {
            self.replace(self.selected.clone(), "\n", EditKind::Other, window, cx);
        } else {
            cx.emit(InputEvent::PressEnter { secondary: true });
        }
    }

    fn show_character_palette(
        &mut self,
        _: &ShowCharacterPalette,
        window: &mut Window,
        _: &mut Context<Self>,
    ) {
        window.show_character_palette();
    }

    // ---- The pointer.

    fn mouse_down(&mut self, e: &MouseDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let Some(at) = self.index_for_screen(e.position) else {
            return;
        };
        self.goal_x = None;
        match e.click_count {
            2 => {
                let w = if self.masked {
                    0..self.text.len()
                } else {
                    self.word_at(at)
                };
                self.selected = w;
                self.reversed = false;
            }
            n if n >= 3 => {
                let start = self.text[..at].rfind('\n').map_or(0, |i| i + 1);
                let end = self.text[at..]
                    .find('\n')
                    .map_or(self.text.len(), |i| at + i);
                self.selected = start..end;
                self.reversed = false;
            }
            _ if e.modifiers.shift => self.select_to(at, cx),
            _ => self.move_to(at, cx),
        }
        self.selecting = true;
        cx.notify();
    }

    fn mouse_drag(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        if self.selecting
            && let Some(at) = self.index_for_screen(position)
        {
            self.select_to(at, cx);
        }
    }

    fn scroll_wheel(&mut self, e: &ScrollEvent, _: &mut Window, cx: &mut Context<Self>) {
        let delta = e.delta.pixel_delta(self.layout.line_height);
        let visible = self.layout.bounds.size.height;
        let max = (self.content_height() - visible).max(px(0.));
        if self.multiline && max > px(0.) {
            let y = (self.scroll.y - delta.y).clamp(px(0.), max);
            if y != self.scroll.y {
                self.scroll.y = y;
                cx.stop_propagation();
                cx.notify();
            }
        }
    }

    // ---- The layout of the last draw.

    fn content_height(&self) -> Pixels {
        let lh = self.layout.line_height;
        self.layout
            .lines
            .iter()
            .map(|(_, l)| l.size(lh).height)
            .sum::<Pixels>()
            .max(lh)
    }

    /// Where `offset` is, from the text's top left (before scrolling).
    fn position_for(&self, offset: usize) -> Option<Point<Pixels>> {
        let shown = self.to_shown(offset);
        let lh = self.layout.line_height;
        let mut y = px(0.);
        for (i, (start, line)) in self.layout.lines.iter().enumerate() {
            let end = start + line.len();
            let last = i + 1 == self.layout.lines.len();
            if shown <= end || last {
                let local = shown.saturating_sub(*start).min(line.len());
                let p = line.position_for_index(local, lh)?;
                return Some(point(p.x + self.layout.dx, y + p.y));
            }
            y += line.size(lh).height;
        }
        Some(point(px(0.), px(0.)))
    }

    /// The offset nearest `p`, from the text's top left (before scrolling).
    fn index_for(&self, p: Point<Pixels>) -> Option<usize> {
        let lh = self.layout.line_height;
        let mut y = px(0.);
        let n = self.layout.lines.len();
        for (i, (start, line)) in self.layout.lines.iter().enumerate() {
            let h = line.size(lh).height;
            if p.y < y + h || i + 1 == n {
                let x = (p.x - self.layout.dx).max(px(0.));
                let local = point(x, (p.y - y).clamp(px(0.), h - px(0.01)));
                let at = match line.closest_index_for_position(local, lh) {
                    Ok(i) | Err(i) => i,
                };
                return Some(self.text_offset(start + at));
            }
            y += h;
        }
        Some(0)
    }

    fn index_for_screen(&self, p: Point<Pixels>) -> Option<usize> {
        let b = self.layout.bounds;
        if self.layout.lines.is_empty() {
            return Some(0);
        }
        self.index_for(p - b.origin + self.scroll)
    }

    /// Keeps the caret in view after an edit or a move.
    fn scroll_to_cursor(&mut self) {
        let Some(c) = self.position_for(self.cursor()) else {
            return;
        };
        let b = self.layout.bounds.size;
        let lh = self.layout.line_height;
        if self.multiline {
            if c.y < self.scroll.y {
                self.scroll.y = c.y;
            } else if c.y + lh > self.scroll.y + b.height {
                self.scroll.y = c.y + lh - b.height;
            }
            let max = (self.content_height() - b.height).max(px(0.));
            self.scroll.y = self.scroll.y.clamp(px(0.), max);
            self.scroll.x = px(0.);
        } else {
            let caret = CARET_ROOM;
            if c.x < self.scroll.x {
                self.scroll.x = c.x;
            } else if c.x + caret > self.scroll.x + b.width {
                self.scroll.x = c.x + caret - b.width;
            }
            let width = self.layout.lines.first().map_or(px(0.), |(_, l)| l.width());
            let max = (width + caret - b.width).max(px(0.));
            self.scroll.x = self.scroll.x.clamp(px(0.), max);
            self.scroll.y = px(0.);
        }
    }
}

/// As much of `text` as fits in `room` characters, cut between graphemes.
fn fit(text: &str, room: usize) -> String {
    let mut out = String::new();
    let mut used = 0;
    for g in text.graphemes(true) {
        let n = g.chars().count();
        if used + n > room {
            break;
        }
        used += n;
        out.push_str(g);
    }
    out
}

/// One line, or many: a one-line field takes line breaks as spaces (a pasted address, say).
fn sanitize(text: String, multiline: bool) -> String {
    let text = if text.contains('\r') {
        text.replace("\r\n", "\n").replace('\r', "\n")
    } else {
        text
    };
    if multiline || !text.contains('\n') {
        text
    } else {
        text.replace('\n', " ")
    }
}

impl EntityInputHandler for InputState {
    fn text_for_range(
        &mut self,
        range: Range<usize>,
        actual: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        let range = self.range_from_utf16(&range);
        actual.replace(self.range_to_utf16(&range));
        if self.masked {
            return Some("•".repeat(self.text[range].chars().count()));
        }
        Some(self.text[range].to_string())
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        Some(UTF16Selection {
            range: self.range_to_utf16(&self.selected),
            reversed: self.reversed,
        })
    }

    fn set_selected_text_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected = self.range_from_utf16(&range);
        self.reversed = false;
        window.notify_text_selection_changed(&self.focus);
        cx.notify();
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.offset_to_utf16(self.text.len()))
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.as_ref().map(|r| self.range_to_utf16(r))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Committed as it is: what doesn't fit the limit is cut, as a typed commit's is.
        if let (Some(max), Some(r)) = (self.max_chars, self.marked.clone())
            && let Some(marked) = self.text.get(r.clone())
        {
            let kept = self.text.chars().count() - marked.chars().count();
            let room = max.saturating_sub(kept);
            if marked.chars().count() > room {
                let cut = fit(marked, room);
                self.text.replace_range(r.clone(), &cut);
                let end = r.start + cut.len();
                self.selected = end..end;
                self.reversed = false;
                cx.emit(InputEvent::Change);
            }
        }
        self.marked = None;
        window.notify_text_input_changed(&self.focus);
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked.clone())
            .unwrap_or(self.selected.clone());
        self.replace(range, new, EditKind::Typing, window, cx);
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range: Option<Range<usize>>,
        new: &str,
        new_selected: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let range = range
            .as_ref()
            .map(|r| self.range_from_utf16(r))
            .or(self.marked.clone())
            .unwrap_or(self.selected.clone());
        let new = sanitize(new.to_string(), self.multiline);
        self.checkpoint(EditKind::Typing);
        self.text.replace_range(range.clone(), &new);
        self.marked = (!new.is_empty()).then(|| range.start..range.start + new.len());
        let start16 = self.offset_to_utf16(range.start);
        let len16 = new.encode_utf16().count();
        self.selected = new_selected
            .map(|s| {
                self.range_from_utf16(&(start16 + s.start.min(len16)..start16 + s.end.min(len16)))
            })
            .unwrap_or_else(|| range.start + new.len()..range.start + new.len());
        self.reversed = false;
        window.notify_text_input_changed(&self.focus);
        cx.emit(InputEvent::Change);
        cx.notify();
    }

    fn bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range);
        let lh = self.layout.line_height;
        let a = self.position_for(range.start)?;
        let b = self.position_for(range.end)?;
        let origin = bounds.origin - self.scroll;
        let end = if b.y > a.y {
            point(a.x + px(1.), a.y + lh)
        } else {
            point(b.x, b.y + lh)
        };
        Some(Bounds::from_corners(origin + a, origin + end))
    }

    fn character_index_for_point(
        &mut self,
        p: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        let at = self.index_for_screen(p)?;
        Some(self.offset_to_utf16(at))
    }

    fn paste(&mut self, item: ClipboardItem, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = item.text() {
            self.replace(self.selected.clone(), &text, EditKind::Other, window, cx);
        }
    }

    /// One band per row the range covers, on screen (the platform's selection handles and its
    /// text-editing menu sit on them).
    fn selection_bounds_for_range(
        &mut self,
        range: Range<usize>,
        bounds: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Vec<Bounds<Pixels>> {
        let range = self.range_from_utf16(&range);
        let lh = self.layout.line_height;
        let (Some(a), Some(b)) = (self.position_for(range.start), self.position_for(range.end))
        else {
            return Vec::new();
        };
        let origin = bounds.origin - self.scroll;
        let right = self.layout.bounds.size.width + self.scroll.x;
        let mut out = Vec::new();
        let mut row = a.y;
        while row <= b.y + px(0.5) {
            let x0 = if (row - a.y).abs() < px(0.5) {
                a.x
            } else {
                px(0.)
            };
            let x1 = if (row - b.y).abs() < px(0.5) {
                b.x
            } else {
                right
            };
            out.push(Bounds::from_corners(
                origin + point(x0, row),
                origin + point(x1.max(x0), row + lh),
            ));
            row += lh;
        }
        out
    }

    fn accepts_text_input(&self, _: &mut Window, _: &mut Context<Self>) -> bool {
        true
    }

    fn text_input_configuration(
        &mut self,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> TextInputConfiguration {
        if let Some(k) = &self.keyboard {
            return k.clone();
        }
        let text = !self.masked;
        TextInputConfiguration {
            autocorrect: text,
            autocapitalize: if text {
                Autocapitalize::Sentences
            } else {
                Autocapitalize::None
            },
            suggestions: text,
            input_action: match (self.multiline, self.submit_on_enter) {
                (true, false) => TextInputAction::Enter,
                (true, true) => TextInputAction::Send,
                (false, _) => TextInputAction::Done,
            },
        }
    }

    /// The whole text: nothing in a field is out of the input method's reach.
    fn text_input_editable_range(
        &mut self,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Range<usize>> {
        Some(0..self.offset_to_utf16(self.text.len()))
    }
}

fn css(v: f32) -> gpui::Rems {
    rems(v / 15.)
}

/// Draws an [`InputState`]: a field with the web app's look (`input.tsx`, `textarea.tsx`), or
/// with no look at all ([`appearance(false)`](Input::appearance): no border, no fill and no
/// padding, for a host that draws its own).
#[derive(IntoElement)]
pub struct Input {
    state: Entity<InputState>,
    appearance: bool,
    label: Option<SharedString>,
    colors: InputColors,
    align: TextAlign,
    style: StyleRefinement,
}

/// Colours an [`Input`] draws its caret, selection and placeholder in, over the theme's: for a
/// host with a palette of its own (a light card in a dark app). The text itself takes the colour
/// of the text around the field.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct InputColors {
    caret: Option<Hsla>,
    selection: Option<Hsla>,
    placeholder: Option<Hsla>,
    ring: Option<Hsla>,
}

impl Input {
    pub fn new(state: &Entity<InputState>) -> Self {
        Self {
            state: state.clone(),
            appearance: true,
            label: None,
            colors: InputColors::default(),
            align: TextAlign::Left,
            style: StyleRefinement::default(),
        }
    }

    /// Where a one-line field's text sits while it fits: right for numbers, say. Wider than the
    /// field, it scrolls from the left as any. A textarea's text stays to the left.
    pub fn text_align(mut self, align: TextAlign) -> Self {
        self.align = align;
        self
    }

    /// The caret's colour (otherwise the theme's foreground).
    pub fn caret_color(mut self, color: impl Into<Hsla>) -> Self {
        self.colors.caret = Some(color.into());
        self
    }

    /// The selection's colour, drawn behind the text as given (otherwise the theme's ring at
    /// 30%).
    pub fn selection_color(mut self, color: impl Into<Hsla>) -> Self {
        self.colors.selection = Some(color.into());
        self
    }

    /// The keyboard focus ring's colour (with `appearance`), drawn as given (otherwise the
    /// theme's ring at half strength).
    pub fn focus_ring_color(mut self, color: impl Into<Hsla>) -> Self {
        self.colors.ring = Some(color.into());
        self
    }

    /// The placeholder's colour (otherwise the theme's muted foreground).
    pub fn placeholder_color(mut self, color: impl Into<Hsla>) -> Self {
        self.colors.placeholder = Some(color.into());
        self
    }

    /// Whether it draws the web's field (a border, the input fill, padding and a focus ring).
    pub fn appearance(mut self, appearance: bool) -> Self {
        self.appearance = appearance;
        self
    }

    /// What it's for, to assistive technology (the label beside it, usually).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }
}

impl Styled for Input {
    fn style(&mut self) -> &mut StyleRefinement {
        &mut self.style
    }
}

impl RenderOnce for Input {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let s = self.state.clone();
        let (focus, multiline, masked, empty, value, placeholder) = {
            let st = s.read(cx);
            (
                st.focus.clone(),
                st.multiline,
                st.masked,
                st.text.is_empty(),
                st.text.clone(),
                st.placeholder.clone(),
            )
        };
        let ring = match self.colors.ring {
            Some(color) => gpui::BoxShadow {
                color,
                ..focus_ring(cx)
            },
            None => focus_ring(cx),
        };
        let mut el = div()
            .id(ElementId::Integer(s.entity_id().as_u64()))
            .key_context(CONTEXT)
            .track_focus(&focus)
            .role(if multiline {
                Role::MultilineTextInput
            } else {
                Role::TextInput
            })
            .when(!masked, |d| d.aria_value(SharedString::from(value)))
            .when(empty && !placeholder.is_empty(), |d| {
                d.aria_placeholder(placeholder)
            })
            .when_some(self.label, |d, l| d.aria_label(l))
            .cursor(CursorStyle::IBeam)
            .on_action(window.listener_for(&s, InputState::backspace))
            .on_action(window.listener_for(&s, InputState::delete))
            .on_action(window.listener_for(&s, InputState::delete_word_left))
            .on_action(window.listener_for(&s, InputState::delete_word_right))
            .on_action(window.listener_for(&s, InputState::delete_to_line_start))
            .on_action(window.listener_for(&s, InputState::delete_to_line_end))
            .on_action(window.listener_for(&s, InputState::left))
            .on_action(window.listener_for(&s, InputState::right))
            .on_action(window.listener_for(&s, InputState::up))
            .on_action(window.listener_for(&s, InputState::down))
            .on_action(window.listener_for(&s, InputState::word_left))
            .on_action(window.listener_for(&s, InputState::word_right))
            .on_action(window.listener_for(&s, InputState::line_start_action))
            .on_action(window.listener_for(&s, InputState::line_end_action))
            .on_action(window.listener_for(&s, InputState::doc_start))
            .on_action(window.listener_for(&s, InputState::doc_end))
            .on_action(window.listener_for(&s, InputState::select_left))
            .on_action(window.listener_for(&s, InputState::select_right))
            .on_action(window.listener_for(&s, InputState::select_up))
            .on_action(window.listener_for(&s, InputState::select_down))
            .on_action(window.listener_for(&s, InputState::select_word_left))
            .on_action(window.listener_for(&s, InputState::select_word_right))
            .on_action(window.listener_for(&s, InputState::select_line_start))
            .on_action(window.listener_for(&s, InputState::select_line_end))
            .on_action(window.listener_for(&s, InputState::select_doc_start))
            .on_action(window.listener_for(&s, InputState::select_doc_end))
            .on_action(window.listener_for(&s, InputState::on_select_all))
            .on_action(window.listener_for(&s, InputState::copy))
            .on_action(window.listener_for(&s, InputState::cut))
            .on_action(window.listener_for(&s, InputState::paste))
            .on_action(window.listener_for(&s, InputState::undo_action))
            .on_action(window.listener_for(&s, InputState::redo_action))
            .on_action(window.listener_for(&s, InputState::enter))
            .on_action(window.listener_for(&s, InputState::new_line))
            .on_action(window.listener_for(&s, InputState::show_character_palette))
            .on_mouse_down(
                MouseButton::Left,
                window.listener_for(&s, InputState::mouse_down),
            )
            .on_scroll(window.listener_for(&s, InputState::scroll_wheel))
            .flex()
            .min_w_0()
            .when(self.appearance, |d| {
                let dark = theme.mode.is_dark();
                d.w_full()
                    .px(css(12.))
                    .py(if multiline { css(8.) } else { css(4.) })
                    .when(!multiline, |d| d.min_h(css(36.)).items_center())
                    .rounded(theme.radius_md())
                    .border_1()
                    .border_color(theme.input)
                    .when(dark, |d| d.bg(theme.input.opacity(0.3)))
                    .text_size(theme.text.sm)
                    .focus_visible(move |s| s.border_color(theme.ring).shadow(vec![ring]))
            });
        el.style().refine(&self.style);
        el.child(TextBox {
            state: s,
            colors: self.colors,
            align: self.align,
        })
    }
}

/// The text, the selection and the caret, laid out to the width it's given.
struct TextBox {
    state: Entity<InputState>,
    colors: InputColors,
    align: TextAlign,
}

impl IntoElement for TextBox {
    type Element = Self;
    fn into_element(self) -> Self {
        self
    }
}

struct Prepaint {
    /// Where each line of the layout is drawn.
    origins: Vec<Point<Pixels>>,
    selection: Vec<Bounds<Pixels>>,
    caret: Option<Bounds<Pixels>>,
}

/// The shown text's runs: the placeholder, or the text with any marked (composing) part
/// underlined.
fn runs_for(
    state: &InputState,
    shown: &str,
    color: Hsla,
    placeholder: Hsla,
    font: gpui::Font,
) -> (String, Vec<TextRun>) {
    let run = |len, color| TextRun {
        len,
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    if shown.is_empty() {
        let p = state.placeholder.to_string();
        let len = p.len();
        return (p, vec![run(len, placeholder)]);
    }
    let mut runs = Vec::new();
    match &state.marked {
        Some(m) if !state.masked => {
            let (a, b) = (state.to_shown(m.start), state.to_shown(m.end));
            if a > 0 {
                runs.push(run(a, color));
            }
            runs.push(TextRun {
                underline: Some(UnderlineStyle {
                    color: Some(color),
                    thickness: px(1.),
                    wavy: false,
                }),
                ..run(b - a, color)
            });
            if shown.len() > b {
                runs.push(run(shown.len() - b, color));
            }
        }
        _ => runs.push(run(shown.len(), color)),
    }
    (shown.to_string(), runs)
}

/// Splits `text` and its runs at line breaks: one line's text and runs each, with where it starts.
fn split_lines(text: &str, runs: &[TextRun]) -> Vec<(usize, String, Vec<TextRun>)> {
    let mut out = Vec::new();
    let mut start = 0;
    for line in text.split('\n') {
        let end = start + line.len();
        let mut lr = Vec::new();
        let mut at = 0;
        for r in runs {
            let (rs, re) = (at, at + r.len);
            at = re;
            let (s, e) = (rs.max(start), re.min(end));
            if s < e {
                lr.push(TextRun {
                    len: e - s,
                    ..r.clone()
                });
            }
        }
        out.push((start, line.to_string(), lr));
        start = end + 1;
    }
    out
}

impl Element for TextBox {
    type RequestLayoutState = ();
    type PrepaintState = Prepaint;

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        let state = self.state.clone();
        let (multiline, rows) = {
            let s = state.read(cx);
            (s.multiline, s.rows)
        };
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.min_size.width = px(0.).into();
        style.flex_grow = 1.;
        style.flex_shrink = 1.;
        let line_height = window.line_height();
        if !multiline {
            style.size.height = line_height.into();
            return (window.request_layout(style, [], cx), ());
        }
        // A textarea is as tall as its wrapped text, from its fewest rows to its most.
        let text_style = window.text_style();
        let id = window.request_measured_layout(style, move |known, available, window, cx| {
            let width = known.width.or(match available.width {
                AvailableSpace::Definite(w) => Some(w),
                _ => None,
            });
            let font_size = text_style.font_size.to_pixels(window.rem_size());
            let lh = text_style.line_height_in_pixels(window.rem_size());
            let s = state.read(cx);
            let shown = s.shown();
            let (text, runs) = runs_for(
                s,
                &shown,
                text_style.color,
                text_style.color,
                text_style.font(),
            );
            let mut n = 0usize;
            for (_, line, lr) in split_lines(&text, &runs) {
                let shaped = window
                    .text_system()
                    .shape_text(line.into(), font_size, &lr, width, None)
                    .map(|v| {
                        v.iter()
                            .map(|l| l.wrap_boundaries().len() + 1)
                            .sum::<usize>()
                    })
                    .unwrap_or(1);
                n += shaped.max(1);
            }
            let rows = n.clamp(s.rows.0, s.rows.1);
            size(width.unwrap_or(px(0.)), lh * rows as f32)
        });
        let _ = rows;
        (id, ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) -> Prepaint {
        let theme = cx.theme().clone();
        let style = window.text_style();
        let font_size = style.font_size.to_pixels(window.rem_size());
        let lh = window.line_height();
        let focused = self.state.read(cx).focus.is_focused(window);
        let colors = self.colors;
        let align = self.align;
        self.state.update(cx, |s, _| {
            let shown = s.shown();
            let (text, runs) = runs_for(
                s,
                &shown,
                style.color,
                colors.placeholder.unwrap_or(theme.muted_foreground),
                style.font(),
            );
            let wrap = s.multiline.then_some(bounds.size.width);
            let mut lines = Vec::new();
            for (start, line, lr) in split_lines(&text, &runs) {
                if let Ok(mut shaped) =
                    window
                        .text_system()
                        .shape_text(line.into(), font_size, &lr, wrap, None)
                    && let Some(l) = shaped.pop()
                {
                    lines.push((start, l));
                }
            }
            // A one-line field's text where its alignment puts it, while it fits; past that it
            // scrolls from the left, as any.
            let room = bounds.size.width - CARET_ROOM;
            let dx = match (s.multiline, lines.first()) {
                (false, Some((_, l))) => match align {
                    TextAlign::Left => px(0.),
                    TextAlign::Center => ((room - l.width()) / 2.).max(px(0.)),
                    TextAlign::Right => (room - l.width()).max(px(0.)),
                },
                _ => px(0.),
            };
            s.layout = Layout {
                lines,
                line_height: lh,
                bounds,
                dx,
            };
            s.scroll_to_cursor();
        });
        let s = self.state.read(cx);
        let origin = bounds.origin - s.scroll;
        let mut y = px(0.);
        let mut origins = Vec::new();
        for (_, l) in &s.layout.lines {
            origins.push(origin + point(s.layout.dx, y));
            y += l.size(lh).height;
        }
        let empty = s.text.is_empty();
        // The selection: a band per row it covers.
        let mut selection = Vec::new();
        if !s.selected.is_empty() && !empty {
            let (a, b) = (
                s.position_for(s.selected.start),
                s.position_for(s.selected.end),
            );
            if let (Some(a), Some(b)) = (a, b) {
                let right = bounds.size.width.max(px(4.)) + s.scroll.x;
                let mut row = a.y;
                while row <= b.y + px(0.5) {
                    let x0 = if row == a.y { a.x } else { px(0.) };
                    let x1 = if (row - b.y).abs() < px(0.5) {
                        b.x
                    } else {
                        right
                    };
                    if x1 > x0 {
                        selection.push(Bounds::from_corners(
                            origin + point(x0, row),
                            origin + point(x1, row + lh),
                        ));
                    }
                    row += lh;
                }
            }
        }
        let caret = (focused && s.selected.is_empty())
            .then(|| s.position_for(s.cursor()))
            .flatten()
            .map(|p| Bounds::new(origin + p, size(px(1.5), lh)));
        Prepaint {
            origins,
            selection,
            caret,
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        p: &mut Prepaint,
        window: &mut Window,
        cx: &mut App,
    ) {
        let theme = cx.theme().clone();
        let focus = self.state.read(cx).focus.clone();
        window.handle_input(
            &focus,
            ElementInputHandler::new(bounds, self.state.clone()),
            cx,
        );
        let lh = window.line_height();
        // The text is clipped to the field, with three pixels' room either side for ink that
        // reaches past a glyph's advance (an emoji, a wide character on a narrow row).
        let ink = rems(3. / 15.).to_pixels(window.rem_size());
        let clip = Bounds::from_corners(
            point(bounds.left() - ink, bounds.top()),
            point(bounds.right() + ink, bounds.bottom()),
        );
        window.with_content_mask(Some(gpui::ContentMask { bounds: clip }), |window| {
            for b in p.selection.drain(..) {
                window.paint_quad(fill(b, self.colors.selection.unwrap_or(theme.selection())));
            }
            // The shaped lines stay with the state (the pointer and the input method read
            // them between draws); they're borrowed out of it to paint.
            let lines = self
                .state
                .update(cx, |s, _| std::mem::take(&mut s.layout.lines));
            for (origin, (_, line)) in p.origins.iter().zip(&lines) {
                let _ = line.paint(*origin, lh, TextAlign::Left, None, window, cx);
            }
            self.state.update(cx, |s, _| s.layout.lines = lines);
            if let Some(c) = p.caret.take() {
                window.paint_quad(fill(c, self.colors.caret.unwrap_or(theme.foreground)));
            }
        });
        // A drag that leaves the field keeps selecting until the button is let go.
        if self.state.read(cx).selecting {
            let s = self.state.clone();
            window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble && e.pressed_button == Some(MouseButton::Left) {
                    s.update(cx, |s, cx| s.mouse_drag(e.position, cx));
                }
            });
            let s = self.state.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble {
                    s.update(cx, |s, cx| {
                        if s.selecting {
                            s.selecting = false;
                            cx.notify();
                        }
                    });
                }
            });
        }
    }
}
