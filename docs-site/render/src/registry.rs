//! Every component on the site, and its variants. To add a variant, write its demo file and
//! add one line to its component's `variants`; to add a component, add a `component!` block.
//! See ../../README.md.
//!
//! A variant is `demo("Name", "slug")` or `view("Name", "slug")`, drawn from
//! `docs-site/demos/<component slug>/<variant slug>.rs`:
//!
//! - `demo`: the file is one expression, of a type that is `IntoElement`, with
//!   `window: &mut Window` and `cx: &mut App` in scope.
//! - `view`: the file defines `pub struct Example`, with
//!   `pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self` and `impl Render`, for a
//!   demo with state of its own (an `InputState`, a toggle that turns).
//!
//! Both are compiled with `use gpui::*; use gpui_core::*;` and nothing else, so they use only
//! what those crates export.
//!
//! A variant ending in `.click()`, `.right_click()` or `.hover()` has the pointer do that in the
//! middle of what the demo drew before its picture is taken, to show a menu, a hover card or a
//! tooltip open: `view("Default", "default").click()`.

use gpui::{AnyElement, AnyView, App, AppContext as _, Window};

use crate::site::{Act, Component, Crate, Demo, Family, Variant};

/// What the pointer does before a variant's picture: none, `click`, `right_click` or `hover`.
macro_rules! act {
    () => {
        Act::None
    };
    (click) => {
        Act::Click
    };
    (right_click) => {
        Act::RightClick
    };
    (hover) => {
        Act::Hover
    };
}

/// A demo file's path, from this crate's manifest.
macro_rules! demo_path {
    ($component:literal, $variant:literal) => {
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../demos/",
            $component,
            "/",
            $variant,
            ".rs"
        )
    };
}

/// One variant of the component `$component` (its slug).
macro_rules! variant {
    (@demo $component:literal, $name:literal, $slug:literal, $act:expr) => {
        Variant {
            name: $name,
            slug: $slug,
            code: include_str!(demo_path!($component, $slug)),
            act: $act,
            // The file wrapped in `fn render(window, cx)` by build.rs.
            demo: Demo::Element(include!(concat!(
                env!("OUT_DIR"),
                "/demos/",
                $component,
                "/",
                $slug,
                ".rs"
            ))),
        }
    };
    (@view $component:literal, $name:literal, $slug:literal, $act:expr) => {
        Variant {
            name: $name,
            slug: $slug,
            code: include_str!(demo_path!($component, $slug)),
            act: $act,
            demo: Demo::View({
                mod demo {
                    #[allow(unused_imports)]
                    use gpui::*;
                    #[allow(unused_imports)]
                    use gpui_core::*;
                    include!(demo_path!($component, $slug));
                }
                fn build(window: &mut Window, cx: &mut App) -> AnyView {
                    cx.new(|cx| demo::Example::new(window, cx)).into()
                }
                build
            }),
        }
    };
}

/// A component and its variants, in the order they show on its page.
macro_rules! component {
    (
        family: $family:ident,
        name: $name:literal,
        slug: $slug:literal,
        about: $about:literal,
        docs: $docs:literal,
        variants: [
            $($kind:ident($vname:literal, $vslug:literal) $(. $act:ident())?),* $(,)?
        ] $(,)?
    ) => {
        Component {
            family: Family::$family,
            name: $name,
            slug: $slug,
            about: $about,
            docs: $docs,
            variants: vec![$(variant!(@$kind $slug, $vname, $vslug, act!($($act)?))),*],
        }
    };
}

