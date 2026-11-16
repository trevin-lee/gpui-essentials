//! Syntax highlighting with tree-sitter, behind the `highlight` feature (off by default, for
//! the grammars' size).
//!
//! A [`Highlighter`] knows the languages; [`Highlighter::highlight`] gives a piece of code's
//! ranges and styles in a [`SyntaxTheme`], and [`Highlighter::into_fn`] makes the
//! [`HighlightFn`] that TextView, gpui-blocks and gpui-chatwidgets
//! take from their host. Nothing runs until code is highlighted, and nothing while idle.
//!
//! ```no_run
//! use gpui_core::highlight::{Highlighter, SyntaxTheme};
//! # fn run(cx: &gpui::App) {
//! let highlight = Highlighter::new().into_fn(SyntaxTheme::from_theme(gpui_core::Theme::global(cx)));
//! let spans = highlight("rust", "fn main() {}");
//! # }
//! ```

use std::cell::RefCell;
use std::sync::{Arc, OnceLock};

use gpui::{FontStyle, FontWeight, HighlightStyle, Hsla};
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent};

use crate::text::{HighlightFn, Highlights};
use crate::theme::Theme;

/// The capture names a grammar's highlights are matched to (the longest that prefixes a
/// capture wins: `keyword.function` is `keyword`), each with its place in a [`SyntaxTheme`].
const NAMES: &[&str] = &[
    "keyword",
    "string",
    "string.escape",
    "string.special",
    "escape",
    "comment",
    "function",
    "function.method",
    "function.macro",
    "constructor",
    "type",
    "type.builtin",
    "constant",
    "constant.builtin",
    "number",
    "boolean",
    "property",
    "attribute",
    "tag",
    "label",
    "punctuation",
    "operator",
    "variable.builtin",
    "module",
    "embedded",
    // Markdown's block grammar.
    "text.title",
    "markup.heading",
    "text.literal",
    "markup.raw",
    "text.uri",
    "markup.link",
    "punctuation.special",
];

/// Code over this size (bytes) isn't highlighted: a guard for hostile or runaway content.
pub const MAX_LEN: usize = 256 * 1024;

/// Styles for highlight captures: colour, weight and italic only, so lines keep their height.
#[derive(Clone, Debug, PartialEq)]
pub struct SyntaxTheme {
    styles: Vec<Option<HighlightStyle>>,
}

fn color(c: Hsla) -> HighlightStyle {
    HighlightStyle {
        color: Some(c),
        ..HighlightStyle::default()
    }
}

impl SyntaxTheme {
    /// The theme's `syntax_*` tokens.
    pub fn from_theme(theme: &Theme) -> Self {
        let c = &theme.colors;
        let styles = NAMES
            .iter()
            .map(|name| {
                let s = match *name {
                    "keyword" | "operator" | "module" => color(c.syntax_keyword),
                    "string" | "string.special" | "text.literal" | "markup.raw" => {
                        color(c.syntax_string)
                    }
                    "string.escape" | "escape" => color(c.syntax_constant),
                    "comment" => HighlightStyle {
                        font_style: Some(FontStyle::Italic),
                        ..color(c.syntax_comment)
                    },
                    "function" | "function.method" | "function.macro" | "constructor" => {
                        color(c.syntax_function)
                    }
                    "type" | "type.builtin" => color(c.syntax_type),
                    "constant" | "constant.builtin" | "number" | "boolean" | "variable.builtin" => {
                        color(c.syntax_constant)
                    }
                    "property" | "label" => color(c.syntax_property),
                    "attribute" => color(c.syntax_attribute),
                    "tag" => color(c.syntax_tag),
                    "punctuation" | "punctuation.special" => color(c.syntax_punctuation),
                    "text.title" | "markup.heading" => HighlightStyle {
                        font_weight: Some(FontWeight::SEMIBOLD),
                        ..color(c.syntax_keyword)
                    },
                    "text.uri" | "markup.link" => color(c.syntax_function),
                    _ => return None,
                };
                Some(s)
            })
            .collect();
        Self { styles }
    }

    /// Sets the style for a capture name (one of those [`names`](Self::names) lists), or clears
    /// it with `None` so that capture keeps the text's look. Only its colour, weight and italic
    /// are kept: no background, underline, strikethrough or fade.
    pub fn set(&mut self, name: &str, style: Option<HighlightStyle>) {
        if let Some(i) = NAMES.iter().position(|n| *n == name) {
            self.styles[i] = style.map(|s| HighlightStyle {
                color: s.color,
                font_weight: s.font_weight,
                font_style: s.font_style,
                ..HighlightStyle::default()
            });
        }
    }

