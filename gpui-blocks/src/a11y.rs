//! The editor's accessibility checks: every control it draws has a role and a label.

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// Every element that takes a click in gpui-blocks and its engine is a control with a role
    /// and a label: its builder calls `.control(..)` (which sets both and reports them to the
    /// audit), is a gpui-core `Button` (which labels itself), is made by a helper that does one
    /// of those, or says why it needs neither in an `// a11y:` comment. A new click handler
    /// without one fails here.
    #[test]
    fn every_click_handler_is_a_labelled_control() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut unlabelled = Vec::new();
        for dir in [root.join("src"), root.join("engine/src")] {
            let mut files: Vec<_> = std::fs::read_dir(&dir)
                .unwrap()
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "rs"))
                .collect();
            files.sort();
            for file in files {
                let text = std::fs::read_to_string(&file).unwrap();
                for (line, head) in unlabelled_handlers(&text) {
                    unlabelled.push(format!("{}:{line}: {head}", file.display()));
                }
            }
        }
        assert!(
            unlabelled.is_empty(),
            "click handlers on elements with no role and label:\n{}",
            unlabelled.join("\n")
        );
    }

    /// The click handlers in `text` (before its tests) whose element isn't a labelled control:
    /// their line and the line their builder starts at.
    fn unlabelled_handlers(text: &str) -> Vec<(usize, String)> {
        let lines: Vec<&str> = text.lines().collect();
        let end = lines
            .iter()
            .position(|l| l.starts_with("#[cfg(test)]"))
            .unwrap_or(lines.len());
        let indent = |l: &str| l.len() - l.trim_start_matches(' ').len();
        // Closures that make labelled controls: `let icon = |..| Button::new(..)..`.
        let helpers: Vec<&str> = text
            .match_indices("let ")
            .filter_map(|(at, _)| {
                let rest = &text[at + 4..];
                let name_end = rest.find(|c: char| !c.is_alphanumeric() && c != '_')?;
                let after = rest[name_end..].trim_start();
                let body = after
                    .strip_prefix("= |")
                    .or_else(|| after.strip_prefix("= move |"))?;
                let body = &body[..body.len().min(1500)];
                (body.contains(".control(") || body.contains("accessibility_label("))
                    .then_some(&rest[..name_end])
            })
            .collect();
        let mut out = Vec::new();
        for i in 0..end {
            let line = lines[i].trim_start();
            if !(line.starts_with(".on_click(") || line.starts_with(".on_mouse_down(")) {
                continue;
            }
            // The builder's first line: the nearest line above that's less indented, past
            // `.map(|this| ..)` and `match` arms over `this`.
            let mut level = indent(lines[i]);
            let mut start = i;
            while let Some(j) = (0..start)
                .rev()
                .find(|&j| !lines[j].trim().is_empty() && indent(lines[j]) < level)
            {
                start = j;
                let head = lines[start].trim();
                if !(head.ends_with("this") || head.contains("|this|")) {
                    break;
                }
                level = indent(lines[start]);
            }
            let head = lines[start].trim();
            let chunk = lines[start..=i].join("\n");
            let labelled = [".control(", "Button::new(", "// a11y:"]
                .iter()
                .any(|m| chunk.contains(m))
                || helpers.iter().any(|h| head.starts_with(&format!("{h}(")));
            if !labelled {
                out.push((i + 1, head.to_string()));
            }
        }
        out
    }

    #[test]
    fn the_scan_finds_an_unlabelled_handler() {
        let text = "fn f() {\n    div()\n        .id(\"x\")\n        .on_click(|_, _, _| {})\n}\n";
        assert_eq!(unlabelled_handlers(text), [(4, "div()".to_string())]);
        let labelled = "fn f() {\n    div()\n        .control(module_path!(), Role::Button, \"X\")\n        .on_click(|_, _, _| {})\n}\n";
        assert!(unlabelled_handlers(labelled).is_empty());
    }

    use gpui::{App, Modifiers, Role, TestAppContext, VisualTestContext};
    use markdown_editor::BlockPosition;

    use crate::docs::{DocInfo, Docs};
    use crate::{Editor, Options};

    struct OneDoc;

    impl Docs for OneDoc {
        fn info(&self, id: &str, _: &App) -> Option<DocInfo> {
            (id == "r1").then(|| DocInfo::new("Roadmap"))
        }
    }

    fn draw(cx: &mut VisualTestContext) {
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
    }

    /// A page with every kind of block and its controls showing (the side menu, the block menu,
    /// the format bar), drawn while gpui-core's audit records what each control says: none goes
    /// without a label.
    #[gpui::test]
    fn every_control_drawn_has_a_role_and_a_label(cx: &mut TestAppContext) {
        let source = [
            crate::toc::MARKDOWN.to_string(),
            "# Plans".into(),
            "Some words, and [Roadmap](doc:r1) mentioned.".into(),
            crate::docs::page("r1", "Roadmap"),
            crate::docs::embed("r1", "Roadmap", "page", 200),
            crate::bookmark::Bookmark::new("https://example.com/a").markdown(),
            "```rust\nfn main() {}\n```".into(),
            "<!-- columns -->\n\n<!-- column 0.5 -->\n\nleft\n\n<!-- column 0.5 -->\n\nright\n\n<!-- /columns -->".into(),
            "> [!NOTE] <!-- icon:💡 -->\n> A note".into(),
            "<details open>\n<summary>A toggle</summary>\n\nInside\n\n</details>".into(),
            "- [ ] A task".into(),
            "| a | b |\n| --- | --- |\n| 1 | 2 |".into(),
        ]
        .join("\n\n");
        cx.update(|cx| {
            gpui_core::init(cx);
            crate::init(cx);
        });
        let (editor, cx) = cx.add_window_view(move |window, cx| {
            let options = Options::default()
                .docs(OneDoc)
                .embeds(|_, _, _| None)
                .unfurl(|_, _| gpui::Task::ready(None))
                .comments()
                .block_link(|id| format!("doc:r1#{id}"));
            Editor::with_options(&source, options, window, cx)
        });
        cx.update(|window, cx| editor.update(cx, |e, cx| e.focus(window, cx)));
        draw(cx);
        gpui_core::audit_start();
        // The side menu beside the heading, and the block menu open.
        let heading = editor.update(cx, |e, cx| e.state().read(cx).top_level_blocks()[1]);
        cx.simulate_mouse_move(heading.1.center(), None, Modifiers::none());
        draw(cx);
        editor.update(cx, |e, cx| e.open_block_menu(heading.0, cx));
        draw(cx);
        editor.update(cx, |e, cx| e.close_block_menu(cx));
        // The format bar over a few words, selected with the pointer.
        let (from, to) = editor.update(cx, |e, cx| {
            let state = e.state().read(cx);
            let leaf = state
                .model()
                .document()
                .leaves()
                .find(|l| l.text().is_some_and(|t| t.starts_with("Some")))
                .unwrap()
                .id();
            let at = |offset| {
                state
                    .position_bounds(BlockPosition::new(leaf, offset))
                    .unwrap()
                    .center()
            };
            (at(0), at(10))
        });
        cx.simulate_mouse_down(from, gpui::MouseButton::Left, Modifiers::none());
        draw(cx);
        cx.simulate_mouse_move(to, Some(gpui::MouseButton::Left), Modifiers::none());
        draw(cx);
        cx.simulate_mouse_up(to, gpui::MouseButton::Left, Modifiers::none());
        draw(cx);
        draw(cx);
        let seen = gpui_core::audit_take();
        let unnamed: Vec<_> = seen
            .iter()
            .filter(|a| a.label.as_ref().is_none_or(|l| l.trim().is_empty()))
            .collect();
        assert!(unnamed.is_empty(), "controls with no label: {unnamed:?}");
        let container: Vec<_> = seen
            .iter()
            .filter(|a| a.role == Role::GenericContainer)
            .collect();
        assert!(container.is_empty(), "controls with no role: {container:?}");
        let label = |l: &str| seen.iter().any(|a| a.label.as_deref() == Some(l));
        for expected in [
            "Page",
            "Move block",
            "Add a block below",
            "Plans",
            "Open Roadmap",
            "Format",
        ] {
            assert!(label(expected), "no control says {expected:?}: {seen:#?}");
        }
        assert!(
            seen.iter().any(|a| a.what == "gpui_blocks::bookmark"),
            "the bookmark card: {seen:#?}"
        );
    }
}
