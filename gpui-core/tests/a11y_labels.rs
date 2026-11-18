//! Every interactive node a component draws tells assistive technology what it is and what it's
//! called: a role (not a bare container) and a label. Each component is drawn as a host writes
//! it: with its own words where it has some (a button's label, a tab's name), and with
//! `accessibility_label` where it has none (an icon button, a switch, a slider), as their docs
//! ask. While it draws, gpui-core records what each control (each `Press`, and the controls
//! that aren't one) says; anything without a role or a label fails here, nested parts included
//! (a dialog's close button, a carousel's arrows, a pagination's pages).
//!
//! GPUI's test windows never build the accessibility tree (no assistive technology connects to
//! them), so the controls report themselves instead.

use gpui::{
    AnyElement, AppContext as _, Context, IntoElement, ParentElement as _, Render, Role,
    SharedString, Styled as _, TestAppContext, Window, div, px, size,
};
use gpui_core::*;

type Make = Box<dyn Fn(&mut Window, &mut gpui::App) -> AnyElement>;

fn cases() -> Vec<(&'static str, Make)> {
    macro_rules! case {
        ($name:literal, |$w:ident, $c:ident| $make:expr) => {
            (
                $name,
                Box::new(|$w: &mut Window, $c: &mut gpui::App| {
                    let _ = (&$w, &$c);
                    $make.into_any_element()
                }) as Make,
            )
        };
    }
    vec![
        // Words of their own.
        case!("Button", |w, c| Button::new("b").label("Save")),
        case!("ButtonGroup", |w, c| ButtonGroup::new("g")
            .child(Button::new("a").label("Day"))
            .child(Button::new("b").label("Week"))),
        case!("Checkbox", |w, c| Checkbox::new("cb").label("Remember me")),
        case!("Toggle", |w, c| Toggle::new("t").label("Bold")),
        case!("Collapsible", |w, c| Collapsible::new("col")
            .label("Details")
            .child("More")),
        case!("Badge, pressable", |w, c| Badge::new("New")
            .on_click(|_, _, _| {})),
        case!("Accordion", |w, c| Accordion::new("acc")
            .item(AccordionItem::new("a").title("Shipping").child("Free"))),
        case!("Item, pressable", |w, c| Item::new("it")
            .title("Ana Lima")
            .on_click(|_, _, _| {})),
        case!("Dialog, open", |w, c| Dialog::new("d")
            .open(true)
            .title("Edit profile")
            .child("Body")),
        case!("Drawer, open", |w, c| Drawer::new("dr")
            .open(true)
            .title("Move goal")
            .child("Body")
            .footer(Button::new("submit").label("Submit"))),
        // Words the host gives.
        case!("Button, icon only", |w, c| Button::new("b")
            .icon(IconName::Star)
            .accessibility_label("Star it")),
        case!("Toggle, icon only", |w, c| Toggle::new("t")
            .icon(IconName::Italic)
            .accessibility_label("Italic")),
        case!("Checkbox, no words", |w, c| Checkbox::new("cb")
            .accessibility_label("Select row")),
        case!("Switch", |w, c| Switch::new("sw")
            .accessibility_label("Wi-Fi")),
        case!("Select", |w, c| Select::new("s")
            .option("a", "Apple")
            .accessibility_label("Fruit")),
        case!("Combobox", |w, c| Combobox::new("co")
            .option("a", "Alpha")
            .accessibility_label("Project")),
        case!("SegmentedControl", |w, c| SegmentedControl::new("seg")
            .segment("a", "List")
            .segment("b", "Grid")
            .accessibility_label("View")),
        case!("RadioGroup", |w, c| RadioGroup::new("r")
            .option("a", "Free")
            .option("b", "Pro")
            .accessibility_label("Plan")),
        case!("ToggleGroup", |w, c| ToggleGroup::new("tg")
            .item("l", "Left")
            .item("r", "Right")
            .accessibility_label("Align")),
        case!("Tabs", |w, c| Tabs::new("tabs")
            .tab(Tab::new("a", "Account"))
            .tab(Tab::new("b", "Password"))
            .accessibility_label("Settings")),
        case!("Pagination", |w, c| Pagination::new("p")
            .total(5)
            .page(2)
            .accessibility_label("Pages")),
        case!("Carousel", |w, c| Carousel::new("car")
            .child(div().child("One"))
            .child(div().child("Two"))
            .accessibility_label("Photos")),
        case!("Breadcrumb", |w, c| Breadcrumb::new("bc")
            .item(BreadcrumbItem::new("h", "Home"))
            .item(BreadcrumbItem::new("d", "Docs"))),
        case!("DropdownMenu", |w, c| DropdownMenu::new("dm")
            .trigger(Button::new("open").label("Account"))
            .item(PopupMenuItem::new("a", "Log out"))
            .accessibility_label("Account")),
        case!("NavigationMenu", |w, c| NavigationMenu::new("nav")
            .item(NavigationMenuItem::link("docs", "Docs"))
            .accessibility_label("Site")),
        case!("Menubar", |w, c| Menubar::new("mb")
            .menu(MenubarMenu::new(
                "file",
                "File",
                PopupMenu::new("fm").item(PopupMenuItem::new("n", "New")),
            ))
            .accessibility_label("Menu bar")),
        case!("SplitView", |w, c| SplitView::new("sv")
            .pane(Pane::new(div().child("Side")).accessibility_label("Sidebar"))
            .pane(Pane::new(div().child("Main")))),
        case!("ScrollArea, a Tab stop", |w, c| ScrollArea::new("sa")
            .tab_stop(true)
            .accessibility_label("Log")
            .child("Line")),
        case!("SearchList", |w, c| SearchList::new("sl")
            .accessibility_label("Passwords")),
        case!("Slider", |w, c| {
            let s = c.new(|_| SliderState::new());
            Slider::new(&s).accessibility_label("Volume")
        }),
        case!("DatePicker", |w, c| {
            let s = c.new(|cx| DatePickerState::new(w, cx));
            DatePicker::new(&s).accessibility_label("Due date")
        }),
        case!("MonthGrid", |w, c| {
            let s = c.new(|cx| DatePickerState::new(w, cx));
            MonthGrid::new(&s)
        }),
        case!("Input", |w, c| {
            let s = c.new(|cx| InputState::new(w, cx));
            Input::new(&s).accessibility_label("Name")
        }),
        case!("InputGroup", |w, c| {
            let s = c.new(|cx| InputState::new(w, cx));
            InputGroup::new(&s)
                .leading_icon(IconName::Search)
                .accessibility_label("Search")
        }),
        case!("InputOtp", |w, c| {
            let s = c.new(|cx| InputOtpState::new(w, cx));
            InputOtp::new(&s).accessibility_label("Code")
        }),
    ]
}

