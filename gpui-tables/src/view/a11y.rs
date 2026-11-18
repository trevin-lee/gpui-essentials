//! What a control tells assistive technology: a role and a label, reported to gpui-core's
//! accessibility audit as it's drawn, so a test can fail on any control without a label (GPUI's
//! test windows build no accessibility tree). gpui-core's `Press` reports itself; the views'
//! own elements use [`Control::control`].

use gpui::{Role, SharedString, StatefulInteractiveElement};

/// An interactive element as a control: its role and its label, set and reported together.
pub(crate) trait Control: StatefulInteractiveElement {
    /// Gives the element `role` and `label`, and reports them as `what` (the drawing module,
    /// `module_path!()`) to gpui-core's audit while one runs.
    fn control(self, what: &'static str, role: Role, label: impl Into<SharedString>) -> Self {
        let label = label.into();
        gpui_core::audit(what, role, Some(&label));
        self.role(role).aria_label(label)
    }
}

impl<T: StatefulInteractiveElement> Control for T {}

#[cfg(test)]
mod tests {
    use std::path::Path;

    /// Every element that takes a click in the views is a control with a role and a label: its
    /// builder calls `.control(..)` (which sets both and reports them to the audit) or is a
    /// gpui-core `Press` given an `.accessibility_label(..)`, is made by a helper that does one
    /// of those, or says why it needs neither in an `// a11y:` comment. A new click handler
    /// without one fails here.
    #[test]
    fn every_click_handler_is_a_labelled_control() {
        let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/view");
        let mut files: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| p.extension().is_some_and(|e| e == "rs"))
            .collect();
        files.sort();
        let mut unlabelled = Vec::new();
        for file in files {
            let text = std::fs::read_to_string(&file).unwrap();
            for (line, head) in unlabelled_handlers(&text) {
                unlabelled.push(format!("{}:{line}: {head}", file.display()));
            }
        }
        assert!(
            unlabelled.is_empty(),
            "click handlers on elements with no role and label:\n{}",
            unlabelled.join("\n")
        );
    }

    const LABELLED: [&str; 3] = [".control(", "accessibility_label(", "// a11y:"];

    /// The click handlers in `text` (before its tests) whose element isn't a labelled control:
    /// their line and the line their builder starts at.
    fn unlabelled_handlers(text: &str) -> Vec<(usize, String)> {
        let lines: Vec<&str> = text.lines().collect();
        let end = lines
            .iter()
            .position(|l| l.starts_with("#[cfg(test)]"))
            .unwrap_or(lines.len());
        let indent = |l: &str| l.len() - l.trim_start_matches(' ').len();
        // Functions and closures that make labelled controls: `let icon = |..| Press::new(..)..`
        // or `fn button(..) -> Press { .. }`.
        let mut helpers: Vec<&str> = Vec::new();
        for (at, _) in text.match_indices("let ").chain(text.match_indices("fn ")) {
            let skip = if text[at..].starts_with("let ") { 4 } else { 3 };
            let rest = &text[at + skip..];
            let Some(name_end) = rest.find(|c: char| !c.is_alphanumeric() && c != '_') else {
                continue;
            };
            let after = rest[name_end..].trim_start();
            let body = if skip == 4 {
                match after
                    .strip_prefix("= |")
                    .or_else(|| after.strip_prefix("= move |"))
                {
                    Some(body) => body,
                    None => continue,
                }
            } else if after.starts_with('(') || after.starts_with('<') {
                after
            } else {
                continue;
            };
            let mut cut = body.len().min(1500);
            while !body.is_char_boundary(cut) {
                cut -= 1;
            }
            let body = &body[..cut];
            // A function's body: up to its next sibling.
            let body = match (skip == 3).then(|| body.find("\n    fn ").or(body.find("\nfn "))) {
                Some(Some(next)) => &body[..next],
                _ => body,
            };
            if LABELLED.iter().any(|m| body.contains(m)) {
                helpers.push(&rest[..name_end]);
            }
        }
        let mut out = Vec::new();
        for i in 0..end {
            let line = lines[i].trim_start();
            if !(line.starts_with(".on_click(") || line.starts_with(".on_mouse_down(")) {
                continue;
            }
            // The builder's first line: the nearest line above that's less indented, past
            // `.map(|this| ..)`, `.when(.., |this| ..)` and `match` arms over `this`.
            let mut level = indent(lines[i]);
            let mut start = i;
            while let Some(j) = (0..start)
                .rev()
                .find(|&j| !lines[j].trim().is_empty() && indent(lines[j]) < level)
            {
                start = j;
                let head = lines[start].trim();
                if !(head.ends_with("this") || head.contains("|this|") || head.contains("|b|")) {
                    break;
                }
                level = indent(lines[start]);
            }
            let head = lines[start].trim();
            let chunk = lines[start..=i].join("\n");
            let labelled = LABELLED.iter().any(|m| chunk.contains(m))
                || helpers.iter().any(|h| {
                    let call = format!("{h}(");
                    head.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '(')
                        .any(|w| w.starts_with(&call))
                });
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
        let helper = "fn button(id: &str) -> Press {\n    Press::new(id).accessibility_label(\"X\")\n}\n\nfn g() {\n    button(\"a\")\n        .on_click(|_, _, _| {})\n}\n";
        assert!(unlabelled_handlers(helper).is_empty());
    }
}
