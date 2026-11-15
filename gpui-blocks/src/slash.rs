//! The slash menu: typing `/` lists the blocks the line can become.
//!
//! The rules are Tiptap's and Novel's: `/` opens it at the start of a line or after a space,
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
#[derive(Clone, Debug, PartialEq)]
pub enum SlashAction {
    /// Run an editor command on the line.
    Command(EditorCommand),
    /// Insert Markdown at the caret, parsed as blocks (a divider, a widget): on an empty line it
    /// replaces the line and the caret goes to a line after it.
    Markdown(SharedString),
    /// Ask for a picture from this computer, then insert it.
    Image,
}

/// One entry of the menu.
#[derive(Clone, Debug)]
pub struct SlashItem {
    pub title: SharedString,
    pub hint: SharedString,
    /// Other words that find it, in lowercase.
    pub aliases: Vec<SharedString>,
    /// The glyph in its tile.
    pub icon: SharedString,
    /// The heading it's listed under while nothing is typed.
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
        basic("Text", "Just start writing", &["paragraph", "plain"], "T", Command(C::SetBlockKind(BlockKind::Paragraph))),
        basic("Heading 1", "Big section heading", &["h1", "title"], "H1", Command(C::SetBlockKind(BlockKind::Heading { level: 1 }))),
        basic("Heading 2", "Medium section heading", &["h2", "subtitle"], "H2", Command(C::SetBlockKind(BlockKind::Heading { level: 2 }))),
        basic("Heading 3", "Small section heading", &["h3"], "H3", Command(C::SetBlockKind(BlockKind::Heading { level: 3 }))),
        basic("Bulleted list", "A simple bulleted list", &["ul", "bullet", "unordered"], "•", Command(C::ToggleList { ordered: false })),
        basic("Numbered list", "A list with numbers", &["ol", "ordered", "1."], "1.", Command(C::ToggleList { ordered: true })),
        basic("To-do list", "Track tasks with a checklist", &["todo", "task", "checkbox", "[]"], "☐", Command(C::ToggleTask)),
        basic("Toggle", "Hide content inside", &["details", "collapse", "fold"], "▸", Command(C::WrapInToggle)),
        basic("Quote", "Capture a quote", &["blockquote", ">"], "❝", Command(C::ToggleBlockquote)),
        basic("Callout", "Make writing stand out", &["note", "info", "alert"], "ℹ", Command(C::SetCallout("note".into()))),
        basic("Warning", "A callout that warns", &["caution", "alert", "danger"], "⚠", Command(C::SetCallout("warning".into()))),
        basic("Code", "Capture a code snippet", &["codeblock", "```", "snippet"], "</>", Command(C::SetCodeBlock { lang: None })),
        basic("Divider", "Visually divide blocks", &["hr", "rule", "line", "---"], "—", Markdown("---".into())),
        SlashItem::new("Table", Command(C::InsertTable { rows: 3, cols: 3 }))
            .hint("A table of rows and columns")
            .aliases(["grid", "columns"])
            .icon("▦"),
        SlashItem::new("Image", Image)
            .hint("A picture from this computer")
            .aliases(["picture", "photo", "img"])
            .icon("▣"),
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
            let hit = title.contains(&q) || item.aliases.iter().any(|a| a.contains(&q));
            hit.then_some((!title.starts_with(&q), ix))
        })
        .collect();
    found.sort();
    found.into_iter().map(|(_, ix)| ix).collect()
}

/// Where a slash command is being typed: the byte offset of its `/` in the caret's line and the
/// query after it. `prefix` is the line up to the caret. A `/` counts at the start or after a
/// space, and the query holds no space; a URL's or a path's slashes don't open the menu.
pub fn query_at(prefix: &str) -> Option<(usize, &str)> {
    let slash = prefix.rfind('/')?;
    let before = &prefix[..slash];
    if !(before.is_empty() || before.ends_with(char::is_whitespace)) {
        return None;
    }
    let query = &prefix[slash + 1..];
    if query.contains(char::is_whitespace) || query.len() > 24 {
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
        assert_eq!(titles("list"), ["Bulleted list", "Numbered list", "To-do list"]);
        assert!(titles("zzz").is_empty());
        assert_eq!(filter(&items, "").len(), items.len());
    }

    #[test]
    fn an_apps_own_item_is_found_by_its_aliases() {
        let mut items = default_items();
        items.push(SlashItem::new("Chart", SlashAction::Markdown("```chart\n```".into())).aliases(["Graph", "plot"]));
        let found: Vec<_> = filter(&items, "plot").into_iter().map(|ix| items[ix].title.clone()).collect();
        assert_eq!(found, ["Chart"]);
    }
}