    /// The capture names it styles.
    pub fn names() -> &'static [&'static str] {
        NAMES
    }
}

/// A language: its names as a code fence writes them, its grammar and its queries.
pub struct LanguageConfig {
    /// The first is its name; the rest are aliases. Matched case aside.
    pub names: Vec<String>,
    pub language: tree_sitter::Language,
    pub highlights: String,
    pub injections: String,
    pub locals: String,
}

struct Entry {
    names: Vec<String>,
    build: Box<dyn Fn() -> LanguageConfig + Send + Sync>,
    config: OnceLock<Option<HighlightConfiguration>>,
}

impl Entry {
    fn config(&self) -> Option<&HighlightConfiguration> {
        self.config
            .get_or_init(|| {
                let l = (self.build)();
                let mut c = HighlightConfiguration::new(
                    l.language,
                    l.names.first().cloned().unwrap_or_default(),
                    &l.highlights,
                    &l.injections,
                    &l.locals,
                )
                .ok()?;
                c.configure(NAMES);
                Some(c)
            })
            .as_ref()
    }
}

/// The languages code can be highlighted in. Cheap to clone. Each grammar's queries are
/// compiled the first time code in it is highlighted.
#[derive(Clone)]
pub struct Highlighter {
    langs: Vec<Arc<Entry>>,
}

impl Default for Highlighter {
    fn default() -> Self {
        Self::new()
    }
}

fn lang(names: &[&str], build: impl Fn() -> LanguageConfig + Send + Sync + 'static) -> Arc<Entry> {
    Arc::new(Entry {
        names: names.iter().map(|s| s.to_string()).collect(),
        build: Box::new(build),
        config: OnceLock::new(),
    })
}

fn cfg(
    names: &[&str],
    language: tree_sitter::Language,
    highlights: &[&str],
    injections: &str,
    locals: &str,
) -> LanguageConfig {
    LanguageConfig {
        names: names.iter().map(|s| s.to_string()).collect(),
        language,
        highlights: highlights.join("\n"),
        injections: injections.into(),
        locals: locals.into(),
    }
}

