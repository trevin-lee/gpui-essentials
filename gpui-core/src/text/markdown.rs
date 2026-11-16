//! Markdown (GitHub's flavour) to the blocks TextView draws. Nothing in it is ever run or
//! fetched: raw HTML shows as its text, and an image shows as its description.

use markdown::mdast::{AlignKind, Node};

use super::paragraph::{Marks, Paragraph};

/// The most text, and the deepest nesting, a TextView takes; past them the rest is dropped, so
/// hostile input can't stall a frame.
const MAX_LEN: usize = 512 * 1024;
const MAX_DEPTH: usize = 32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Paragraph(Paragraph),
    Heading(u8, Paragraph),
    Code {
        lang: Option<String>,
        text: String,
    },
    Quote(Vec<Block>),
    List {
        start: Option<u32>,
        items: Vec<Item>,
    },
    Rule,
    /// The first row is the head.
    Table {
        align: Vec<Align>,
        rows: Vec<Vec<Paragraph>>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    /// A task list item's box: ticked or not.
    pub checked: Option<bool>,
    pub blocks: Vec<Block>,
}

pub fn parse(source: &str) -> Vec<Block> {
    let mut end = source.len().min(MAX_LEN);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    // Raw HTML is never rendered, so it isn't parsed either: it reads as the text it is, and
    // the Markdown after it isn't swallowed into an HTML block (which runs to a blank line).
    let mut options = markdown::ParseOptions::gfm();
    options.constructs.html_flow = false;
    options.constructs.html_text = false;
    match markdown::to_mdast(&source[..end], &options) {
        Ok(Node::Root(root)) => blocks(&root.children, 0),
        // Markdown without MDX has no syntax errors; if anything comes back, show the text.
        _ => vec![Block::Paragraph(Paragraph::plain(&source[..end]))],
    }
}

fn blocks(nodes: &[Node], depth: usize) -> Vec<Block> {
    if depth > MAX_DEPTH {
        return Vec::new();
    }
    let mut out = Vec::new();
    for n in nodes {
        match n {
            Node::Paragraph(p) => out.push(Block::Paragraph(inline(&p.children, depth))),
            Node::Heading(h) => out.push(Block::Heading(h.depth, inline(&h.children, depth))),
            Node::Code(c) => out.push(Block::Code {
                lang: c.lang.clone(),
                text: c.value.clone(),
            }),
            Node::Math(m) => out.push(Block::Code {
                lang: Some("math".into()),
                text: m.value.clone(),
            }),
            Node::Blockquote(q) => out.push(Block::Quote(blocks(&q.children, depth + 1))),
            Node::List(l) => out.push(Block::List {
                start: l.ordered.then(|| l.start.unwrap_or(1)),
                items: l
                    .children
                    .iter()
                    .filter_map(|c| match c {
                        Node::ListItem(i) => Some(Item {
                            checked: i.checked,
                            blocks: blocks(&i.children, depth + 1),
                        }),
                        _ => None,
                    })
                    .collect(),
            }),
            Node::ThematicBreak(_) => out.push(Block::Rule),
            Node::Table(t) => out.push(Block::Table {
                align: t
                    .align
                    .iter()
                    .map(|a| match a {
                        AlignKind::Center => Align::Center,
                        AlignKind::Right => Align::Right,
                        _ => Align::Left,
                    })
                    .collect(),
                rows: t
                    .children
                    .iter()
                    .filter_map(|r| match r {
                        Node::TableRow(r) => Some(
                            r.children
                                .iter()
                                .map(|c| match c {
                                    Node::TableCell(c) => inline(&c.children, depth),
                                    _ => Paragraph::default(),
                                })
                                .collect(),
                        ),
                        _ => None,
                    })
                    .collect(),
            }),
            Node::Html(h) => out.push(Block::Paragraph(Paragraph::plain(h.value.clone()))),
            Node::FootnoteDefinition(f) => out.extend(blocks(&f.children, depth + 1)),
            // Frontmatter and the like: nothing to show.
            _ => {}
        }
    }
    out
}

/// A text node's soft line breaks as spaces, as CommonMark and the web read them: a single line
/// ending inside a paragraph, with the spaces and tabs either side of it, is one space. (A hard
/// break, two spaces or a backslash before the line ending, is a `Break` node, kept as a line.)
fn soft_breaks(text: &str) -> std::borrow::Cow<'_, str> {
    if !text.contains(['\n', '\r']) {
        return text.into();
    }
    let lines: Vec<&str> = text.split('\n').collect();
    let last = lines.len() - 1;
    let mut out = String::with_capacity(text.len());
    for (i, line) in lines.into_iter().enumerate() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        let line = if i > 0 {
            line.trim_start_matches([' ', '\t'])
        } else {
            line
        };
        let line = if i < last {
            line.trim_end_matches([' ', '\t'])
        } else {
            line
        };
        if i > 0 {
            out.push(' ');
        }
        out.push_str(line);
    }
    out.into()
}

fn inline(nodes: &[Node], depth: usize) -> Paragraph {
    let mut p = Paragraph::default();
    walk(nodes, &Marks::default(), &mut p, depth);
    p
}

