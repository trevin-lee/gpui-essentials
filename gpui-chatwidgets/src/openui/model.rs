//! The card as a typed tree, for a host to draw: one variant per component of the library the
//! web renders with (@openuidev/react-ui's openuiLibrary), props typed and defaulted the way its
//! renderers default them, sub-components (a table's columns, a form's controls, a tab's
//! content) folded into their parents, and every list, text and the tree itself capped. A
//! component this doesn't know draws nothing ([`Component::Unknown`]).

use super::action::{Action, Rules, action};
use super::chart::{self, Bars, Chart, ChartKind, Curve, Scatter, Slices, SlicesKind};
use super::value::{El, Obj, V, num_text};
use super::{
    CardError, MAX_CHILDREN, MAX_COLUMNS, MAX_COMPONENTS, MAX_NESTING, MAX_ROWS, MAX_TEXT,
    MAX_TOTAL_TEXT,
};

/// What one OpenUI message draws.
#[derive(Clone, Debug, PartialEq)]
pub struct Card {
    pub root: Component,
}

/// An image source from a card, kept as data. The crate never loads it: the host's image
/// loader decides whether and how (the renderer shows the alt text otherwise). Kept only when
/// it's 1 to 4096 bytes after trimming, with no control characters; anything else, data: and
/// javascript: included, is passed on as written for the host to refuse.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct ImageSrc(String);

impl ImageSrc {
    pub fn parse(src: &str) -> Option<ImageSrc> {
        let s = src.trim();
        if s.is_empty() || s.len() > 4096 || s.chars().any(char::is_control) {
            return None;
        }
        Some(ImageSrc(s.to_string()))
    }

    /// The source as the card wrote it (trimmed).
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The source as a path and query on exactly `origin` (such as `https://host.example`),
    /// for a host that serves a card's images itself: relative ones resolve against the origin,
    /// absolute ones must be on it (same scheme, host and port), and one with a user name or
    /// password, or of another scheme (data:, blob:, javascript:), is None. The path comes
    /// back normalised ("/a/../b" is "/b"), so check it after this.
    pub fn same_origin(&self, origin: &str) -> Option<String> {
        let base = url::Url::parse(&format!("{}/", origin.trim_end_matches('/'))).ok()?;
        if !matches!(base.scheme(), "http" | "https") {
            return None;
        }
        let u = base.join(&self.0).ok()?;
        if u.origin() != base.origin() || !u.username().is_empty() || u.password().is_some() {
            return None;
        }
        Some(match u.query() {
            Some(q) => format!("{}?{q}", u.path()),
            None => u.path().to_string(),
        })
    }
}

macro_rules! keyword {
    ($(#[$m:meta])* $name:ident { $($variant:ident = $s:literal),+ $(,)? } default $def:ident) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum $name {
            $($variant),+
        }
        impl Default for $name {
            fn default() -> $name {
                $name::$def
            }
        }
        // Not every keyword is read both with and without its default.
        #[allow(dead_code)]
        impl $name {
            fn read(v: &V) -> Option<$name> {
                match v.as_str()? {
                    $($s => Some($name::$variant),)+
                    _ => None,
                }
            }
            fn or_default(v: &V) -> $name {
                Self::read(v).unwrap_or_default()
            }
        }
    };
}

keyword!(Direction { Row = "row", Column = "column" } default Column);
keyword!(Gap { None = "none", Xs = "xs", S = "s", M = "m", L = "l", Xl = "xl", Xxl = "2xl" } default M);
keyword!(Align { Start = "start", Center = "center", End = "end", Stretch = "stretch", Baseline = "baseline" } default Stretch);
keyword!(Justify { Start = "start", Center = "center", End = "end", Between = "between", Around = "around", Evenly = "evenly" } default Start);
keyword!(
    /// A container's look: raised (card), recessed (sunk) or none (clear).
    Surface { Card = "card", Sunk = "sunk", Clear = "clear" } default Card
);
keyword!(TextSize { Default = "default", Small = "small", Large = "large", SmallHeavy = "small-heavy", LargeHeavy = "large-heavy" } default Default);
keyword!(
    /// A callout's, tag's or icon's colour.
    Tone { Neutral = "neutral", Info = "info", Success = "success", Warning = "warning", Error = "error", Danger = "danger", Inverted = "inverted", Filled = "filled", Soft = "soft" } default Neutral
);
keyword!(ButtonVariant { Primary = "primary", Secondary = "secondary", Tertiary = "tertiary", Ghost = "ghost" } default Primary);
keyword!(Size { Medium = "medium", ExtraSmall = "extra-small", Small = "small", Large = "large", Sm = "sm", Md = "md", Lg = "lg", Xs = "xs", Default = "default", S = "s", M = "m", L = "l", Xl = "xl" } default Medium);
keyword!(ColumnKind { String = "string", Number = "number", Action = "action" } default String);
keyword!(InputKind { Text = "text", Email = "email", Password = "password", Number = "number", Url = "url" } default Text);
keyword!(Layout { Grid = "grid", Carousel = "carousel", Horizontal = "horizontal", Vertical = "vertical" } default Grid);