impl Highlighter {
    /// Rust, Swift, Go, TypeScript (and TSX), JavaScript (and JSX), Python, JSON, TOML, YAML,
    /// Bash, SQL, Markdown, HTML, CSS, C and C++, under the names code fences use.
    pub fn new() -> Self {
        use tree_sitter_javascript as js;
        let langs = vec![
            lang(&["rust", "rs"], || {
                cfg(
                    &["rust"],
                    tree_sitter_rust::LANGUAGE.into(),
                    &[tree_sitter_rust::HIGHLIGHTS_QUERY],
                    "",
                    "",
                )
            }),
            lang(&["swift"], || {
                cfg(
                    &["swift"],
                    tree_sitter_swift::LANGUAGE.into(),
                    &[tree_sitter_swift::HIGHLIGHTS_QUERY],
                    "",
                    tree_sitter_swift::LOCALS_QUERY,
                )
            }),
            lang(&["go", "golang"], || {
                cfg(
                    &["go"],
                    tree_sitter_go::LANGUAGE.into(),
                    &[tree_sitter_go::HIGHLIGHTS_QUERY],
                    "",
                    "",
                )
            }),
            lang(&["typescript", "ts", "mts", "cts"], || {
                cfg(
                    &["typescript"],
                    tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
                    &[
                        js::HIGHLIGHT_QUERY,
                        tree_sitter_typescript::HIGHLIGHTS_QUERY,
                    ],
                    "",
                    tree_sitter_typescript::LOCALS_QUERY,
                )
            }),
            lang(&["tsx"], || {
                cfg(
                    &["tsx"],
                    tree_sitter_typescript::LANGUAGE_TSX.into(),
                    &[
                        js::HIGHLIGHT_QUERY,
                        js::JSX_HIGHLIGHT_QUERY,
                        tree_sitter_typescript::HIGHLIGHTS_QUERY,
                    ],
                    "",
                    tree_sitter_typescript::LOCALS_QUERY,
                )
            }),
            lang(&["javascript", "js", "mjs", "cjs", "jsx"], || {
                cfg(
                    &["javascript"],
                    js::LANGUAGE.into(),
                    &[js::HIGHLIGHT_QUERY, js::JSX_HIGHLIGHT_QUERY],
                    "",
                    js::LOCALS_QUERY,
                )
            }),
            lang(&["python", "py"], || {
                cfg(
                    &["python"],
                    tree_sitter_python::LANGUAGE.into(),
                    &[tree_sitter_python::HIGHLIGHTS_QUERY],
                    "",
                    "",
                )
            }),
            lang(&["json", "jsonc", "json5"], || {
                cfg(
                    &["json"],
                    tree_sitter_json::LANGUAGE.into(),
                    &[tree_sitter_json::HIGHLIGHTS_QUERY],
                    "",
                    "",
                )
            }),
            lang(&["toml"], || {
                cfg(
                    &["toml"],
                    tree_sitter_toml_ng::LANGUAGE.into(),
                    &[tree_sitter_toml_ng::HIGHLIGHTS_QUERY],
                    "",
                    "",
                )
            }),
            lang(&["yaml", "yml"], || {
                cfg(
                    &["yaml"],
                    tree_sitter_yaml::LANGUAGE.into(),
                    &[tree_sitter_yaml::HIGHLIGHTS_QUERY],
                    "",
                    "",
                )
            }),
            lang(&["bash", "sh", "shell", "zsh", "console"], || {
                cfg(
                    &["bash"],
                    tree_sitter_bash::LANGUAGE.into(),
                    &[tree_sitter_bash::HIGHLIGHT_QUERY],
                    "",
                    "",
                )
            }),
            lang(&["sql"], || {
                cfg(
                    &["sql"],
                    tree_sitter_sequel::LANGUAGE.into(),
                    &[tree_sitter_sequel::HIGHLIGHTS_QUERY],
                    "",
                    "",
                )
            }),
            lang(&["markdown", "md"], || {
                cfg(
                    &["markdown"],
                    tree_sitter_md::LANGUAGE.into(),
                    &[tree_sitter_md::HIGHLIGHT_QUERY_BLOCK],
                    "",
                    "",
                )
            }),
            lang(&["html", "htm"], || {
                cfg(
                    &["html"],
                    tree_sitter_html::LANGUAGE.into(),
                    &[tree_sitter_html::HIGHLIGHTS_QUERY],
                    "",
                    "",
                )
            }),
            lang(&["css"], || {
                cfg(
                    &["css"],
                    tree_sitter_css::LANGUAGE.into(),
                    &[tree_sitter_css::HIGHLIGHTS_QUERY],
                    "",
                    "",
                )
            }),
            lang(&["c", "h"], || {
                cfg(
                    &["c"],
                    tree_sitter_c::LANGUAGE.into(),
                    &[tree_sitter_c::HIGHLIGHT_QUERY],
                    "",
                    "",
                )
            }),
            lang(&["cpp", "c++", "cc", "cxx", "hpp", "hh", "hxx"], || {
                cfg(
                    &["cpp"],
                    tree_sitter_cpp::LANGUAGE.into(),
                    &[
                        tree_sitter_c::HIGHLIGHT_QUERY,
                        tree_sitter_cpp::HIGHLIGHT_QUERY,
                    ],
                    "",
                    "",
                )
            }),
        ];
        Self { langs }
    }

    /// Adds a language, ahead of the ones it has (so it can replace one under the same name).
    pub fn register(&mut self, language: LanguageConfig) {
        let names = language
            .names
            .iter()
            .map(|n| n.to_lowercase())
            .collect::<Vec<_>>();
        let cell = std::sync::Mutex::new(Some(language));
        let entry = Arc::new(Entry {
            names,
            build: Box::new(move || {
                cell.lock()
                    .ok()
                    .and_then(|mut l| l.take())
                    .expect("a registered language is built once")
            }),
            config: OnceLock::new(),
        });
        self.langs.insert(0, entry);
    }

    /// Whether it knows `lang` (a code fence's info: case aside, its first word).
    pub fn knows(&self, lang: &str) -> bool {
        self.entry(lang).is_some()
    }

    fn entry(&self, lang: &str) -> Option<&Arc<Entry>> {
        let word = lang
            .split(|c: char| c.is_whitespace() || c == ',' || c == '{')
            .next()
            .unwrap_or("")
            .to_lowercase();
        if word.is_empty() {
            return None;
        }
        self.langs.iter().find(|e| e.names.contains(&word))
    }

