//! Code blocks: the code in a monospace font on a dark ground, lightly highlighted, scrolling
//! sideways when a line is long, with a Copy button.

use gpui::prelude::FluentBuilder as _;
use gpui::*;

use super::content::Glyph;
use super::{Draw, pressable};
use crate::theme::CodeColors;

const KEYWORDS: &[&str] = &[
    "if",
    "then",
    "else",
    "elif",
    "fi",
    "for",
    "while",
    "do",
    "done",
    "case",
    "esac",
    "in",
    "function",
    "return",
    "export",
    "def",
    "class",
    "import",
    "from",
    "as",
    "with",
    "try",
    "except",
    "finally",
    "raise",
    "lambda",
    "yield",
    "pass",
    "None",
    "True",
    "False",
    "const",
    "let",
    "var",
    "new",
    "this",
    "async",
    "await",
    "switch",
    "break",
    "continue",
    "throw",
    "catch",
    "typeof",
    "true",
    "false",
    "null",
    "undefined",
    "func",
    "struct",
    "enum",
    "type",
    "package",
    "go",
    "defer",
    "select",
    "interface",
    "fn",
    "pub",
    "use",
    "mod",
    "impl",
    "match",
    "mut",
    "self",
    "Self",
    "trait",
    "where",
    "loop",
];

/// The code split into coloured runs: strings, numbers, comments and common keywords; in a
/// shell, the command and its flags too. Returns (byte range, colour) for every run.
pub(crate) fn highlight(
    code: &str,
    language: &str,
    c: &CodeColors,
) -> Vec<(std::ops::Range<usize>, Hsla)> {
    let lang = language.to_ascii_lowercase();
    let shell = matches!(lang.as_str(), "bash" | "sh" | "shell" | "zsh" | "console");
    let hash_comments = shell
        || matches!(
            lang.as_str(),
            "python" | "py" | "ruby" | "yaml" | "yml" | "toml"
        );
    let mut runs = Vec::new();
    let mut offset = 0;
    for line in code.split_inclusive('\n') {
        let bytes = line.as_bytes();
        let mut i = 0;
        let mut expect_command = shell;
        while i < bytes.len() {
            let ch = bytes[i];
            let start = offset + i;
            if (ch == b'#' && hash_comments) || line[i..].starts_with("//") {
                runs.push((
                    start..offset + line.trim_end_matches('\n').len().max(i),
                    c.comment,
                ));
                break;
            }
            if ch == b'"' || ch == b'\'' || ch == b'`' {
                let mut j = i + 1;
                while j < bytes.len() && bytes[j] != ch && bytes[j] != b'\n' {
                    if bytes[j] == b'\\' {
                        j += 1;
                    }
                    j += 1;
                }
                let end = (j + 1).min(bytes.len());
                runs.push((start..offset + end, c.string));
                i = end;
                expect_command = false;
                continue;
            }
            if ch.is_ascii_alphanumeric() || ch == b'_' || ch == b'-' || ch >= 0x80 {
                let mut j = i;
                while j < bytes.len()
                    && (bytes[j].is_ascii_alphanumeric()
                        || bytes[j] == b'_'
                        || bytes[j] == b'-'
                        || bytes[j] == b'.'
                        || bytes[j] >= 0x80)
                {
                    j += 1;
                }
                // Never split a character.
                while j < bytes.len() && !line.is_char_boundary(j) {
                    j += 1;
                }
                let word = &line[i..j];
                let color = if word.chars().next().is_some_and(|c| c.is_ascii_digit()) {
                    Some(c.number)
                } else if shell && expect_command {
                    Some(c.command)
                } else if shell && word.starts_with('-') {
                    Some(c.flag)
                } else if KEYWORDS.contains(&word) {
                    Some(c.keyword)
                } else {
                    None
                };
                if let Some(color) = color {
                    runs.push((start..offset + j, color));
                }
                expect_command = false;
                i = j;
                continue;
            }
            if shell && (ch == b'|' || ch == b';' || ch == b'&') {
                expect_command = true;
            }
            i += 1;
            while i < bytes.len() && !line.is_char_boundary(i) {
                i += 1;
            }
        }
        offset += line.len();
    }
    runs
}