/// The site's components. The index and the sidebar sort them by family, then by name.
pub fn components() -> Vec<Component> {
    vec![
        component! {
            family: Forms,
            name: "Button",
            slug: "button",
            about: "Starts an action: a label, an icon or both, in six variants and five sizes.",
            docs: "gpui_core/struct.Button.html",
            variants: [
                demo("Default", "default"),
                demo("Secondary", "secondary"),
                demo("Outline", "outline"),
                demo("Ghost", "ghost"),
                demo("Destructive", "destructive"),
                demo("Link", "link"),
                demo("Sizes", "sizes"),
                demo("With an icon", "with-icon"),
                demo("Disabled", "disabled"),
            ],
        },
        component! {
            family: Forms,
            name: "Checkbox",
            slug: "checkbox",
            about: "A box to check, with its label: checked, unchecked or partly (a dash).",
            docs: "gpui_core/struct.Checkbox.html",
            variants: [
                demo("Default", "default"),
                view("Select all", "select-all"),
                demo("Disabled", "disabled"),
                demo("Invalid", "invalid"),
            ],
        },
        component! {
            family: Forms,
            name: "Switch",
            slug: "switch",
            about: "An on/off switch whose thumb slides between the two.",
            docs: "gpui_core/struct.Switch.html",
            variants: [
                demo("Default", "default"),
                demo("With a label", "with-label"),
                demo("Disabled", "disabled"),
                view("Interactive", "interactive"),
            ],
        },
        component! {
            family: Forms,
            name: "Input",
            slug: "input",
            about: "A text field of one line or several, its text kept in an InputState.",
            docs: "gpui_core/input/struct.Input.html",
            variants: [
                view("Default", "default"),
                view("Password", "password"),
                view("Textarea", "textarea"),
            ],
        },
        component! {
            family: Forms,
            name: "Input Group",
            slug: "input-group",
            about: "A text field with icons, text or buttons inside its border, before or after.",
            docs: "gpui_core/struct.InputGroup.html",
            variants: [
                view("With an icon", "icon"),
                view("Text either side", "text"),
                view("With a button", "button"),
                view("Invalid", "invalid"),
            ],
        },
        component! {
            family: Forms,
            name: "Input OTP",
            slug: "input-otp",
            about: "A one-time code, a box per character, filled by typing or pasting.",
            docs: "gpui_core/struct.InputOtp.html",
            variants: [
                view("Default", "default"),
                view("Letters and digits", "letters"),
                view("Invalid", "invalid"),
            ],
        },
        component! {
            family: Forms,
            name: "Radio Group",
            slug: "radio-group",
            about: "One choice of several, each option a circle with its label.",
            docs: "gpui_core/struct.RadioGroup.html",
            variants: [
                view("Default", "default"),
                demo("Horizontal", "horizontal"),
                demo("Disabled", "disabled"),
                demo("Invalid", "invalid"),
            ],
        },
        component! {
            family: Forms,
            name: "Select",
            slug: "select",
            about: "A button that opens a list of options, to pick one.",
            docs: "gpui_core/struct.Select.html",
            variants: [
                view("Default", "default"),
                demo("Groups", "groups"),
                demo("Open", "open").click(),
                demo("Disabled", "disabled"),
                demo("Invalid", "invalid"),
            ],
        },
        component! {
            family: Forms,
            name: "Slider",
            slug: "slider",
            about: "A value along a track, set by dragging or with the arrow keys.",
            docs: "gpui_core/struct.Slider.html",
            variants: [
                view("Default", "default"),
                view("With its value", "value"),
            ],
        },
        component! {
            family: Forms,
            name: "Toggle",
            slug: "toggle",
            about: "A button that stays pressed until pressed again.",
            docs: "gpui_core/struct.Toggle.html",
            variants: [
                view("Default", "default"),
                demo("Outline", "outline"),
                demo("Sizes", "sizes"),
                demo("Disabled", "disabled"),
            ],
        },
        component! {
            family: Forms,
            name: "Toggle Group",
            slug: "toggle-group",
            about: "Toggles in a row: at most one on, or several.",
            docs: "gpui_core/struct.ToggleGroup.html",
            variants: [
                view("One at most", "single"),
                demo("Several", "multiple"),
                demo("Disabled", "disabled"),
            ],
        },
        component! {
            family: Forms,
            name: "Segmented Control",
            slug: "segmented-control",
            about: "One of a few choices, as segments joined in a row.",
            docs: "gpui_core/struct.SegmentedControl.html",
            variants: [
                view("Default", "default"),
                demo("With icons", "icons"),
                demo("Disabled", "disabled"),
            ],
        },
        component! {
            family: Forms,
            name: "Date Picker",
            slug: "date-picker",
            about: "A day, or a range of days, picked from a month.",
            docs: "gpui_core/struct.DatePicker.html",
            variants: [
                view("Default", "default"),
                view("Open", "open").click(),
                view("Month", "month"),
            ],
        },
        component! {
            family: Forms,
            name: "Field",
            slug: "field",
            about: "A control with its label, a line of help or an error, and sets of them.",
            docs: "gpui_core/struct.Field.html",
            variants: [
                view("Default", "default"),
                view("With an error", "error"),
                view("Field set", "fieldset"),
            ],
        },
        component! {
            family: Overlays,
            name: "Dialog",
            slug: "dialog",
            about: "A modal panel in the middle of the window, over a scrim.",
            docs: "gpui_core/struct.Dialog.html",
            variants: [view("Default", "default").click()],
        },
        component! {
            family: Overlays,
            name: "Alert Dialog",
            slug: "alert-dialog",
            about: "A dialog asking for a decision, which a press outside doesn't close.",
            docs: "gpui_core/struct.AlertDialog.html",
            variants: [view("Default", "default").click()],
        },
        component! {
            family: Overlays,
            name: "Popover",
            slug: "popover",
            about: "A panel hanging from what opens it, floating over everything else.",
            docs: "gpui_core/struct.Popover.html",
            variants: [view("Default", "default").click()],
        },
        component! {
            family: Overlays,
            name: "Hover Card",
            slug: "hover-card",
            about: "More about something, shown while the pointer rests on it.",
            docs: "gpui_core/struct.HoverCard.html",
            variants: [demo("Default", "default").hover()],
        },
        component! {
            family: Overlays,
            name: "Drawer",
            slug: "drawer",
            about: "A modal panel from an edge of the window, with a header and a footer.",
            docs: "gpui_core/struct.Drawer.html",
            variants: [
                view("Default", "default").click(),
                view("From the right", "right").click(),
            ],
        },
        component! {
            family: Overlays,
            name: "Sheet",
            slug: "sheet",
            about: "A plain panel of rows across the bottom of the window, for touch.",
            docs: "gpui_core/struct.Sheet.html",
            variants: [view("Default", "default").click()],
        },
        component! {
            family: Overlays,
            name: "Tooltip",
            slug: "tooltip",
            about: "A few words shown while the pointer rests on what they explain.",
            docs: "gpui_core/struct.Tooltip.html",
            variants: [demo("Default", "default").hover()],
        },
        component! {
            family: Overlays,
            name: "Toast",
            slug: "toast",
            about: "A short note at the bottom of the window that goes by itself.",
            docs: "gpui_core/struct.Toast.html",
            variants: [
                demo("Default", "default").click(),
                demo("With an action", "action").click(),
            ],
        },
        component! {
            family: Overlays,
            name: "Command Dialog",
            slug: "command-dialog",
            about: "A command palette: a search over grouped commands, in a dialog.",
            docs: "gpui_core/struct.CommandDialog.html",
            variants: [view("Default", "default").click()],
        },
        component! {
            family: Menus,
            name: "Dropdown Menu",
            slug: "dropdown-menu",
            about: "A button that opens a menu of actions hung from it.",
            docs: "gpui_core/struct.DropdownMenu.html",
            variants: [
                demo("Default", "default").click(),
                view("Checks and a submenu", "checks").click(),
            ],
        },
        component! {
            family: Menus,
            name: "Context Menu",
            slug: "context-menu",
            about: "A menu that opens where something is right-clicked or held.",
            docs: "gpui_core/struct.ContextMenu.html",
            variants: [demo("Default", "default").right_click()],
        },
        component! {
            family: Menus,
            name: "Menubar",
            slug: "menubar",
            about: "A row of menus, as at the top of a desktop app's window.",
            docs: "gpui_core/struct.Menubar.html",
            variants: [demo("Default", "default").click()],
        },
        component! {
            family: Menus,
            name: "Combobox",
            slug: "combobox",
            about: "A button that opens a search over its options, to pick one.",
            docs: "gpui_core/struct.Combobox.html",
            variants: [view("Default", "default").click()],
        },
        component! {
            family: Charts,
            name: "Bar Chart",
            slug: "bar-chart",
            about: "Values as bars by category: side by side, stacked, or below zero.",
            docs: "gpui_core/chart/struct.BarChart.html",
            variants: [
                demo("Default", "default"),
                demo("Stacked", "stacked"),
                demo("Negative", "negative"),
            ],
        },
        component! {
            family: Charts,
            name: "Line Chart",
            slug: "line-chart",
            about: "Values as lines across categories, smooth or straight.",
            docs: "gpui_core/chart/struct.LineChart.html",
            variants: [
                demo("Default", "default"),
                demo("Linear, with dots", "linear"),
            ],
        },
        component! {
            family: Charts,
            name: "Area Chart",
            slug: "area-chart",
            about: "Values as filled areas, stacked or not, flat or with a gradient.",
            docs: "gpui_core/chart/struct.AreaChart.html",
            variants: [
                demo("Default", "default"),
                demo("Own values", "values"),
            ],
        },
        component! {
            family: Charts,
            name: "Pie Chart",
            slug: "pie-chart",
            about: "Slices of a whole, as a pie or a donut.",
            docs: "gpui_core/chart/struct.PieChart.html",
            variants: [
                demo("Default", "default"),
                demo("Donut", "donut"),
            ],
        },
        component! {
            family: Scrolling,
            name: "Scroll Area",
            slug: "scroll-area",
            about: "A box that scrolls, with thin bars that show while it's in use.",
            docs: "gpui_core/struct.ScrollArea.html",
            variants: [
                demo("Default", "default").hover(),
                demo("Horizontal", "horizontal").hover(),
            ],
        },
        component! {
            family: Scrolling,
            name: "Carousel",
            slug: "carousel",
            about: "Slides one or several at a time, with buttons and dots.",
            docs: "gpui_core/struct.Carousel.html",
            variants: [
                demo("Default", "default"),
                demo("Several in view", "several"),
            ],
        },
        component! {
            family: Navigation,
            name: "Tabs",
            slug: "tabs",
            about: "Panels shown one at a time, chosen from a row of tabs.",
            docs: "gpui_core/struct.Tabs.html",
            variants: [
                view("Default", "default"),
                demo("Line", "line"),
            ],
        },
        component! {
            family: Navigation,
            name: "Roving Focus",
            slug: "roving-focus",
            about: "Rows that share one Tab stop, the arrow keys moving between them: a list, tiles or a tree.",
            docs: "gpui_core/struct.RovingFocus.html",
            variants: [
                view("List", "list"),
                demo("Tiles", "tiles"),
                view("Tree", "tree"),
            ],
        },
        component! {
            family: Navigation,
            name: "Breadcrumb",
            slug: "breadcrumb",
            about: "Where a page sits, from the top down, each place a link.",
            docs: "gpui_core/struct.Breadcrumb.html",
            variants: [
                demo("Default", "default"),
                demo("Collapsed", "collapsed"),
            ],
        },
        component! {
            family: Navigation,
            name: "Pagination",
            slug: "pagination",
            about: "Pages by number, with Previous and Next.",
            docs: "gpui_core/struct.Pagination.html",
            variants: [
                view("Default", "default"),
                demo("Compact", "compact"),
            ],
        },
        component! {
            family: Navigation,
            name: "Navigation Menu",
            slug: "navigation-menu",
            about: "A site's top links, some opening a panel of links.",
            docs: "gpui_core/struct.NavigationMenu.html",
            variants: [demo("Default", "default").click()],
        },
        component! {
            family: Navigation,
            name: "Sidebar",
            slug: "sidebar",
            about: "An app's side navigation, collapsible to icons.",
            docs: "gpui_core/struct.Sidebar.html",
            variants: [view("Default", "default")],
        },
        component! {
            family: Navigation,
            name: "Split View",
            slug: "split-view",
            about: "Panes side by side, resized by dragging the handles between them.",
            docs: "gpui_core/struct.SplitView.html",
            variants: [view("Default", "default")],
        },
        component! {
            family: Containers,
            name: "Card",
            slug: "card",
            about: "A raised panel with a header, content and a footer, each drawn only when given.",
            docs: "gpui_core/struct.Card.html",
            variants: [
                demo("Default", "default"),
                demo("Small", "small"),
                demo("Content only", "content-only"),
            ],
        },
        component! {
            family: Display,
            name: "Badge",
            slug: "badge",
            about: "A small pill of text, perhaps with an icon, in four looks.",
            docs: "gpui_core/struct.Badge.html",
            variants: [
                demo("Default", "default"),
                demo("Secondary", "secondary"),
                demo("Destructive", "destructive"),
                demo("Outline", "outline"),
                demo("With an icon", "with-icon"),
            ],
        },
        component! {
            family: Containers,
            name: "Accordion",
            slug: "accordion",
            about: "Sections that open under their titles, one at a time or several.",
            docs: "gpui_core/struct.Accordion.html",
            variants: [demo("Default", "default")],
        },
        component! {
            family: Containers,
            name: "Collapsible",
            slug: "collapsible",
            about: "A label that shows or hides what's under it.",
            docs: "gpui_core/struct.Collapsible.html",
            variants: [demo("Default", "default")],
        },
        component! {
            family: Containers,
            name: "Item",
            slug: "item",
            about: "A row with an icon, a title, a description and an action.",
            docs: "gpui_core/struct.Item.html",
            variants: [demo("Default", "default")],
        },
        component! {
            family: Containers,
            name: "Aspect Ratio",
            slug: "aspect-ratio",
            about: "A box that keeps its width-to-height ratio.",
            docs: "gpui_core/struct.AspectRatio.html",
            variants: [demo("Default", "default")],
        },
        component! {
            family: Display,
            name: "Alert",
            slug: "alert",
            about: "A note in the page that calls for attention.",
            docs: "gpui_core/struct.Alert.html",
            variants: [
                demo("Default", "default"),
                demo("Destructive", "destructive"),
            ],
        },
        component! {
            family: Display,
            name: "Avatar",
            slug: "avatar",
            about: "A picture of someone, or their initials, alone or in a group.",
            docs: "gpui_core/struct.Avatar.html",
            variants: [
                demo("Default", "default"),
                demo("Group", "group"),
            ],
        },
        component! {
            family: Display,
            name: "Kbd",
            slug: "kbd",
            about: "A key, or keys pressed together, as printed on the keyboard.",
            docs: "gpui_core/struct.Kbd.html",
            variants: [demo("Default", "default")],
        },
        component! {
            family: Display,
            name: "Separator",
            slug: "separator",
            about: "A thin line between things, across or down.",
            docs: "gpui_core/struct.Separator.html",
            variants: [demo("Default", "default")],
        },
        component! {
            family: Display,
            name: "Skeleton",
            slug: "skeleton",
            about: "Grey shapes standing in for what's loading.",
            docs: "gpui_core/struct.Skeleton.html",
            variants: [demo("Default", "default")],
        },
        component! {
            family: Display,
            name: "Spinner",
            slug: "spinner",
            about: "A turning ring: something is happening.",
            docs: "gpui_core/struct.Spinner.html",
            variants: [demo("Default", "default")],
        },
        component! {
            family: Display,
            name: "Progress",
            slug: "progress",
            about: "How far along something is.",
            docs: "gpui_core/struct.Progress.html",
            variants: [demo("Default", "default")],
        },
        component! {
            family: Display,
            name: "Empty State",
            slug: "empty-state",
            about: "What a place shows with nothing in it yet.",
            docs: "gpui_core/struct.EmptyState.html",
            variants: [demo("Default", "default")],
        },
        component! {
            family: Display,
            name: "Table",
            slug: "table",
            about: "Rows under column headings, drawn only as far as they're seen.",
            docs: "gpui_core/struct.Table.html",
            variants: [view("Default", "default")],
        },
        component! {
            family: Display,
            name: "Text View",
            slug: "text-view",
            about: "Markdown drawn as selectable text.",
            docs: "gpui_core/text/struct.TextView.html",
            variants: [demo("Default", "default")],
        },
    ]
}

