//! A command palette (the web's `CommandDialog`): a [`Dialog`] holding a search field over
//! grouped commands, each with an icon and a shortcut's text, filtered as you type.
//!
//! It's a [`Picker`] in a dialog: typing filters the commands (every word, case aside), a
//! group's heading shows while any of its commands match, Up and Down move the highlight while
//! the field keeps the keyboard, and Enter or a press runs the highlighted one through
//! [`CommandDialog::on_select`]. Escape and a press outside call [`CommandDialog::on_dismiss`].
//!
//! ```no_run
//! # use gpui::*;
//! # use gpui_core::{CommandDialog, IconName, MenuSelect, PopupMenuItem};
//! # struct Panel { open: bool }
//! # impl Render for Panel {
//! fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
//!     div().size_full().child(
//!         CommandDialog::new("commands")
//!             .open(self.open)
//!             .on_dismiss(cx.listener(|this, _, _, cx| {
//!                 this.open = false;
//!                 cx.notify();
//!             }))
//!             .group(
//!                 "Suggestions",
//!                 [PopupMenuItem::new("calendar", "Calendar").icon(IconName::Calendar)],
//!             )
//!             .group(
//!                 "Settings",
//!                 [PopupMenuItem::new("profile", "Profile").shortcut("⌘P")],
//!             )
//!             .on_select(cx.listener(|this, pick: &MenuSelect, _, cx| {
//!                 this.open = false;
//!                 cx.notify();
//!             })),
//!     )
//! }
//! # }
//! ```

use std::rc::Rc;

use gpui::{
    App, ElementId, IntoElement, ParentElement as _, Rems, RenderOnce, SharedString, Window, rems,
};

use crate::base::{DismissReason, MenuSelect, Picker, PopupMenuItem};
use crate::dialog::Dialog;

/// Runs on a dismiss, with why.
type DismissFn = Rc<dyn Fn(&DismissReason, &mut Window, &mut App)>;
/// Runs with a pick.
type OnSelect = Rc<dyn Fn(&MenuSelect, &mut Window, &mut App)>;
/// Runs with what's typed.
type OnQueryChange = Rc<dyn Fn(&str, &mut App)>;

/// A size in the web's CSS pixels, as rems of its 15 pixel text.
fn css(v: f32) -> Rems {
    rems(v / 15.)
}

/// A command palette: a [`Dialog`] with a search field over grouped
/// commands. It opens with the keyboard in the field and the first command highlighted, and
/// gives the keyboard back when it closes; the host closes it from
/// [`on_select`](CommandDialog::on_select) and [`on_dismiss`](CommandDialog::on_dismiss).
///
/// Commands are [`PopupMenuItem`]s: give each an [`icon`](PopupMenuItem::icon) and a
/// [`shortcut`](PopupMenuItem::shortcut)'s text (it's shown, not bound: the host binds its
/// keys). To assistive tech it's a dialog named by its
/// [`accessibility_label`](CommandDialog::accessibility_label), holding a list box.
#[derive(IntoElement)]
pub struct CommandDialog {
    id: ElementId,
    open: bool,
    on_dismiss: Option<DismissFn>,
    on_select: Option<OnSelect>,
    /// Each group: its heading, its commands, and whether they're found ones (not filtered).
    groups: Vec<(SharedString, Vec<PopupMenuItem>, bool)>,
    on_query_change: Option<OnQueryChange>,
    placeholder: SharedString,
    empty: SharedString,
    label: SharedString,
    max_width: Rems,
    touch: bool,
}

impl CommandDialog {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            open: false,
            on_dismiss: None,
            on_select: None,
            groups: Vec::new(),
            on_query_change: None,
            placeholder: "Type a command or search…".into(),
            empty: "No results found.".into(),
            label: "Command palette".into(),
            // The web's `max-w-lg`.
            max_width: css(512.),
            touch: false,
        }
    }

    pub fn open(mut self, open: bool) -> Self {
        self.open = open;
        self
    }

    /// A group of commands under a heading, shown while any of them match.
    pub fn group(
        mut self,
        heading: impl Into<SharedString>,
        commands: impl IntoIterator<Item = PopupMenuItem>,
    ) -> Self {
        self.groups
            .push((heading.into(), commands.into_iter().collect(), false));
        self
    }

    /// A group of what the host found for what's typed (docs, people, a search by meaning),
    /// under a heading: shown as given, since typing doesn't filter them. The host searches
    /// from [`on_query_change`](Self::on_query_change).
    pub fn found(
        mut self,
        heading: impl Into<SharedString>,
        items: impl IntoIterator<Item = PopupMenuItem>,
    ) -> Self {
        self.groups
            .push((heading.into(), items.into_iter().collect(), true));
        self
    }

    /// Told what's typed each time it changes, for a host that searches for it.
    pub fn on_query_change(mut self, f: impl Fn(&str, &mut App) + 'static) -> Self {
        self.on_query_change = Some(Rc::new(f));
        self
    }

    /// Runs with the command picked (by Enter or a press). The host runs it and closes the
    /// palette here.
    pub fn on_select(mut self, f: impl Fn(&MenuSelect, &mut Window, &mut App) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// Runs on Escape and on a press outside. The host closes it here.
    pub fn on_dismiss(
        mut self,
        f: impl Fn(&DismissReason, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_dismiss = Some(Rc::new(f));
        self
    }

    /// The field's placeholder ("Type a command or search…" unless given).
    pub fn placeholder(mut self, text: impl Into<SharedString>) -> Self {
        self.placeholder = text.into();
        self
    }

    /// What it shows when nothing matches ("No results found." unless given).
    pub fn empty_text(mut self, text: impl Into<SharedString>) -> Self {
        self.empty = text.into();
        self
    }

    /// What it's called to assistive tech ("Command palette" unless given).
    pub fn accessibility_label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    /// The widest it draws (the web's 32 rem by default).
    pub fn max_width(mut self, width: Rems) -> Self {
        self.max_width = width;
        self
    }

    /// Commands 44 points tall, for touch.
    pub fn touch(mut self, touch: bool) -> Self {
        self.touch = touch;
        self
    }
}

impl RenderOnce for CommandDialog {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut dialog = Dialog::new(self.id.clone())
            .open(self.open)
            .close_button(false)
            .accessibility_label(self.label)
            .max_width(self.max_width)
            .bare();
        if let Some(f) = self.on_dismiss {
            dialog = dialog.on_dismiss(move |reason, window, cx| f(reason, window, cx));
        }
        if !self.open {
            return dialog;
        }
        let mut picker = Picker::new(ElementId::NamedChild(
            std::sync::Arc::new(self.id),
            "picker".into(),
        ))
        .placeholder(self.placeholder)
        .empty_text(self.empty)
        .touch(self.touch);
        for (heading, commands, found) in self.groups {
            picker = picker.heading(heading);
            picker = if found {
                picker.found(commands)
            } else {
                picker.items(commands)
            };
        }
        if let Some(f) = self.on_query_change {
            picker = picker.on_query_change(move |text, cx| f(text, cx));
        }
        if let Some(f) = self.on_select {
            picker = picker.on_select(move |pick, window, cx| f(pick, window, cx));
        }
        dialog.child(picker)
    }
}