impl Draw<'_, '_> {
    pub fn code(&mut self, language: &str, code: &str, path: &str) -> AnyElement {
        let t = self.t;
        let limit = self.v.limits.code_chars;
        let (code, cut) = match code.char_indices().nth(limit) {
            Some((at, _)) => (&code[..at], true),
            None => (code, false),
        };
        let code = code.trim_end_matches('\n').to_string();
        let colors = t.colors.code;
        let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> =
            highlight(&code, language, &colors)
                .into_iter()
                .map(|(r, c)| {
                    (
                        r,
                        HighlightStyle {
                            color: Some(c),
                            ..Default::default()
                        },
                    )
                })
                .collect();
        let text = StyledText::new(code.clone()).with_highlights(highlights);
        let copied = self.v.ui.copied.as_deref() == Some(path);
        let (key, source) = (path.to_string(), code.clone());
        let copy_it = self
            .cx
            .listener(move |this, _, _, cx| this.copy(&key, &source, cx));
        let copy = pressable(self.id(&format!("{path}.copy")), self.t)
            .accessibility_label("Copy")
            .focus_on_press(false)
            .flex_none()
            .h(self.px(26.))
            .px(self.px(8.))
            .gap(self.px(4.))
            .rounded(self.px(t.radii.sm))
            .bg(colors.background)
            .border_1()
            .border_color(Hsla {
                a: 0.14,
                ..colors.text
            })
            .hover(|s| {
                s.bg(Hsla {
                    a: 0.08,
                    ..colors.text
                })
            })
            .debug_selector({
                let sel = self.selector(&format!("copy:{path}"));
                move || sel
            })
            .child(self.glyph(
                if copied { Glyph::Check } else { Glyph::Copy },
                12.,
                colors.text,
            ))
            .child(self.line(
                if copied { "Copied" } else { "Copy" },
                t.text.xs,
                colors.text,
                self.regular(),
            ));
        let copy = self.on_press(copy, copy_it);
        let (scroller, handle) = self.scroll_x(&format!("{path}.scroll"), &format!("{path}.code"));
        // The code takes its own width (at least the box's): what a scroller can scroll is what
        // its children take.
        let scroller = scroller.flex().child(
            div()
                .flex_none()
                .min_w_full()
                .p(self.px(13.))
                .pr(self.px(72.))
                .font_family(t.fonts.mono.clone())
                .text_size(self.px(t.text.code))
                .line_height(self.px(t.text.code * 1.5))
                .text_color(colors.text)
                .whitespace_nowrap()
                .debug_selector({
                    let sel = self.selector(&format!("code:{path}"));
                    move || sel
                })
                .child(text),
        );
        let boxed = self.keep_sideways(div().relative(), handle);
        div()
            .w_full()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(self.px(4.))
            .child(
                boxed
                    .w_full()
                    .min_w_0()
                    .rounded(self.px(t.radii.md))
                    .bg(colors.background)
                    .overflow_hidden()
                    .child(scroller)
                    .child(
                        div()
                            .absolute()
                            .top(self.px(8.))
                            .right(self.px(8.))
                            .child(copy),
                    ),
            )
            .when(cut, |d| {
                d.child(self.text(
                    "The rest of this code is too long to show.",
                    t.text.sm,
                    t.colors.text_muted,
                    self.regular(),
                ))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // The built-in #[test], not GPUI's, which gpui::* brings in.
    use crate::CardTheme;
    use std::prelude::v1::test;

    #[test]
    fn highlighting_never_splits_a_character_and_stays_in_bounds() {
        let c = CardTheme::dark().colors.code;
        for (code, lang) in [
            ("echo \"héllo\" | grep -i é # done", "bash"),
            ("fn main() { let x = 42; } // ok", "rust"),
            ("'unterminated", "js"),
            ("日本語 'x' 1", "py"),
            ("", ""),
        ] {
            for (r, _) in highlight(code, lang, &c) {
                assert!(r.start <= r.end && r.end <= code.len(), "{code}: {r:?}");
                assert!(
                    code.is_char_boundary(r.start) && code.is_char_boundary(r.end),
                    "{code}: {r:?}"
                );
            }
        }
        let runs = highlight("ls -la", "bash", &c);
        assert_eq!(runs, vec![(0..2, c.command), (3..6, c.flag)]);
    }
}
