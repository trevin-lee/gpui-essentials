// Changed in Trevin Lee's fork (2026): callout and toggle kinds.
use gpui::SharedString;

/// The most columns a [`BlockKind::Columns`] has.
pub const MAX_COLUMNS: usize = 5;

/// The alignment of a table column.
#[derive(Debug, Default, Copy, Clone, PartialEq)]
pub enum ColumnAlign {
    /// No alignment in the delimiter row (`---`); drawn like `Left`.
    #[default]
    None,
    Left,
    Center,
    Right,
}

impl From<markdown::mdast::AlignKind> for ColumnAlign {
    fn from(value: markdown::mdast::AlignKind) -> Self {
        use markdown::mdast::AlignKind as A;
        match value {
            A::None => Self::None,
            A::Left => Self::Left,
            A::Center => Self::Center,
            A::Right => Self::Right,
        }
    }
}

/// Public, payload-light projection of a block's kind, for matching in
/// toolbar code. Payloads (code language, table alignments, definition
/// targets) stay in the crate-internal `BlockKindData`.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum BlockKind {
    // containers
    Root,
    Blockquote,
    /// A blockquote with a callout marker (`> [!NOTE]`). Its name, in ASCII
    /// lowercase, is read with `BlockEditorModel::callout_at_caret`.
    Callout,
    /// A block whose first child is its summary and the rest its body, shown
    /// only while open (`<details><summary>`).
    Toggle {
        open: bool,
    },
    List {
        ordered: bool,
    },
    ListItem {
        checked: Option<bool>,
    },
    Table,
    TableRow,
    TableCell,
    /// Blocks side by side: 2 to [`MAX_COLUMNS`] columns, only at the top level.
    Columns,
    /// One of a [`BlockKind::Columns`]' columns: any blocks but columns.
    Column,
    // text-bearing leaves
    Paragraph,
    Heading {
        level: u8,
    },
    CodeBlock,
    // atomic leaves
    /// Reserved: on main, images are inline nodes inside a paragraph, so the
    /// spike stores them as inline atoms and never creates this kind.
    Image,
    HorizontalRule,
    Break,
    Custom,
    Definition,
    Unknown,
}

/// Internal kind with payload.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum BlockKindData {
    Root,
    Blockquote,
    Callout {
        name: SharedString,
    },
    Toggle {
        open: bool,
    },
    List {
        ordered: bool,
        start: Option<u32>,
    },
    ListItem {
        checked: Option<bool>,
        spread: bool,
    },
    Table {
        aligns: Vec<ColumnAlign>,
    },
    TableRow,
    TableCell,
    Columns,
    /// `width`: its share of the row, as written; the shares are made to add up to 1 when the
    /// document is written.
    Column {
        width: f32,
    },
    Paragraph,
    Heading {
        level: u8,
    },
    CodeBlock {
        lang: Option<SharedString>,
        meta: Option<SharedString>,
    },
    HorizontalRule,
    Break {
        html: bool,
    },
    Custom,
    Definition {
        identifier: SharedString,
        url: SharedString,
        title: Option<SharedString>,
    },
    Unknown,
}

impl BlockKindData {
    pub(crate) fn public(&self) -> BlockKind {
        match self {
            Self::Root => BlockKind::Root,
            Self::Blockquote => BlockKind::Blockquote,
            Self::Callout { .. } => BlockKind::Callout,
            Self::Toggle { open } => BlockKind::Toggle { open: *open },
            Self::List { ordered, .. } => BlockKind::List { ordered: *ordered },
            Self::ListItem { checked, .. } => BlockKind::ListItem { checked: *checked },
            Self::Table { .. } => BlockKind::Table,
            Self::TableRow => BlockKind::TableRow,
            Self::TableCell => BlockKind::TableCell,
            Self::Columns => BlockKind::Columns,
            Self::Column { .. } => BlockKind::Column,
            Self::Paragraph => BlockKind::Paragraph,
            Self::Heading { level } => BlockKind::Heading { level: *level },
            Self::CodeBlock { .. } => BlockKind::CodeBlock,
            Self::HorizontalRule => BlockKind::HorizontalRule,
            Self::Break { .. } => BlockKind::Break,
            Self::Custom => BlockKind::Custom,
            Self::Definition { .. } => BlockKind::Definition,
            Self::Unknown => BlockKind::Unknown,
        }
    }

    /// Whether this kind holds child blocks.
    pub(crate) fn is_container(&self) -> bool {
        matches!(
            self,
            Self::Root
                | Self::Blockquote
                | Self::Callout { .. }
                | Self::Toggle { .. }
                | Self::List { .. }
                | Self::ListItem { .. }
                | Self::Table { .. }
                | Self::TableRow
                | Self::Columns
                | Self::Column { .. }
        )
    }

    /// Whether this kind holds editable inline text.
    pub(crate) fn is_textblock(&self) -> bool {
        matches!(
            self,
            Self::Paragraph | Self::Heading { .. } | Self::TableCell
        )
    }
}

impl BlockKindData {
    /// Whether this kind holds blocks the way a blockquote does: Enter on an
    /// empty last paragraph leaves it, and Backspace at the start of its first
    /// paragraph lifts that paragraph out.
    pub(crate) fn is_quote_like(&self) -> bool {
        matches!(self, Self::Blockquote | Self::Callout { .. })
    }
}
