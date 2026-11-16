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
    let options = markdown::ParseOptions::gfm();
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
            Node::Text(t) => p.push(&t.value, marks.clone()),
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
        let b = parse("<script>alert(1)</script>\n\n![a cat](https://x.test/cat.png)");
        assert!(matches!(&b[0], Block::Paragraph(p) if p.text.contains("<script>")));
        assert!(matches!(&b[1], Block::Paragraph(p) if p.text == "a cat" && p.spans[0].1.muted));
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
