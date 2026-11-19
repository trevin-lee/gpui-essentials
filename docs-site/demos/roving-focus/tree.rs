// A tree: the host draws the rows in view, each with its depth and whether it's open. Up and
// Down move, Right opens a row or goes to its first child, Left folds it or goes to its parent.
pub struct Example {
    open: Vec<&'static str>,
}

impl Example {
    pub fn new(_: &mut Window, _: &mut Context<Self>) -> Self {
        Self { open: vec!["docs"] }
    }
}

/// The folders and pages: id, name, depth, and whether it has children.
const ROWS: [(&str, &str, usize, bool); 6] = [
    ("docs", "Docs", 1, true),
    ("guides", "Guides", 2, true),
    ("start", "Getting started", 3, false),
    ("api", "API", 2, false),
    ("notes", "Notes", 1, true),
    ("ideas", "Ideas", 2, false),
];

impl Render for Example {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let host = cx.entity().downgrade();
        let mut tree = RovingFocus::new("pages")
            .accessibility_label("Pages")
            .tree(move |id, open, _, cx| {
                let id = ROWS.iter().find(|r| r.0 == id.as_ref()).map(|r| r.0);
                host.update(cx, |this, cx| {
                    if let Some(id) = id {
                        this.open.retain(|o| *o != id);
                        if open {
                            this.open.push(id);
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .w(rems(18.));
        // A folded row's children aren't rows.
        let mut folded_at: Option<usize> = None;
        for (id, name, level, parent) in ROWS {
            if folded_at.is_some_and(|l| level > l) {
                continue;
            }
            folded_at = None;
            let open = self.open.contains(&id);
            if parent && !open {
                folded_at = Some(level);
            }
            let chevron = if open { IconName::ChevronDown } else { IconName::ChevronRight };
            tree = tree.tree_row(
                id,
                name,
                level,
                parent.then_some(open),
                base::Press::new(id)
                    .flex()
                    .items_center()
                    .gap(rems(0.375))
                    .h(rems(1.875))
                    .pl(rems(0.5 + 1.25 * (level - 1) as f32))
                    .rounded(theme.radius_md())
                    .text_sm()
                    .child(
                        div()
                            .w(rems(1.))
                            .children(parent.then(|| Icon::new(chevron).color(theme.muted_foreground))),
                    )
                    .child(name),
            );
        }
        tree
    }
}