/// The crates built on gpui-core, each with pictures already in its `docs/` folder (the ones
/// its README shows), in the order their pages show them.
pub fn crates() -> Vec<Crate> {
    vec![
        Crate {
            name: "gpui-blocks",
            slug: "blocks",
            dir: "gpui-blocks",
            lib: "gpui_blocks",
            about: "A block editor: Markdown documents with a slash menu, drag handles, callouts, toggles and widget blocks.",
            pictures: vec![
                (
                    "doc.png",
                    "A document: headings, a to-do list, a numbered list, callouts and toggles",
                ),
                ("slash.png", "The slash menu open under a \"/\""),
                ("hover.png", "\"+\" and a grip beside a to-do list"),
                (
                    "drag.png",
                    "A heading carried down past a to-do list, which slides up to make room",
                ),
            ],
        },
        Crate {
            name: "gpui-calendar",
            slug: "calendar",
            dir: "gpui-calendar",
            lib: "gpui_calendar",
            about: "Day, week, month, year and agenda views with a time grid, dragging, a mini month and time zones.",
            pictures: vec![
                (
                    "week.png",
                    "A week: all-day spans, overlapping events side by side, and the line at the current time",
                ),
                (
                    "month.png",
                    "A month with spans, dotted rows and \"+N more\"",
                ),
                (
                    "year.png",
                    "The year as twelve small months, busy days shaded",
                ),
                (
                    "drag-move.png",
                    "An event carried to another day, the original faded",
                ),
                (
                    "agenda.png",
                    "A day's events listed with their times, colours and links",
                ),
                (
                    "compact.png",
                    "A day at 390 pixels wide, with the compact header",
                ),
            ],
        },
        Crate {
            name: "gpui-agents",
            slug: "agents",
            dir: "gpui-agents",
            lib: "gpui_agents",
            about: "Draws AI agents' OpenUI cards natively: cards, tables, charts, forms and buttons.",
            pictures: vec![
                (
                    "cards-kpi.png",
                    "A card of account metrics: snippet cards with icons, values and trends, and a product card with a bar chart",
                ),
                (
                    "form.png",
                    "A form with text fields, a select, a date picker, a slider, checkboxes, radios, chips and option cards",
                ),
                (
                    "charts.png",
                    "Grouped bars with a negative value, and a horizontal bar chart",
                ),
                (
                    "pies.png",
                    "A pie, a donut, a half pie, radial rings and a single stacked bar, each with its legend",
                ),
                ("table.png", "A table with tags and buttons in its cells"),
            ],
        },
        Crate {
            name: "gpui-dnd",
            slug: "dnd",
            dir: "gpui-dnd",
            lib: "gpui_dnd",
            about: "Drag and drop in the manner of react-dnd: sources, nested targets, a preview, sortable lists and kanban boards.",
            pictures: vec![
                (
                    "kanban-drag.png",
                    "A kanban board: a card lifted out of To do and held over Doing, where the cards have parted to open a gap for it",
                ),
                (
                    "sortable-drag.png",
                    "A list of tasks with one lifted and held lower, the others parted around the gap, beside a row of tag chips",
                ),
                (
                    "nested-hover.png",
                    "Nested targets: a file held over a folder inside a drive; the folder shows it's under the pointer and the drive that it's inside",
                ),
                (
                    "custom_preview-drag.png",
                    "Two photos picked up together, drawn by a custom layer as a stack with a count, held over an album",
                ),
            ],
        },
        Crate {
            name: "gpui-flow",
            slug: "flow",
            dir: "gpui-flow",
            lib: "gpui_flow",
            about: "A canvas for flowcharts and whiteboards: shaped nodes, edges, groups, ink, controls and a minimap.",
            pictures: vec![(
                "whiteboard.png",
                "A flowchart: shaped nodes, labelled and dotted edges, a group, pen and marker strokes, the controls and the minimap",
            )],
        },
        Crate {
            name: "gpui-tables",
            slug: "tables",
            dir: "gpui-tables",
            lib: "gpui_tables",
            about: "Notion-style database views (table, board and list), with a model and engine that build without GPUI.",
            pictures: vec![
                (
                    "table-people.png",
                    "A table of people: names, companies, roles, stages and tags, sorted by name",
                ),
                (
                    "board.png",
                    "A board with a column for each stage, each card a person with their company, last contact and owner",
                ),
                (
                    "list-grouped.png",
                    "People by stage: a header for each stage with its count, and the people under it",
                ),
                (
                    "table-grouped.png",
                    "Interactions grouped by month, each group with its count and a New row",
                ),
                (
                    "table-date.png",
                    "A date's month under its cell, the day it holds picked and today ringed",
                ),
            ],
        },
    ]
}
