//! The slash menu: typing `/` lists the blocks the line can become.
//!
//! The rules follow Tiptap's suggestion menus: `/` opens it at the start of a line or after a space,
//! the text typed after it narrows the list by title and aliases, ↑ and ↓ move (wrapping), Enter
//! or Tab picks, Esc closes it and leaves the text, and it closes on its own once nothing matches
//! or the caret leaves the `/…` it was opened for. Picking removes the `/…` and turns the line
//! into the block, or inserts the block in place of an empty line.
//!
//! [`default_items`] are the built-in blocks; an app adds its own (a widget, say) with
//! [`Options::slash_item`](crate::Options::slash_item).

use gpui_kit::SharedString;
use markdown_editor::{BlockKind, EditorCommand};

/// What picking an item does to the caret's line.
#[non_exhaustive]
#[derive(Clone, Debug, PartialEq)]
pub enum SlashAction {
    /// Run an editor command on the line.
    Command(EditorCommand),
    /// Insert Markdown at the caret, parsed as blocks (a divider, a widget): on an empty line it
    /// replaces the line and the caret goes to a line after it.
    Markdown(SharedString),
    /// Ask for a picture from this computer, then insert it (see
    /// [`Options::image_link`](crate::Options::image_link) for the link it writes).
    Image,
    /// Ask the app for a new document inside this one
    /// ([`EditorEvent::NewPageRequested`](crate::EditorEvent::NewPageRequested)). The app makes it
    /// and calls [`Editor::insert_page`](crate::Editor::insert_page).
    NewPage,
    /// Ask the app which document to link to
    /// ([`EditorEvent::DocLinkRequested`](crate::EditorEvent::DocLinkRequested)). The app shows
    /// a picker and calls [`Editor::insert_mention`](crate::Editor::insert_mention).
    LinkToPage,
    /// Ask the app which document to embed, and how to show it
    /// ([`EditorEvent::EmbedRequested`](crate::EditorEvent::EmbedRequested)). The app calls
    /// [`Editor::insert_embed`](crate::Editor::insert_embed).
    Embed,
}

/// One entry of the menu. Make one with [`SlashItem::new`] and its builder methods.
#[non_exhaustive]
#[derive(Clone, Debug)]
pub struct SlashItem {
    pub title: SharedString,
    pub hint: SharedString,
    /// Other words that find it.
    pub aliases: Vec<SharedString>,
    /// The glyph in its tile.
    pub icon: SharedString,
    /// The heading it's listed under while nothing is typed: [`MEDIA`] unless set.
    pub group: SharedString,
    pub action: SlashAction,
}

impl SlashItem {
    pub fn new(title: impl Into<SharedString>, action: SlashAction) -> Self {
        Self {
            title: title.into(),
            hint: SharedString::default(),
            aliases: Vec::new(),
            icon: "•".into(),
            group: MEDIA.into(),
            action,
        }
    }

    pub fn hint(mut self, hint: impl Into<SharedString>) -> Self {
        self.hint = hint.into();
        self
    }

    pub fn aliases<S: Into<SharedString>>(mut self, aliases: impl IntoIterator<Item = S>) -> Self {
        self.aliases = aliases
            .into_iter()
            .map(|a| a.into().to_lowercase().into())
            .collect();
        self
    }

    pub fn icon(mut self, icon: impl Into<SharedString>) -> Self {
        self.icon = icon.into();
        self
    }

    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = group.into();
        self
    }
}

/// The group the built-in text blocks are listed under.
pub const BASIC: &str = "Basic blocks";
/// The group pictures, tables and widgets are listed under.
pub const MEDIA: &str = "Media and widgets";

/// The built-in blocks, in order.
pub fn default_items() -> Vec<SlashItem> {
    use EditorCommand as C;
    use SlashAction::*;
    let basic = |title: &str, hint: &str, aliases: &[&str], icon: &str, action| {
        SlashItem::new(title.to_string(), action)
            .hint(hint.to_string())
            .aliases(aliases.iter().map(|a| a.to_string()))
            .icon(icon.to_string())
            .group(BASIC)
    };
    vec![
        basic(
            "Text",
            "A plain paragraph",
            &["paragraph", "plain"],
            "T",
            Command(C::SetBlockKind(BlockKind::Paragraph)),
        ),
        basic(
            "Heading 1",
            "The largest heading",
            &["h1", "title"],
            "H1",
            Command(C::SetBlockKind(BlockKind::Heading { level: 1 })),
        ),
        basic(
            "Heading 2",
            "A section heading",
            &["h2", "subtitle"],
            "H2",
            Command(C::SetBlockKind(BlockKind::Heading { level: 2 })),
        ),
        basic(
            "Heading 3",
            "A smaller heading",
            &["h3"],
            "H3",
            Command(C::SetBlockKind(BlockKind::Heading { level: 3 })),
        ),
        basic(
            "Bulleted list",
            "Points, one per line",
            &["ul", "bullet", "unordered"],
            "•",
            Command(C::ToggleList { ordered: false }),
        ),
        basic(
            "Numbered list",
            "Steps in order",
            &["ol", "ordered", "1."],
            "1.",
            Command(C::ToggleList { ordered: true }),
        ),
        basic(
            "To-do list",
            "Items you can tick off",
            &["todo", "task", "checkbox", "[]"],
            "☐",
            Command(C::ToggleTask),
        ),
        basic(
            "Toggle",
            "A summary that opens to more",
            &["details", "collapse", "fold"],
            "▸",
            Command(C::WrapInToggle),
        ),
        basic(
            "Quote",
            "Words from someone else",
            &["blockquote", ">"],
            "❝",
            Command(C::ToggleBlockquote),
        ),
        basic(
            "Callout",
            "A highlighted note",
            &["note", "info", "alert"],
            "ℹ",
            Command(C::SetCallout("note".into())),
        ),
        basic(
            "Warning",
            "A callout that warns",
            &["caution", "alert", "danger"],
            "⚠",
            Command(C::SetCallout("warning".into())),
        ),
        basic(
            "Code",
            "Code with highlighting",
            &["codeblock", "```", "snippet"],
            "</>",
            Command(C::SetCodeBlock { lang: None }),
        ),
        basic(
            "Divider",
            "A line across the page",
            &["hr", "rule", "line", "---"],
            "—",
            Markdown("---".into()),
        ),
        SlashItem::new("Table of contents", Markdown(crate::toc::MARKDOWN.into()))
            .hint("The page's headings, as links")
            .aliases(["toc", "contents", "outline"])
            .icon("☰"),
        SlashItem::new("Table", Command(C::InsertTable { rows: 3, cols: 3 }))
            .hint("Rows and columns")
            .aliases(["grid", "columns"])
            .icon("▦"),
        SlashItem::new("Image", Image)
            .hint("A picture from this computer")
            .aliases(["picture", "photo", "img"])
            .icon("▣"),
    ]
}