struct Shown(Make);

impl Render for Shown {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child((self.0)(window, cx))
    }
}

#[gpui::test]
fn every_interactive_node_has_a_role_and_a_label(cx: &mut TestAppContext) {
    cx.update(gpui_core::init);
    // A control drawn twice (each frame) is one problem.
    let mut bad = std::collections::BTreeSet::new();
    for (name, make) in cases() {
        // Each in a window of its own, as the window's content.
        gpui_core::audit_start();
        let (_, vcx) =
            cx.add_window_view(move |window, cx| Root::new(cx.new(|_| Shown(make)), window, cx));
        vcx.simulate_resize(size(px(800.), px(600.)));
        vcx.run_until_parked();
        let said = gpui_core::audit_take();
        if said.is_empty() {
            bad.insert(format!(
                "{name}: drew no control at all: it should record what its controls say"
            ));
        }
        for a in said {
            let no_role = matches!(a.role, Role::Unknown | Role::GenericContainer);
            let label = a.label.as_ref().map(SharedString::as_ref).unwrap_or("");
            if no_role || label.trim().is_empty() {
                bad.insert(format!(
                    "{name}: a {} with role {:?} and label {:?}: every control needs a role and \
                     a label (its words, or the host's accessibility_label)",
                    a.what, a.role, a.label
                ));
            }
        }
    }
    let bad: Vec<String> = bad.into_iter().collect();
    assert!(
        bad.is_empty(),
        "{} problems:\n  {}",
        bad.len(),
        bad.join("\n  ")
    );
}