    /// `code`'s ranges (bytes, on character boundaries, in order, none overlapping) and their
    /// styles. An unknown language, code that doesn't parse at all, or code over [`MAX_LEN`]
    /// gives none.
    pub fn highlight(&self, lang: &str, code: &str, theme: &SyntaxTheme) -> Highlights {
        if code.len() > MAX_LEN {
            return Vec::new();
        }
        let Some(config) = self.entry(lang).and_then(|e| e.config()) else {
            return Vec::new();
        };
        thread_local! {
            static HIGHLIGHTER: RefCell<tree_sitter_highlight::Highlighter> =
                RefCell::new(tree_sitter_highlight::Highlighter::new());
        }
        HIGHLIGHTER.with(|h| {
            let mut h = h.borrow_mut();
            let Ok(events) = h.highlight(config, code.as_bytes(), None, |_| None) else {
                return Vec::new();
            };
            let mut out: Highlights = Vec::new();
            let mut stack: Vec<usize> = Vec::new();
            for event in events {
                match event {
                    Ok(HighlightEvent::HighlightStart(h)) => stack.push(h.0),
                    Ok(HighlightEvent::HighlightEnd) => {
                        stack.pop();
                    }
                    Ok(HighlightEvent::Source { start, end }) => {
                        // The innermost capture with a style wins.
                        let style = stack
                            .iter()
                            .rev()
                            .find_map(|i| theme.styles.get(*i).copied().flatten());
                        let Some(style) = style else { continue };
                        match out.last_mut() {
                            Some((r, s)) if r.end == start && *s == style => r.end = end,
                            _ => out.push((start..end, style)),
                        }
                    }
                    Err(_) => return Vec::new(),
                }
            }
            out
        })
    }