/// A value the user can change: bound to a $variable (its name, with the $), or fixed as
/// written.
#[derive(Clone, Debug, PartialEq)]
pub enum Bound {
    Var(String),
    Fixed(serde_json::Value),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Component {
    Stack(Flex),
    Card(Flex),
    Tabs(Vec<Section>),
    Accordion(Vec<Section>),
    Steps(Vec<StepItem>),
    /// Slides side by side, each a list of components.
    Carousel {
        slides: Vec<Vec<Component>>,
        surface: Surface,
    },
    Separator {
        vertical: bool,
    },
    Modal(Modal),
    Header {
        title: Option<String>,
        subtitle: Option<String>,
    },
    /// Text that may hold Markdown, drawn as text with Markdown's emphasis; never HTML.
    Text {
        markdown: String,
        size: TextSize,
    },
    Markdown {
        markdown: String,
        surface: Surface,
    },
    Callout(Callout),
    Image(Image),
    Gallery(Vec<GalleryImage>),
    Code {
        language: String,
        code: String,
    },
    InlineHeader {
        heading: String,
        description: Option<String>,
    },
    Table(Table),
    EditableTable(EditableTable),
    Chart(Chart),
    Form(Form),
    /// An input on its own, outside a form's control.
    Field(Field),
    Label(String),
    Button(Button),
    Buttons(Buttons),
    IconButton(IconButton),
    TagBlock {
        tags: Vec<String>,
        size: Size,
    },
    Tag(Tag),
    Icon(Icon),
    EntityList(EntityList),
    List(ListBlock),
    Cards(CardBlock),
    /// A Text or BoldText line.
    Line(Line),
    IconText(IconText),
    ImageText(ImageText),
    Metric(Metric),
    /// A bare string or number among children, drawn as plain text.
    Plain(String),
    /// A component this model doesn't draw (a sub-component out of place): nothing.
    Unknown(String),
}

/// Stack and Card: children in a row or a column.
#[derive(Clone, Debug, PartialEq)]
pub struct Flex {
    pub children: Vec<Component>,
    pub direction: Direction,
    pub gap: Gap,
    pub align: Option<Align>,
    pub justify: Option<Justify>,
    pub wrap: bool,
    /// Card only: its look.
    pub surface: Surface,
}

/// A tab or an accordion section.
#[derive(Clone, Debug, PartialEq)]
pub struct Section {
    pub value: String,
    pub title: String,
    pub content: Vec<Component>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct StepItem {
    pub title: String,
    pub details: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Modal {
    pub title: String,
    /// Open when it's true (or "true"); closing sets a bound $variable to false.
    pub open: Bound,
    pub children: Vec<Component>,
    pub size: Size,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Callout {
    pub tone: Tone,
    pub title: Option<String>,
    pub description: Option<String>,
    /// Callout's visible: bound, it hides itself after three seconds by setting it false.
    pub visible: Option<Bound>,
    /// TextCallout: the plainer banner.
    pub plain: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub alt: Option<String>,
    /// None when there was no usable src: draw the alt text instead.
    pub src: Option<ImageSrc>,
    /// ImageBlock: full width with a loading state.
    pub block: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GalleryImage {
    pub src: Option<ImageSrc>,
    pub alt: Option<String>,
    pub details: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Table {
    pub columns: Vec<Column>,
    /// Row by row, one cell per column (the program writes them column by column).
    pub rows: Vec<Vec<Cell>>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Column {
    pub label: String,
    pub kind: ColumnKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Cell {
    Empty,
    Text(String),
    /// Components in a cell (a Tag, a Button made with @Each).
    Components(Vec<Component>),
}

/// Saving writes the rows as `[{ id, values }]` with
/// `Store::set_field(Some(name), Some("EditableTable"), name, rows)`, then presses
/// [`Action::default_words`] with "Save Changes" in the form `name`, as the web's Save Changes does.
#[derive(Clone, Debug, PartialEq)]
pub struct EditableTable {
    /// The form field the edits are saved under (its own name, as on the web).
    pub name: String,
    pub columns: Vec<EditColumn>,
    pub rows: Vec<EditRow>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditColumn {
    /// text, number, date-single, select or url.
    pub kind: String,
    pub key: String,
    pub header: String,
    pub width: Option<f64>,
    pub options: Vec<Choice>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditRow {
    pub id: String,
    /// One per column, as text (a number written the way JavaScript writes it).
    pub values: Vec<String>,
}

/// An option in a select, radio group, chips or option cards.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub value: String,
    pub label: String,
    pub description: Option<String>,
    pub icon: Option<Icon>,
    pub image: Option<Image>,
    pub disabled: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Form {
    pub name: String,
    pub fields: Vec<FormControl>,
    pub buttons: Buttons,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FormControl {
    pub label: String,
    pub hint: Option<String>,
    pub field: Field,
}

/// An input. Its value is read and written through the store: bound ones at their $variable
/// (Store::var, Store::set_var), the others under their form (Store::field, Store::set_field
/// with [`Field::kind`]).
#[derive(Clone, Debug, PartialEq)]
pub struct Field {
    pub name: String,
    pub input: Input,
    pub rules: Rules,
    /// The $variable it's bound to, or a value written in place.
    pub value: Option<Bound>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Text {
        kind: InputKind,
        placeholder: Option<String>,
    },
    TextArea {
        placeholder: Option<String>,
        rows: Option<f64>,
    },
    Select {
        options: Vec<Choice>,
        placeholder: Option<String>,
        size: Size,
    },
    Date {
        range: bool,
    },
    Slider {
        discrete: bool,
        min: f64,
        max: f64,
        step: Option<f64>,
        default: Vec<f64>,
        label: Option<String>,
    },
    /// Each box's name with its label, description and whether it starts checked.
    CheckBoxes(Vec<Toggle>),
    Radio {
        options: Vec<Choice>,
        default: Option<String>,
    },
    Switches {
        items: Vec<Toggle>,
        surface: Surface,
    },
    Chips {
        options: Vec<Choice>,
        multiple: bool,
        default: Vec<String>,
    },
    OptionCards {
        options: Vec<Choice>,
        multiple: bool,
        default: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Toggle {
    pub name: String,
    pub label: Option<String>,
    pub description: Option<String>,
    pub checked: bool,
}

impl Field {
    /// The componentType it writes under its form, as the web does: Chips and OptionCards name
    /// themselves (a single choice as the chosen value or nothing, several as a list), the other
    /// inputs don't.
    pub fn kind(&self) -> Option<&'static str> {
        match self.input {
            Input::Chips { .. } => Some("Chips"),
            Input::OptionCards { .. } => Some("OptionCards"),
            _ => None,
        }
    }

    /// The value to start from when the store has none: the default chips and option cards
    /// write into the form as soon as they're drawn (react-ui hydrates them), so a press
    /// carries them.
    pub fn hydrated(&self) -> Option<serde_json::Value> {
        let (multiple, default) = match &self.input {
            Input::Chips {
                multiple, default, ..
            }
            | Input::OptionCards {
                multiple, default, ..
            } => (*multiple, default),
            _ => return None,
        };
        if multiple {
            Some(serde_json::json!(default))
        } else {
            default
                .first()
                .filter(|d| !d.is_empty())
                .map(|d| serde_json::json!(d))
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub label: String,
    /// What pressing it does; a button written without one sends its label.
    pub action: Action,
    pub variant: ButtonVariant,
    pub destructive: bool,
    pub size: Size,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Buttons {
    pub buttons: Vec<Button>,
    pub direction: Direction,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IconButton {
    /// Its accessible label, and the words a press sends.
    pub name: String,
    pub icon: Icon,
    pub action: Action,
    pub variant: ButtonVariant,
    pub size: Size,
    pub circle: bool,
}

/// A lucide icon by its kebab-case name, with a topic to fall back on.
#[derive(Clone, Debug, PartialEq)]
pub struct Icon {
    pub name: String,
    pub category: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Tag {
    pub text: String,
    pub icon: Option<Icon>,
    pub size: Size,
    pub tone: Tone,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EntityList {
    pub rows: Vec<Entity>,
    pub small: bool,
    pub header: Option<Entity>,
    pub footer: Option<Entity>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Entity {
    pub left: String,
    pub right: String,
    /// The right side in tabular figures.
    pub number: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListBlock {
    pub items: Vec<ListItem>,
    /// Numbered, or with each item's image.
    pub images: bool,
    pub small: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ListItem {
    pub title: String,
    pub subtitle: Option<String>,
    pub image: Option<Image>,
    pub action_label: Option<String>,
    /// Pressing the item sends its title with this (outside any form).
    pub action: Option<Action>,
}

/// A grid or carousel of small cards (SnippetCardBlock, OverviewCardBlock, ContextCardBlock,
/// CompositeCardBlock, VisualCardBlock).
#[derive(Clone, Debug, PartialEq)]
pub struct CardBlock {
    pub kind: CardBlockKind,
    pub items: Vec<CardItem>,
    pub carousel: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CardBlockKind {
    Snippet,
    Overview,
    Context,
    Composite,
    Visual,
}

/// One small card. Which slots are filled depends on the block: a snippet has a left and a
/// right, an overview a top and a bottom, a composite a header, a body and a footer, a context
/// card a title and a body, a visual card a background image, a tag and a body.
#[derive(Clone, Debug, PartialEq)]
pub struct CardItem {
    pub id: Option<String>,
    pub header: Option<Box<Component>>,
    pub body: Vec<Component>,
    pub footer: Vec<Component>,
    /// A context card's title (a Tag in `header` when it was written as one) and Markdown body.
    pub title: Option<String>,
    pub text: Option<String>,
    pub tag: Option<Tag>,
    pub background: Option<ImageSrc>,
    pub background_alt: Option<String>,
    pub gray: bool,
    /// What pressing the card does, with the words it sends; None when the block has no action.
    pub press: Option<Press>,
}

/// A press prepared in advance: the words and the action with the card's details in it.
#[derive(Clone, Debug, PartialEq)]
pub struct Press {
    pub words: String,
    pub action: Action,
}

/// Text and BoldText: a line with optional subtext.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub bold: bool,
    pub value: String,
    pub subtext: Option<String>,
    /// Tabular figures for the value.
    pub number: bool,
    /// text, number or metric (a leading + or - coloured green or red).
    pub subtext_kind: String,
    pub size: Size,
}

#[derive(Clone, Debug, PartialEq)]
pub struct IconText {
    pub icon: Icon,
    pub tone: Tone,
    pub icon_size: Size,
    pub title: String,
    pub subtitle: Option<String>,
    pub bold: bool,
    pub vertical: bool,
}

/// ImageText (a thumbnail beside the text) and ImageTextLarge (a banner above it).
#[derive(Clone, Debug, PartialEq)]
pub struct ImageText {
    pub src: Option<ImageSrc>,
    pub alt: Option<String>,
    pub title: String,
    pub subtitle: Option<String>,
    pub bold: bool,
    pub vertical: bool,
    pub image_size: Option<f64>,
    pub large: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Metric {
    pub value: String,
    pub subtext: Option<String>,
    /// Struck through before the value (MetricIndicatorWithStrikethrough).
    pub previous: Option<String>,
    pub trend: Option<Trend>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Trend {
    pub up: bool,
    /// The percentage.
    pub value: f64,
}

/// Builds the tree, counting components and text against the card's caps.
struct Build {
    components: usize,
    text: usize,
    depth: usize,
    failed: Option<CardError>,
}

fn prop<'e>(el: &'e El, k: &str) -> &'e V {
    static UNDEF: V = V::Undef;
    el.props.get(k).unwrap_or(&UNDEF)
}

/// A list prop as a list: a single value is a list of one, nothing an empty list, and at most
/// `cap` kept.
fn list(v: &V, cap: usize) -> Vec<V> {
    match v {
        V::Arr(a) => a.iter().take(cap).cloned().collect(),
        V::Undef | V::Null => Vec::new(),
        other => vec![other.clone()],
    }
}

/// The items of a list and of the lists inside it, in order, up to MAX_CHILDREN.
fn flatten(v: &V, out: &mut Vec<V>, depth: usize) {
    match v {
        V::Arr(items) if depth < MAX_NESTING => {
            for it in items.iter() {
                if out.len() >= MAX_CHILDREN {
                    return;
                }
                flatten(it, out, depth + 1);
            }
        }
        V::Arr(_) | V::Undef | V::Null => {}
        other if out.len() < MAX_CHILDREN => out.push(other.clone()),
        _ => {}
    }
}

pub(crate) fn build(root: &V) -> Result<Card, CardError> {
    let mut b = Build {
        components: 0,
        text: 0,
        depth: 0,
        failed: None,
    };
    let root = b
        .component(root)
        .unwrap_or(Component::Unknown(String::new()));
    match b.failed {
        Some(e) => Err(e),
        None => Ok(Card { root }),
    }
}

impl Build {
    /// A text as React would show it: a string, or a number as JavaScript writes it; capped.
    fn text(&mut self, v: &V) -> Option<String> {
        let s = match v {
            V::Str(s) => s.to_string(),
            V::Num(n) => num_text(*n),
            _ => return None,
        };
        let mut s = if s.chars().count() > MAX_TEXT {
            let mut t: String = s.chars().take(MAX_TEXT - 1).collect();
            t.push('…');
            t
        } else {
            s
        };
        self.text += s.chars().count();
        if self.text > MAX_TOTAL_TEXT {
            self.failed.get_or_insert(CardError::TooMuch);
            s.clear();
        }
        Some(s)
    }

    fn string(&mut self, v: &V) -> String {
        self.text(v).unwrap_or_default()
    }

    /// An optional text: absent, null or empty is None.
    fn opt(&mut self, v: &V) -> Option<String> {
        self.text(v).filter(|s| !s.is_empty())
    }

    fn number(&self, v: &V) -> Option<f64> {
        chart::num(v)
    }

    fn image(&mut self, v: &V) -> Option<ImageSrc> {
        v.as_str().and_then(ImageSrc::parse)
    }

    fn icon(&mut self, v: &V) -> Option<Icon> {
        let el = v.as_el().filter(|e| &*e.name == "Icon")?;
        Some(Icon {
            name: self.string(prop(el, "name")),
            category: self.opt(prop(el, "category")),
        })
    }

    fn action(&self, v: &V) -> Action {
        action(v).unwrap_or_else(Action::default_words)
    }

    /// Children as components: elements built, bare strings and numbers as plain text, a list
    /// inside the list in its place (as React draws `Card([title, @Each(...)])`), the rest
    /// (null, objects) nothing; at most MAX_CHILDREN in all.
    fn children(&mut self, v: &V) -> Vec<Component> {
        let mut flat = Vec::new();
        flatten(v, &mut flat, 0);
        flat.iter().filter_map(|c| self.child(c)).collect()
    }

    fn child(&mut self, v: &V) -> Option<Component> {
        match v {
            V::El(_) => self.component(v),
            V::Str(_) | V::Num(_) => self.text(v).map(Component::Plain),
            V::Bool(b) => Some(Component::Plain(b.to_string())),
            _ => None,
        }
    }

    fn component(&mut self, v: &V) -> Option<Component> {
        let el = v.as_el()?;
        if self.failed.is_some() {
            return None;
        }
        self.components += 1;
        if self.components > MAX_COMPONENTS {
            self.failed = Some(CardError::TooMuch);
            return None;
        }
        if self.depth >= MAX_NESTING {
            self.failed = Some(CardError::TooDeep);
            return None;
        }
        self.depth += 1;
        let c = self.build(el);
        self.depth -= 1;
        Some(c)
    }

    fn flex(&mut self, el: &El) -> Flex {
        Flex {
            children: self.children(prop(el, "children")),
            direction: Direction::or_default(prop(el, "direction")),
            gap: Gap::or_default(prop(el, "gap")),
            align: Align::read(prop(el, "align")),
            justify: Justify::read(prop(el, "justify")),
            wrap: matches!(prop(el, "wrap"), V::Bool(true)),
            surface: Surface::or_default(prop(el, "variant")),
        }
    }

    fn sections(&mut self, el: &El, item: &str) -> Vec<Section> {
        list(prop(el, "items"), MAX_CHILDREN)
            .iter()
            .filter_map(|it| it.as_el().filter(|e| &*e.name == item).cloned())
            .map(|it| Section {
                value: self.string(prop(&it, "value")),
                title: self.string(prop(&it, "trigger")),
                content: self.nested(prop(&it, "content")),
            })
            .collect()
    }

    /// Children one level down, counted as nesting (a tab's content, a slide).
    fn nested(&mut self, v: &V) -> Vec<Component> {
        if self.depth >= MAX_NESTING {
            self.failed.get_or_insert(CardError::TooDeep);
            return Vec::new();
        }
        self.depth += 1;
        let out = self.children(v);
        self.depth -= 1;
        out
    }

    fn bound(&mut self, v: &V) -> Option<Bound> {
        match v {
            V::Bind(t) => Some(Bound::Var(t.to_string())),
            V::Undef | V::Null => None,
            other => Some(Bound::Fixed(super::value::to_serde(other))),
        }
    }

    fn choices(&mut self, v: &V) -> Vec<Choice> {
        list(v, MAX_CHILDREN)
            .iter()
            .filter_map(|it| it.as_el().cloned())
            .map(|it| {
                let top = prop(&it, "topContent");
                Choice {
                    value: self.string(prop(&it, "value")),
                    label: self
                        .text(prop(&it, "label"))
                        .or_else(|| self.text(prop(&it, "title")))
                        .unwrap_or_default(),
                    description: self
                        .opt(prop(&it, "description"))
                        .or_else(|| self.opt(prop(&it, "subtitle"))),
                    icon: self.icon(prop(&it, "icon")).or_else(|| self.icon(top)),
                    image: top.as_el().filter(|e| &*e.name == "Image").map(|e| Image {
                        alt: self.opt(prop(e, "alt")),
                        src: self.image(prop(e, "src")),
                        block: false,
                    }),
                    disabled: matches!(prop(&it, "disabled"), V::Bool(true)),
                }
            })
            .collect()
    }

    fn toggles(&mut self, v: &V) -> Vec<Toggle> {
        list(v, MAX_CHILDREN)
            .iter()
            .filter_map(|it| it.as_el().cloned())
            .map(|it| Toggle {
                name: self.string(prop(&it, "name")),
                label: self.opt(prop(&it, "label")),
                description: self.opt(prop(&it, "description")),
                checked: matches!(prop(&it, "defaultChecked"), V::Bool(true)),
            })
            .collect()
    }

    fn selection(&mut self, v: &V) -> Vec<String> {
        match v {
            V::Str(s) if !s.is_empty() => vec![s.to_string()],
            V::Arr(a) => a
                .iter()
                .take(MAX_CHILDREN)
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// An input component as a field, or None for one that isn't an input.
    fn field(&mut self, el: &El) -> Option<Field> {
        let p = |k: &str| prop(el, k);
        let input = match &*el.name {
            "Input" => Input::Text {
                kind: InputKind::or_default(p("type")),
                placeholder: self.opt(p("placeholder")),
            },
            "TextArea" => Input::TextArea {
                placeholder: self.opt(p("placeholder")),
                rows: self.number(p("rows")),
            },
            "Select" => Input::Select {
                options: self.choices(p("items")),
                placeholder: self.opt(p("placeholder")),
                size: Size::or_default(p("size")),
            },
            "DatePicker" => Input::Date {
                range: p("mode").as_str() == Some("range"),
            },
            "Slider" => Input::Slider {
                discrete: p("variant").as_str() == Some("discrete"),
                min: self.number(p("min")).unwrap_or(0.0),
                max: self.number(p("max")).unwrap_or(100.0),
                step: self.number(p("step")),
                default: list(p("defaultValue"), 2)
                    .iter()
                    .filter_map(|x| self.number(x))
                    .collect(),
                label: self.opt(p("label")),
            },
            "CheckBoxGroup" => Input::CheckBoxes(self.toggles(p("items"))),
            "RadioGroup" => Input::Radio {
                options: self.choices(p("items")),
                default: self.opt(p("defaultValue")),
            },
            "SwitchGroup" => Input::Switches {
                items: self.toggles(p("items")),
                surface: Surface::read(p("variant")).unwrap_or(Surface::Clear),
            },
            "Chips" | "OptionCards" => {
                let options = self
                    .choices(p("items"))
                    .into_iter()
                    .filter(|c| !c.value.is_empty())
                    .collect();
                let multiple = p("type").as_str() != Some("single");
                let default = self.selection(p("defaultValue"));
                if &*el.name == "Chips" {
                    Input::Chips {
                        options,
                        multiple,
                        default,
                    }
                } else {
                    Input::OptionCards {
                        options,
                        multiple,
                        default,
                    }
                }
            }
            _ => return None,
        };
        Some(Field {
            name: self.string(p("name")),
            input,
            rules: Rules::from_value(p("rules")),
            value: self.bound(p("value")),
        })
    }

    fn button(&mut self, el: &El) -> Button {
        Button {
            label: self.string(prop(el, "label")),
            action: self.action(prop(el, "action")),
            variant: ButtonVariant::or_default(prop(el, "variant")),
            destructive: prop(el, "type").as_str() == Some("destructive"),
            size: Size::or_default(prop(el, "size")),
        }
    }

    fn buttons(&mut self, el: &El) -> Buttons {
        Buttons {
            buttons: list(prop(el, "buttons"), MAX_CHILDREN)
                .iter()
                .filter_map(|b| b.as_el().filter(|e| &*e.name == "Button").cloned())
                .map(|b| self.button(&b))
                .collect(),
            direction: Direction::read(prop(el, "direction")).unwrap_or(Direction::Row),
        }
    }

    fn tag(&mut self, el: &El) -> Tag {
        Tag {
            text: self.string(prop(el, "text")),
            icon: self.icon(prop(el, "icon")),
            size: Size::read(prop(el, "size")).unwrap_or(Size::Md),
            tone: Tone::or_default(prop(el, "variant")),
        }
    }

    fn entity(&mut self, v: &V) -> Option<Entity> {
        if !matches!(v, V::Obj(_)) {
            return None;
        }
        Some(Entity {
            left: self.string(&v.field("left")),
            right: self.string(&v.field("right")),
            number: v.field("rightVariant").as_str() == Some("number"),
        })
    }

    fn line(&mut self, el: &El, bold: bool) -> Line {
        Line {
            bold,
            value: self.string(prop(el, "value")),
            subtext: self.opt(prop(el, "subtext")),
            number: prop(el, "variant").as_str() == Some("number"),
            subtext_kind: prop(el, "subtextVariant")
                .as_str()
                .filter(|k| matches!(*k, "text" | "number" | "metric"))
                .unwrap_or("text")
                .to_string(),
            size: Size::read(prop(el, "size")).unwrap_or(Size::Sm),
        }
    }

    fn trend(&mut self, v: &V) -> Option<Trend> {
        let dir = v.field("direction");
        let up = match dir.as_str()? {
            "up" => true,
            "down" => false,
            _ => return None,
        };
        Some(Trend {
            up,
            value: self.number(&v.field("value"))?,
        })
    }

    fn table(&mut self, el: &El) -> Table {
        let cols: Vec<El> = list(prop(el, "columns"), MAX_COLUMNS)
            .iter()
            .filter_map(|c| c.as_el().filter(|e| &*e.name == "Col").cloned())
            .collect();
        let data: Vec<Vec<V>> = cols
            .iter()
            .map(|c| list(prop(c, "data"), MAX_ROWS))
            .collect();
        let columns = cols
            .iter()
            .map(|c| Column {
                label: self.string(prop(c, "label")),
                kind: ColumnKind::or_default(prop(c, "type")),
            })
            .collect();
        let count = data.iter().map(Vec::len).max().unwrap_or(0);
        let rows = (0..count)
            .map(|r| {
                data.iter()
                    .map(|col| self.cell(col.get(r).unwrap_or(&V::Undef)))
                    .collect()
            })
            .collect();
        Table { columns, rows }
    }

    /// A table cell as the web shows it: text for a string, number or boolean, the component
    /// for an element, the parts of a list, nothing for a plain object.
    fn cell(&mut self, v: &V) -> Cell {
        match v {
            V::Undef | V::Null => Cell::Empty,
            V::Bool(b) => Cell::Text(b.to_string()),
            V::El(_) => self
                .component(v)
                .map_or(Cell::Empty, |c| Cell::Components(vec![c])),
            V::Arr(items) if items.iter().any(|x| matches!(x, V::El(_))) => {
                Cell::Components(self.children(v))
            }
            V::Arr(items) => {
                let parts: Vec<String> = items
                    .iter()
                    .take(MAX_CHILDREN)
                    .filter_map(|x| match x {
                        V::Bool(b) => Some(b.to_string()),
                        _ => self.text(x),
                    })
                    .collect();
                Cell::Text(parts.concat())
            }
            _ => self.text(v).map_or(Cell::Empty, Cell::Text),
        }
    }

    fn editable(&mut self, el: &El) -> EditableTable {
        let columns = list(prop(el, "columns"), MAX_COLUMNS)
            .iter()
            .map(|c| EditColumn {
                kind: c.field("type").as_str().unwrap_or("text").to_string(),
                key: self.string(&c.field("key")),
                header: self.string(&c.field("header")),
                width: self.number(&c.field("width")),
                options: list(&c.field("options"), MAX_CHILDREN)
                    .iter()
                    .map(|o| Choice {
                        value: self.string(&o.field("value")),
                        label: self.string(&o.field("label")),
                        description: None,
                        icon: None,
                        image: None,
                        disabled: false,
                    })
                    .collect(),
            })
            .collect();
        let rows = list(prop(el, "data"), MAX_ROWS)
            .iter()
            .map(|r| EditRow {
                id: self.string(&r.field("id")),
                values: list(&r.field("values"), MAX_COLUMNS)
                    .iter()
                    .map(|v| self.string(v))
                    .collect(),
            })
            .collect();
        EditableTable {
            name: self.string(prop(el, "name")),
            columns,
            rows,
        }
    }

    fn bars(&mut self, el: &El, kind: ChartKind) -> Option<Component> {
        let (labels, series) = {
            let mut text = |v: &V| self.string(v);
            chart::series(prop(el, "labels"), prop(el, "series"), &mut text)
        };
        if labels.is_empty() && series.is_empty() {
            return None;
        }
        let variant = prop(el, "variant").as_str().unwrap_or("");
        Some(Component::Chart(Chart::Bars(Bars {
            kind,
            labels,
            series,
            stacked: variant == "stacked",
            curve: match variant {
                "natural" => Curve::Natural,
                "step" => Curve::Step,
                _ => Curve::Linear,
            },
            x_label: self.opt(prop(el, "xLabel")),
            y_label: self.opt(prop(el, "yLabel")),
            height: self
                .number(prop(el, "height"))
                .filter(|h| *h > 0.0)
                .map(|h| h.min(2000.0) as f32),
        })))
    }

    fn slices(&mut self, el: &El, kind: SlicesKind) -> Component {
        let slices = {
            let mut text = |v: &V| self.string(v);
            chart::slices(prop(el, "labels"), prop(el, "values"), &mut text)
        };
        let kind = if kind == SlicesKind::Pie && prop(el, "variant").as_str() == Some("donut") {
            SlicesKind::Donut
        } else {
            kind
        };
        Component::Chart(Chart::Slices(Slices {
            kind,
            slices,
            semicircle: prop(el, "appearance").as_str() == Some("semiCircular"),
        }))
    }

    /// A card block, each card with the words and context its press sends (react-ui's card
    /// block renderers).
    fn cards(&mut self, el: &El, kind: CardBlockKind) -> Component {
        let block_action = action(prop(el, "action"));
        let carousel = prop(el, "layout").as_str() == Some("carousel");
        let items: Vec<El> = list(prop(el, "items"), MAX_CHILDREN)
            .iter()
            .filter_map(|i| i.as_el().cloned())
            .collect();
        let mut out = Vec::new();
        for (index, it) in items.iter().enumerate() {
            let p = |k: &str| prop(it, k);
            let id = self.opt(p("id"));
            let str_of = |v: &V, k: &str| {
                v.as_el()
                    .and_then(|e| e.props.get(k))
                    .and_then(|x| x.as_str().map(str::to_string))
            };
            let mut item = CardItem {
                id: id.clone(),
                header: None,
                body: Vec::new(),
                footer: Vec::new(),
                title: None,
                text: None,
                tag: None,
                background: None,
                background_alt: None,
                gray: false,
                press: None,
            };
            let mut details = Obj::new();
            details.set("itemIndex", V::Num(index as f64));
            details.set("itemId", p("id").clone());
            let words = match kind {
                CardBlockKind::Snippet => {
                    item.header = self.component(p("lhs")).map(Box::new);
                    item.body = self.component(p("rhs")).into_iter().collect();
                    let title = str_of(p("lhs"), "title");
                    details.set("itemTitle", title.as_deref().map_or(V::Undef, V::str));
                    details.set(
                        "itemSubtitle",
                        str_of(p("lhs"), "subtitle")
                            .as_deref()
                            .map_or(V::Undef, V::str),
                    );
                    details.set(
                        "itemValue",
                        str_of(p("rhs"), "value")
                            .as_deref()
                            .map_or(V::Undef, V::str),
                    );
                    title
                        .or_else(|| p("id").as_str().map(str::to_string))
                        .unwrap_or_else(|| format!("Snippet card {}", index + 1))
                }
                CardBlockKind::Overview => {
                    item.header = self.component(p("top")).map(Box::new);
                    item.body = self.component(p("bottom")).into_iter().collect();
                    let title = str_of(p("top"), "title").or_else(|| str_of(p("top"), "value"));
                    details.set("itemTitle", title.as_deref().map_or(V::Undef, V::str));
                    details.set(
                        "itemSubtitle",
                        str_of(p("top"), "subtitle")
                            .or_else(|| str_of(p("top"), "subtext"))
                            .as_deref()
                            .map_or(V::Undef, V::str),
                    );
                    details.set(
                        "itemMetricValue",
                        p("bottom")
                            .as_el()
                            .and_then(|e| e.props.get("value").cloned())
                            .unwrap_or(V::Undef),
                    );
                    title
                        .or_else(|| p("id").as_str().map(str::to_string))
                        .unwrap_or_else(|| format!("Overview card {}", index + 1))
                }
                CardBlockKind::Context => {
                    let title = match p("title") {
                        V::Str(s) => s.to_string(),
                        V::El(e) => e
                            .props
                            .get("text")
                            .and_then(V::as_str)
                            .unwrap_or("")
                            .to_string(),
                        _ => String::new(),
                    };
                    if let Some(t) = p("title").as_el().filter(|e| &*e.name == "Tag") {
                        item.tag = Some(self.tag(t));
                    } else {
                        item.title = self.opt(p("title"));
                    }
                    item.text = self.opt(p("body"));
                    item.gray = p("bgColor").as_str() == Some("gray");
                    item.background = self.image(p("bgImageSrc"));
                    item.background_alt = self.opt(p("bgImageAlt"));
                    details.set("itemTitle", V::str(&title));
                    for (k, from) in [
                        ("itemBody", "body"),
                        ("itemBgColor", "bgColor"),
                        ("itemBgImageSrc", "bgImageSrc"),
                        ("itemBgImageAlt", "bgImageAlt"),
                    ] {
                        details.set(k, p(from).clone());
                    }
                    Some(title)
                        .filter(|t| !t.is_empty())
                        .or_else(|| {
                            p("id")
                                .as_str()
                                .filter(|s| !s.is_empty())
                                .map(str::to_string)
                        })
                        .unwrap_or_else(|| format!("Context card {}", index + 1))
                }
                CardBlockKind::Composite => {
                    item.header = self.component(p("header")).map(Box::new);
                    item.body = self.children(p("body"));
                    let footer = p("footer");
                    item.footer = [footer.field("price"), footer.field("button")]
                        .iter()
                        .filter_map(|c| self.component(c))
                        .collect();
                    let header_props = p("header")
                        .as_el()
                        .map(|e| e.props.clone())
                        .unwrap_or_default();
                    let pick = |keys: &[&str]| {
                        keys.iter().find_map(|k| {
                            header_props.get(k).and_then(V::as_str).map(str::to_string)
                        })
                    };
                    let (title, subtitle, alt) = (
                        pick(&["title", "value"]),
                        pick(&["subtitle", "subtext"]),
                        pick(&["alt"]),
                    );
                    details.set("itemHeaderTitle", title.as_deref().map_or(V::Undef, V::str));
                    details.set(
                        "itemHeaderSubtitle",
                        subtitle.as_deref().map_or(V::Undef, V::str),
                    );
                    details.set("itemHeaderAlt", alt.as_deref().map_or(V::Undef, V::str));
                    details.set(
                        "itemBodyCount",
                        V::Num(p("body").as_arr().map_or(0, <[V]>::len) as f64),
                    );
                    details.set(
                        "itemFooterPrice",
                        footer
                            .field("price")
                            .as_el()
                            .and_then(|e| e.props.get("value").cloned())
                            .unwrap_or(V::Undef),
                    );
                    details.set(
                        "itemFooterButtonLabel",
                        footer
                            .field("button")
                            .as_el()
                            .and_then(|e| e.props.get("label").cloned())
                            .unwrap_or(V::Undef),
                    );
                    title
                        .filter(|t| !t.is_empty())
                        .or(alt.filter(|a| !a.is_empty()))
                        .or_else(|| {
                            p("id")
                                .as_str()
                                .filter(|s| !s.is_empty())
                                .map(str::to_string)
                        })
                        .unwrap_or_else(|| format!("Composite card {}", index + 1))
                }
                CardBlockKind::Visual => {
                    item.body = self.component(p("body")).into_iter().collect();
                    item.tag = p("tag")
                        .as_el()
                        .filter(|e| &*e.name == "Tag")
                        .map(|t| self.tag(t));
                    item.background = self.image(p("bgImageSrc"));
                    item.background_alt = self.opt(p("bgImageAlt"));
                    let body = p("body")
                        .as_el()
                        .and_then(|e| e.props.get("value").cloned())
                        .unwrap_or(V::Undef);
                    let tag = p("tag")
                        .as_el()
                        .and_then(|e| e.props.get("text").cloned())
                        .unwrap_or(V::Undef);
                    details.set("itemTag", tag.clone());
                    details.set("itemBody", body.clone());
                    details.set(
                        "itemBodySubtext",
                        p("body")
                            .as_el()
                            .and_then(|e| e.props.get("subtext").cloned())
                            .unwrap_or(V::Undef),
                    );
                    details.set("itemBgImageSrc", p("bgImageSrc").clone());
                    details.set("itemBgImageAlt", p("bgImageAlt").clone());
                    let truthy_text = |v: &V| v.truthy().then(|| super::value::to_string(v));
                    truthy_text(&body)
                        .or_else(|| truthy_text(&tag))
                        .or_else(|| truthy_text(p("id")))
                        .unwrap_or_else(|| format!("Visual card {}", index + 1))
                }
            };
            if let Some(a) = &block_action {
                let words = self.text(&V::str(&words)).unwrap_or_default();
                item.press = Some(Press {
                    words,
                    action: a.with_item(&details),
                });
            }
            out.push(item);
        }
        Component::Cards(CardBlock {
            kind,
            items: out,
            carousel,
        })
    }

    fn build(&mut self, el: &El) -> Component {
        let p = |k: &str| prop(el, k);
        match &*el.name {
            "Stack" => Component::Stack(Flex {
                surface: Surface::Clear,
                ..self.flex(el)
            }),
            "Card" => Component::Card(self.flex(el)),
            "Tabs" => Component::Tabs(self.sections(el, "TabItem")),
            "Accordion" => Component::Accordion(self.sections(el, "AccordionItem")),
            "Steps" => Component::Steps(
                list(p("items"), MAX_CHILDREN)
                    .iter()
                    .filter_map(|i| i.as_el().cloned())
                    .map(|i| StepItem {
                        title: self.string(prop(&i, "title")),
                        details: self.string(prop(&i, "details")),
                    })
                    .collect(),
            ),
            "Carousel" => {
                let slides = list(p("children"), MAX_CHILDREN)
                    .iter()
                    .map(|s| self.nested(s))
                    .collect();
                Component::Carousel {
                    slides,
                    surface: Surface::or_default(p("variant")),
                }
            }
            "Separator" => Component::Separator {
                vertical: p("orientation").as_str() == Some("vertical"),
            },
            "Modal" => Component::Modal(Modal {
                title: self.string(p("title")),
                open: self
                    .bound(p("open"))
                    .unwrap_or(Bound::Fixed(serde_json::Value::Bool(false))),
                children: self.children(p("children")),
                size: Size::read(p("size")).unwrap_or(Size::Md),
            }),
            "CardHeader" => Component::Header {
                title: self.opt(p("title")),
                subtitle: self.opt(p("subtitle")),
            },
            "TextContent" => Component::Text {
                markdown: self.string(p("text")),
                size: TextSize::or_default(p("size")),
            },
            "MarkDownRenderer" => Component::Markdown {
                markdown: self.string(p("textMarkdown")),
                surface: Surface::read(p("variant")).unwrap_or(Surface::Clear),
            },
            "Callout" => Component::Callout(Callout {
                tone: Tone::or_default(p("variant")),
                title: self.opt(p("title")),
                description: self.opt(p("description")),
                visible: self.bound(p("visible")),
                plain: false,
            }),
            "TextCallout" => Component::Callout(Callout {
                tone: Tone::or_default(p("variant")),
                title: self.opt(p("title")),
                description: self.opt(p("description")),
                visible: None,
                plain: true,
            }),
            "Image" | "ImageBlock" => Component::Image(Image {
                alt: self.opt(p("alt")),
                src: self.image(p("src")),
                block: &*el.name == "ImageBlock",
            }),
            "ImageGallery" => Component::Gallery(
                list(p("images"), MAX_CHILDREN)
                    .iter()
                    .map(|i| GalleryImage {
                        src: self.image(&i.field("src")),
                        alt: self.opt(&i.field("alt")),
                        details: self.opt(&i.field("details")),
                    })
                    .collect(),
            ),
            "CodeBlock" => Component::Code {
                language: self.string(p("language")),
                code: self.string(p("codeString")),
            },
            "InlineHeader" => Component::InlineHeader {
                heading: self.string(p("heading")),
                description: self.opt(p("description")),
            },
            "Table" => Component::Table(self.table(el)),
            "EditableTable" => Component::EditableTable(self.editable(el)),
            "BarChart" => self
                .bars(el, ChartKind::Bar)
                .unwrap_or(Component::Unknown(el.name.to_string())),
            "HorizontalBarChart" => self
                .bars(el, ChartKind::HorizontalBar)
                .unwrap_or(Component::Unknown(el.name.to_string())),
            "LineChart" => self
                .bars(el, ChartKind::Line)
                .unwrap_or(Component::Unknown(el.name.to_string())),
            "AreaChart" => self
                .bars(el, ChartKind::Area)
                .unwrap_or(Component::Unknown(el.name.to_string())),
            "RadarChart" => self
                .bars(el, ChartKind::Radar)
                .unwrap_or(Component::Unknown(el.name.to_string())),
            "PieChart" => self.slices(el, SlicesKind::Pie),
            "RadialChart" => self.slices(el, SlicesKind::Radial),
            "SingleStackedBarChart" => self.slices(el, SlicesKind::StackedBar),
            "ScatterChart" => {
                let sets = {
                    let mut text = |v: &V| self.string(v);
                    chart::scatter(p("datasets"), &mut text)
                };
                Component::Chart(Chart::Scatter(Scatter {
                    sets,
                    x_label: self.opt(p("xLabel")),
                    y_label: self.opt(p("yLabel")),
                }))
            }
            "Form" => {
                let fields = list(p("fields"), MAX_CHILDREN)
                    .iter()
                    .filter_map(|f| f.as_el().filter(|e| &*e.name == "FormControl").cloned())
                    .filter_map(|fc| {
                        let field = prop(&fc, "input")
                            .as_el()
                            .cloned()
                            .and_then(|i| self.field(&i))?;
                        Some(FormControl {
                            label: self.string(prop(&fc, "label")),
                            hint: self.opt(prop(&fc, "hint")),
                            field,
                        })
                    })
                    .collect();
                let buttons = p("buttons")
                    .as_el()
                    .filter(|e| &*e.name == "Buttons")
                    .cloned()
                    .map(|b| self.buttons(&b))
                    .unwrap_or(Buttons {
                        buttons: Vec::new(),
                        direction: Direction::Row,
                    });
                Component::Form(Form {
                    name: self.string(p("name")),
                    fields,
                    buttons,
                })
            }
            "FormControl" => match p("input").as_el().cloned().and_then(|i| self.field(&i)) {
                Some(f) => Component::Field(f),
                None => Component::Unknown(el.name.to_string()),
            },
            "Input" | "TextArea" | "Select" | "DatePicker" | "Slider" | "CheckBoxGroup"
            | "RadioGroup" | "SwitchGroup" | "Chips" | "OptionCards" => match self.field(el) {
                Some(f) => Component::Field(f),
                None => Component::Unknown(el.name.to_string()),
            },
            "Label" => Component::Label(self.string(p("text"))),
            "Button" => Component::Button(self.button(el)),
            "Buttons" => Component::Buttons(self.buttons(el)),
            "IconButton" => Component::IconButton(IconButton {
                name: self.string(p("name")),
                icon: self.icon(p("icon")).unwrap_or(Icon {
                    name: String::new(),
                    category: None,
                }),
                action: self.action(p("action")),
                variant: ButtonVariant::read(p("variant")).unwrap_or(ButtonVariant::Secondary),
                size: Size::or_default(p("size")),
                circle: p("shape").as_str() == Some("circle"),
            }),
            "TagBlock" => Component::TagBlock {
                tags: list(p("tags"), MAX_CHILDREN)
                    .iter()
                    .filter_map(|t| self.text(t))
                    .collect(),
                size: Size::read(p("size")).unwrap_or(Size::Md),
            },
            "Tag" => Component::Tag(self.tag(el)),
            "Icon" => match self.icon(&V::El(std::sync::Arc::new(el.clone()))) {
                Some(i) => Component::Icon(i),
                None => Component::Unknown(el.name.to_string()),
            },
            "EntityList" => {
                let small = p("size").as_str() == Some("small");
                Component::EntityList(EntityList {
                    rows: list(p("rows"), MAX_ROWS)
                        .iter()
                        .filter_map(|r| self.entity(r))
                        .collect(),
                    small,
                    header: if small {
                        None
                    } else {
                        self.entity(p("header"))
                    },
                    footer: if small {
                        None
                    } else {
                        self.entity(p("footer"))
                    },
                })
            }
            "ListBlock" => {
                let images = p("variant").as_str() == Some("image");
                let items = list(p("items"), MAX_CHILDREN)
                    .iter()
                    .filter_map(|i| i.as_el().cloned())
                    .map(|i| {
                        let image = prop(&i, "image");
                        let action =
                            action(prop(&i, "action")).filter(|_| prop(&i, "action").truthy());
                        ListItem {
                            title: self.string(prop(&i, "title")),
                            subtitle: self.opt(prop(&i, "subtitle")),
                            image: if images && matches!(image, V::Obj(_)) {
                                Some(Image {
                                    alt: self.opt(&image.field("alt")),
                                    src: self.image(&image.field("src")),
                                    block: false,
                                })
                            } else {
                                None
                            },
                            action_label: if action.is_some() {
                                self.opt(prop(&i, "actionLabel"))
                            } else {
                                None
                            },
                            action,
                        }
                    })
                    .collect();
                Component::List(ListBlock {
                    items,
                    images,
                    small: p("size").as_str() == Some("small"),
                })
            }
            "SnippetCardBlock" => self.cards(el, CardBlockKind::Snippet),
            "OverviewCardBlock" => self.cards(el, CardBlockKind::Overview),
            "ContextCardBlock" => self.cards(el, CardBlockKind::Context),
            "CompositeCardBlock" => self.cards(el, CardBlockKind::Composite),
            "VisualCardBlock" => self.cards(el, CardBlockKind::Visual),
            "Text" | "BoldText" => Component::Line(self.line(el, &*el.name == "BoldText")),
            "IconText" => Component::IconText(IconText {
                icon: self.icon(p("icon")).unwrap_or(Icon {
                    name: String::new(),
                    category: None,
                }),
                tone: Tone::or_default(p("iconVariant")),
                icon_size: Size::read(p("iconSize")).unwrap_or(Size::M),
                title: self.string(p("title")),
                subtitle: self.opt(p("subtitle")),
                bold: matches!(p("bold"), V::Bool(true)),
                vertical: p("layout").as_str() == Some("vertical"),
            }),
            "ImageText" | "ImageTextLarge" => Component::ImageText(ImageText {
                src: self.image(p("src")),
                alt: self.opt(p("alt")),
                title: self.string(p("title")),
                subtitle: self.opt(p("subtitle")),
                bold: matches!(p("bold"), V::Bool(true)),
                vertical: p("layout").as_str() == Some("vertical"),
                image_size: self.number(p("imageSize")),
                large: &*el.name == "ImageTextLarge",
            }),
            "MetricIndicatorInline" | "MetricIndicatorWithStrikethrough" => {
                Component::Metric(Metric {
                    value: self.string(p("value")),
                    subtext: self.opt(p("subtext")),
                    previous: self.opt(p("previousValue")),
                    trend: self.trend(p("trend")),
                })
            }
            other => Component::Unknown(other.to_string()),
        }
    }
}
