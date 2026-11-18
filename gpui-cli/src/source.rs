//! Where gpui-core's files come from: GitHub at a revision (by default the one the host's
//! Cargo.lock has), or a local checkout.

use std::path::{Path, PathBuf};
use std::process::Command;

pub const DEFAULT_REPO: &str = "trevin-lee/gpui-essentials";

pub enum Source {
    /// `owner/repo` on GitHub, at a commit, tag or branch.
    GitHub { repo: String, rev: String },
    /// A gpui-essentials checkout (or its gpui-core directory).
    Local(PathBuf),
}

impl Source {
    /// How the copies' headers name it.
    pub fn describe(&self) -> String {
        match self {
            Source::GitHub { repo, rev } => format!("{repo}@{rev}"),
            Source::Local(path) => format!("{}", path.display()),
        }
    }

    /// The text of `file`, from gpui-core's directory (`components.toml`, `src/select.rs`).
    pub fn read(&self, file: &str) -> Result<String, String> {
        match self {
            Source::Local(root) => {
                let dir = if root.join("gpui-core/Cargo.toml").exists() {
                    root.join("gpui-core")
                } else {
                    root.clone()
                };
                std::fs::read_to_string(dir.join(file))
                    .map_err(|e| format!("reading {}: {e}", dir.join(file).display()))
            }
            Source::GitHub { repo, rev } => {
                let url =
                    format!("https://raw.githubusercontent.com/{repo}/{rev}/gpui-core/{file}");
                let out = Command::new("curl")
                    .args(["-fsSL", "--retry", "2", &url])
                    .output()
                    .map_err(|e| format!("running curl for {url}: {e}"))?;
                if !out.status.success() {
                    let why = String::from_utf8_lossy(&out.stderr);
                    return Err(format!(
                        "fetching {url}: {} (gpui add needs a revision of {repo} \
                         that has gpui-core/components.toml)",
                        why.trim()
                    ));
                }
                String::from_utf8(out.stdout).map_err(|_| format!("{url} isn't UTF-8"))
            }
        }
    }
}

/// The directory holding the nearest Cargo.toml with a `[package]`, from `from` up.
pub fn crate_root(from: &Path) -> Result<PathBuf, String> {
    let mut dir = Some(from);
    while let Some(d) = dir {
        if let Ok(text) = std::fs::read_to_string(d.join("Cargo.toml"))
            && text.lines().any(|l| l.trim() == "[package]")
        {
            return Ok(d.to_path_buf());
        }
        dir = d.parent();
    }
    Err(format!(
        "no Cargo.toml with a [package] in {} or above: run gpui add in your crate",
        from.display()
    ))
}

/// The gpui-core the host's Cargo.lock has: GitHub at its commit for a git dependency, or at
/// the tag `gpui-core-v<version>` for one from crates.io.
pub fn locked(crate_root: &Path) -> Result<Source, String> {
    let mut dir = Some(crate_root);
    let lock = loop {
        let Some(d) = dir else {
            return Err(
                "no Cargo.lock: run `cargo build` (or `cargo generate-lockfile`) after \
                        adding gpui-core, or say which with --rev"
                    .into(),
            );
        };
        if let Ok(text) = std::fs::read_to_string(d.join("Cargo.lock")) {
            break text;
        }
        dir = d.parent();
    };
    let mut found = None;
    for package in lock.split("[[package]]") {
        let field = |key: &str| {
            package.lines().find_map(|l| {
                let (k, v) = l.split_once('=')?;
                (k.trim() == key).then(|| v.trim().trim_matches('"').to_string())
            })
        };
        if field("name").as_deref() == Some("gpui-core") {
            found = Some((field("version").unwrap_or_default(), field("source")));
        }
    }
    let Some((version, source)) = found else {
        return Err(format!(
            "Cargo.lock has no gpui-core: add it first (`cargo add gpui-core --git \
             https://github.com/{DEFAULT_REPO}`), or say which with --rev"
        ));
    };
    match source {
        Some(s) if s.starts_with("git+") => {
            let (url, rev) = s
                .trim_start_matches("git+")
                .split_once('#')
                .ok_or_else(|| format!("Cargo.lock's gpui-core source has no commit: {s}"))?;
            let url = url.split('?').next().unwrap_or(url);
            let repo = url
                .strip_prefix("https://github.com/")
                .or_else(|| url.strip_prefix("ssh://git@github.com/"))
                .map(|r| r.trim_end_matches(".git").trim_end_matches('/').to_string())
                .ok_or_else(|| {
                    format!("gpui-core comes from {url}, not GitHub: use --path with a checkout")
                })?;
            Ok(Source::GitHub {
                repo,
                rev: rev.to_string(),
            })
        }
        Some(s) if s.starts_with("registry+") => Ok(Source::GitHub {
            repo: DEFAULT_REPO.into(),
            rev: format!("gpui-core-v{version}"),
        }),
        _ => Err("gpui-core is a path dependency: say where with --path".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_lock(lock: &str) -> Result<Source, String> {
        let dir = std::env::temp_dir().join(format!(
            "gpui-cli-lock-{}-{}",
            std::process::id(),
            crate::sha256::hex(lock.as_bytes())
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Cargo.lock"), lock).unwrap();
        let out = locked(&dir);
        std::fs::remove_dir_all(&dir).unwrap();
        out
    }

    #[test]
    fn a_git_dependency_is_fetched_at_its_commit() {
        let lock = "[[package]]\nname = \"gpui\"\nversion = \"0.3.12\"\n\n[[package]]\nname = \
                    \"gpui-core\"\nversion = \"0.1.0\"\nsource = \
                    \"git+https://github.com/someone/gpui-essentials?branch=main#abc123\"\n";
        let Ok(Source::GitHub { repo, rev }) = with_lock(lock) else {
            panic!()
        };
        assert_eq!(
            (repo.as_str(), rev.as_str()),
            ("someone/gpui-essentials", "abc123")
        );
    }

    #[test]
    fn a_crates_io_dependency_is_fetched_at_its_tag() {
        let lock = "[[package]]\nname = \"gpui-core\"\nversion = \"0.2.1\"\nsource = \
                    \"registry+https://github.com/rust-lang/crates.io-index\"\n";
        let Ok(Source::GitHub { rev, .. }) = with_lock(lock) else {
            panic!()
        };
        assert_eq!(rev, "gpui-core-v0.2.1");
    }

    #[test]
    fn a_path_dependency_needs_path() {
        let lock = "[[package]]\nname = \"gpui-core\"\nversion = \"0.1.0\"\n";
        assert!(with_lock(lock).is_err_and(|e| e.contains("--path")));
    }
}