    /// A [`HighlightFn`] for hosts to hand TextView, gpui-blocks and gpui-chatwidgets. It
    /// follows `theme` as given: on a theme switch, make a new one.
    pub fn into_fn(self, theme: SyntaxTheme) -> HighlightFn {
        Arc::new(move |lang: &str, code: &str| self.highlight(lang, code, &theme))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn theme() -> SyntaxTheme {
        SyntaxTheme::from_theme(&Theme::dark())
    }

    fn style_of(spans: &Highlights, code: &str, text: &str) -> Option<HighlightStyle> {
        let at = code.find(text)?;
        spans
            .iter()
            .find(|(r, _)| r.start <= at && at + text.len() <= r.end)
            .map(|(_, s)| *s)
    }

    #[test]
    fn rust_comes_out_in_the_themes_colours() {
        let t = Theme::dark();
        let code = "fn main() {\n    let n = 42; // the answer\n    println!(\"hi\");\n}";
        let spans = Highlighter::new().highlight("rust", code, &theme());
        let colour = |text| style_of(&spans, code, text).and_then(|s| s.color);
        assert_eq!(colour("fn"), Some(t.colors.syntax_keyword));
        assert_eq!(colour("main"), Some(t.colors.syntax_function));
        assert_eq!(colour("42"), Some(t.colors.syntax_constant));
        assert_eq!(colour("\"hi\""), Some(t.colors.syntax_string));
        let comment = style_of(&spans, code, "// the answer").unwrap();
        assert_eq!(comment.color, Some(t.colors.syntax_comment));
        assert_eq!(comment.font_style, Some(FontStyle::Italic));
    }

    #[test]
    fn a_fence_is_matched_case_aside_by_its_first_word_and_aliases() {
        let h = Highlighter::new();
        for lang in [
            "Rust",
            "rust,ignore",
            "rs",
            "py",
            "js",
            "mjs",
            "cjs",
            "jsx",
            "ts",
            "tsx",
            "sh",
            "shell",
            "zsh",
            "console",
            "yml",
            "jsonc",
            "json5",
            "c++",
            "cpp",
            "cc",
            "cxx",
            "hpp",
            "hh",
            "h",
            "md",
            "htm",
            "golang",
            "swift",
            "toml",
            "sql",
            "css",
            "html",
            "go",
        ] {
            assert!(h.knows(lang), "{lang}");
        }
        assert!(!h.knows("brainfuck"));
        assert!(!h.knows(""));
        assert!(h.highlight("brainfuck", "+++", &theme()).is_empty());
    }

    #[test]
    fn every_language_highlights_a_sample() {
        let h = Highlighter::new();
        let samples = [
            ("rust", "fn f() -> u8 { 1 }"),
            ("swift", "let x: Int = 1 // one"),
            ("go", "func main() { return }"),
            ("ts", "const x: number = 1;"),
            ("tsx", "const a = <div className=\"x\">{1}</div>;"),
            ("js", "function f() { return 'a'; }"),
            ("python", "def f():\n    return \"a\""),
            ("json", "{\"a\": 1, \"b\": true}"),
            ("toml", "[a]\nb = \"c\""),
            ("yaml", "a: 1\nb: \"c\""),
            ("bash", "echo \"hi\" # say"),
            ("sql", "SELECT a FROM t WHERE b = 1;"),
            ("markdown", "# Title\n\n```\ncode\n```"),
            ("html", "<p class=\"a\">hi</p>"),
            ("css", ".a { color: red; }"),
            ("c", "int main(void) { return 0; }"),
            ("cpp", "namespace a { class B {}; }"),
        ];
        for (lang, code) in samples {
            assert!(!h.highlight(lang, code, &theme()).is_empty(), "{lang}");
        }
    }

    #[test]
    fn ranges_are_in_order_apart_and_on_character_boundaries() {
        let code = "let s = \"h\u{e9}llo \u{1f393}\"; // \u{e9}t\u{e9}\nlet n = 3;";
        let spans = Highlighter::new().highlight("rust", code, &theme());
        assert!(!spans.is_empty());
        let mut end = 0;
        for (r, _) in &spans {
            assert!(r.start >= end && r.start < r.end, "{spans:?}");
            assert!(code.is_char_boundary(r.start) && code.is_char_boundary(r.end));
            end = r.end;
        }
    }

    #[test]
    fn the_innermost_capture_wins() {
        // An escape inside a string takes the escape's colour, not the string's.
        let t = Theme::dark();
        let code = "let s = \"a\\nb\";";
        let spans = Highlighter::new().highlight("rust", code, &theme());
        let colour = |text| style_of(&spans, code, text).and_then(|s| s.color);
        assert_eq!(colour("\\n"), Some(t.colors.syntax_constant));
        assert_eq!(colour("\"a"), Some(t.colors.syntax_string));
    }

    #[test]
    fn code_past_the_limit_isnt_highlighted() {
        let code = "let a = 1;\n".repeat(MAX_LEN / 10);
        assert!(code.len() > MAX_LEN);
        assert!(
            Highlighter::new()
                .highlight("rust", &code, &theme())
                .is_empty()
        );
    }

    #[test]
    fn a_host_can_restyle_or_clear_a_capture() {
        let code = "fn main() {}";
        let mut t = theme();
        t.set("keyword", None);
        let spans = Highlighter::new().highlight("rust", code, &t);
        assert!(style_of(&spans, code, "fn").is_none());
        let red: Hsla = gpui::red();
        t.set(
            "function",
            Some(HighlightStyle {
                color: Some(red),
                background_color: Some(red),
                ..Default::default()
            }),
        );
        let spans = Highlighter::new().highlight("rust", code, &t);
        let s = style_of(&spans, code, "main").unwrap();
        assert_eq!(s.color, Some(red));
        assert_eq!(
            s.background_color, None,
            "only colour, weight and italic are kept"
        );
    }

    #[test]
    fn a_registered_language_comes_first() {
        let mut h = Highlighter::new();
        h.register(LanguageConfig {
            names: vec!["Plain".into(), "rs".into()],
            language: tree_sitter_json::LANGUAGE.into(),
            highlights: tree_sitter_json::HIGHLIGHTS_QUERY.into(),
            injections: String::new(),
            locals: String::new(),
        });
        assert!(h.knows("plain"));
        // Under a name the highlighter has, the registered one comes first: with no highlights
        // query, Rust comes out plain.
        h.register(LanguageConfig {
            names: vec!["rust".into()],
            language: tree_sitter_rust::LANGUAGE.into(),
            highlights: String::new(),
            injections: String::new(),
            locals: String::new(),
        });
        assert!(h.highlight("rust", "fn main() {}", &theme()).is_empty());
    }

    #[test]
    fn every_language_compiles_its_queries() {
        let h = Highlighter::new();
        for e in &h.langs {
            assert!(e.config().is_some(), "{:?}", e.names);
        }
    }
}