/// The group the items for other documents are listed under.
pub const DOCS: &str = "Pages";

/// The items for other documents, which [`Options::docs`](crate::Options::docs) adds: Page and
/// Link to page.
pub fn doc_items() -> Vec<SlashItem> {
    vec![
        SlashItem::new("Page", SlashAction::NewPage)
            .hint("A new page inside this one")
            .aliases(["subpage", "sub-page", "doc", "document"])
            .icon("📄")
            .group(DOCS),
        SlashItem::new("Link to page", SlashAction::LinkToPage)
            .hint("A link to another page")
            .aliases(["mention", "@", "reference", "link"])
            .icon("↗")
            .group(DOCS),
        SlashItem::new("Embed", SlashAction::Embed)
            .hint("Another page, table or board shown here")
            .aliases(["view", "linked", "database", "board", "mindmap"])
            .icon("▣")
            .group(DOCS),
    ]
}

/// The items `query` finds: a case-insensitive substring of the title or an alias, titles that
/// start with the query first.
pub fn filter(items: &[SlashItem], query: &str) -> Vec<usize> {
    let q = query.to_lowercase();
    let mut found: Vec<(bool, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(ix, item)| {
            let title = item.title.to_lowercase();
            let hit =
                title.contains(&q) || item.aliases.iter().any(|a| a.to_lowercase().contains(&q));
            hit.then_some((!title.starts_with(&q), ix))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, ix)| ix).collect()
}

/// Where a slash command is being typed: the byte offset of its `/` in the caret's line and the
/// query after it. `prefix` is the line up to the caret. A `/` counts at the start or after a
/// space, and the query holds no space and at most 24 characters, so a slash inside a word (a
/// URL's, `and/or`) doesn't open the menu.
pub fn query_at(prefix: &str) -> Option<(usize, &str)> {
    let slash = prefix.rfind('/')?;
    let before = &prefix[..slash];
    if !(before.is_empty() || before.ends_with(char::is_whitespace)) {
        return None;
    }
    let query = &prefix[slash + 1..];
    if query.contains(char::is_whitespace) || query.chars().count() > 24 {
        return None;
    }
    Some((slash, query))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_at_line_start_or_after_a_space_only() {
        assert_eq!(query_at("/"), Some((0, "")));
        assert_eq!(query_at("/hea"), Some((0, "hea")));
        assert_eq!(query_at("some text /to"), Some((10, "to")));
        assert_eq!(query_at("a/b"), None);
        assert_eq!(query_at("https://x.com/"), None);
        assert_eq!(query_at("/head ing"), None);
        assert_eq!(query_at("no slash"), None);
        assert_eq!(query_at("日本語 /見出し"), Some((10, "見出し")));
        assert_eq!(
            query_at("/日本語日本語日本語"),
            Some((0, "日本語日本語日本語"))
        );
    }

    #[test]
    fn filters_by_title_and_alias_with_prefix_matches_first() {
        let items = default_items();
        let titles = |q: &str| -> Vec<String> {
            filter(&items, q)
                .into_iter()
                .map(|ix| items[ix].title.to_string())
                .collect()
        };
        assert_eq!(titles("h1"), ["Heading 1"]);
        assert_eq!(titles("todo"), ["To-do list"]);
        assert_eq!(titles("head")[..3], ["Heading 1", "Heading 2", "Heading 3"]);
        // "list" is in three titles; none starts with it, so they keep their order.
        assert_eq!(
            titles("list"),
            ["Bulleted list", "Numbered list", "To-do list"]
        );
        assert!(titles("zzz").is_empty());
        assert_eq!(filter(&items, "").len(), items.len());
    }

    #[test]
    fn an_apps_own_item_is_found_by_its_aliases() {
        let mut items = default_items();
        items.push(
            SlashItem::new("Chart", SlashAction::Markdown("```chart\n```".into()))
                .aliases(["Graph", "plot"]),
        );
        // Aliases set without the builder still match whatever their case.
        items.last_mut().unwrap().aliases.push("Diagram".into());
        let found: Vec<_> = filter(&items, "diag")
            .into_iter()
            .map(|ix| items[ix].title.clone())
            .collect();
        assert_eq!(found, ["Chart"]);
        let found: Vec<_> = filter(&items, "plot")
            .into_iter()
            .map(|ix| items[ix].title.clone())
            .collect();
        assert_eq!(found, ["Chart"]);
    }
}