fn walk(nodes: &[Node], marks: &Marks, p: &mut Paragraph, depth: usize) {
    if depth > MAX_DEPTH {
        return;
    }
    for n in nodes {
        match n {
            Node::Text(t) => p.push(&soft_breaks(&t.value), marks.clone()),
            Node::Emphasis(e) => walk(
                &e.children,
                &Marks {
                    italic: true,
                    ..marks.clone()
                },
                p,
                depth + 1,
            ),
            Node::Strong(s) => walk(
                &s.children,
                &Marks {
                    bold: true,
                    ..marks.clone()
                },
                p,
                depth + 1,
            ),
            Node::Delete(d) => walk(
                &d.children,
                &Marks {
                    strike: true,
                    ..marks.clone()
                },
                p,
                depth + 1,
            ),
            Node::InlineCode(c) => p.push(
                &c.value,
                Marks {
                    code: true,
                    ..marks.clone()
                },
            ),
            Node::InlineMath(m) => p.push(
                &m.value,
                Marks {
                    code: true,
                    ..marks.clone()
                },
            ),
            Node::Break(_) => p.push("\n", marks.clone()),
            Node::Link(l) => {
                let link = Some(l.url.clone().into());
                walk(
                    &l.children,
                    &Marks {
                        link,
                        ..marks.clone()
                    },
                    p,
                    depth + 1,
                )
            }
            Node::Image(i) => {
                let alt = if i.alt.is_empty() {
                    "image"
                } else {
                    i.alt.as_str()
                };
                p.push(
                    alt,
                    Marks {
                        muted: true,
                        ..marks.clone()
                    },
                )
            }
            Node::Html(h) => p.push(&h.value, marks.clone()),
            Node::FootnoteReference(f) => p.push(
                &format!("[{}]", f.identifier),
                Marks {
                    muted: true,
                    ..marks.clone()
                },
            ),
            other => {
                if let Some(children) = other.children() {
                    walk(children, marks, p, depth + 1)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_and_marks() {
        let b = parse(
            "# Title\n\nSome **bold** and [a link](https://x.test) and `code`.\n\n- [x] done\n- [ ] open\n\n```rust\nfn main() {}\n```\n\n> quoted\n\n---\n\n| a | b |\n|:-:|--:|\n| 1 | 2 |\n",
        );
        assert!(matches!(&b[0], Block::Heading(1, p) if p.text == "Title"));
        let Block::Paragraph(p) = &b[1] else { panic!() };
        assert_eq!(p.text, "Some bold and a link and code.");
        assert!(
            p.spans
                .iter()
                .any(|(r, m)| m.bold && &p.text[r.clone()] == "bold")
        );
        assert_eq!(
            p.link_at(p.text.find("link").unwrap()).map(|s| s.as_ref()),
            Some("https://x.test")
        );
        assert!(
            matches!(&b[2], Block::List { start: None, items } if items[0].checked == Some(true) && items[1].checked == Some(false))
        );
        assert!(
            matches!(&b[3], Block::Code { lang: Some(l), text } if l == "rust" && text == "fn main() {}")
        );
        assert!(matches!(&b[4], Block::Quote(_)));
        assert!(matches!(&b[5], Block::Rule));
        assert!(
            matches!(&b[6], Block::Table { align, rows } if align == &[Align::Center, Align::Right] && rows.len() == 2)
        );
    }

    #[test]
    fn html_is_text_and_images_are_their_words() {
        // HTML right before Markdown, with no blank line between: the Markdown is still read.
        let h = parse("<div>raw</div>\n# A heading\n- an item");
        assert!(
            matches!(&h[0], Block::Paragraph(p) if p.text.contains("<div>raw</div>")),
            "{h:?}"
        );
        assert!(
            h.iter()
                .any(|b| matches!(b, Block::Heading(1, p) if p.text == "A heading")),
            "{h:?}"
        );
        assert!(h.iter().any(|b| matches!(b, Block::List { .. })), "{h:?}");
        let b = parse("<script>alert(1)</script>\n\n![a cat](https://x.test/cat.png)");
        assert!(matches!(&b[0], Block::Paragraph(p) if p.text.contains("<script>")));
        assert!(matches!(&b[1], Block::Paragraph(p) if p.text == "a cat" && p.spans[0].1.muted));
    }

    #[test]
    fn a_single_line_ending_is_a_space_and_a_hard_break_a_line() {
        let b = parse(
            "one line\nand the next,  \nthen a hard break\\\nand another\r\n  indented **bold\nacross**",
        );
        let Block::Paragraph(p) = &b[0] else {
            panic!("{b:?}")
        };
        assert_eq!(
            p.text,
            "one line and the next,\nthen a hard break\nand another indented bold across"
        );
        assert!(
            p.spans
                .iter()
                .any(|(r, m)| m.bold && &p.text[r.clone()] == "bold across")
        );
        // In a list item and a quote too; a code block keeps its lines.
        let b = parse("- an item\n  that wraps\n\n> a quote\n> on two lines\n\n```\na\nb\n```");
        let Block::List { items, .. } = &b[0] else {
            panic!("{b:?}")
        };
        assert!(
            matches!(&items[0].blocks[0], Block::Paragraph(p) if p.text == "an item that wraps"),
            "{b:?}"
        );
        let Block::Quote(q) = &b[1] else {
            panic!("{b:?}")
        };
        assert!(
            matches!(&q[0], Block::Paragraph(p) if p.text == "a quote on two lines"),
            "{q:?}"
        );
        assert!(matches!(&b[2], Block::Code { text, .. } if text == "a\nb"));
    }

    #[test]
    fn hostile_nesting_and_size_are_cut_short() {
        let deep = ">".repeat(5000) + " x";
        let _ = parse(&deep);
        let big = "a".repeat(MAX_LEN * 2);
        let b = parse(&big);
        assert!(matches!(&b[0], Block::Paragraph(p) if p.text.len() <= MAX_LEN));
    }
}
