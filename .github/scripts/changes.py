#!/usr/bin/env python3
"""What a pull request changed, for one crate's workflow: prints `own=` and `run=` lines for
$GITHUB_OUTPUT.

- own: the crate's own files (or its workflow) changed. The slow jobs (iOS, the shots lint)
  run only then.
- run: anything the crate builds from changed, so its macOS job runs. That's `own`, the
  toolchain, the folder of a workspace crate it depends on by path (directly or not, any kind of
  dependency: gpui-tables on gpui-dnd on gpui-core), the root Cargo.toml past its `members`
  line, or a package in Cargo.lock that the crate depends on (directly or not, on any target,
  dev-dependencies included). A pull request that only adds a crate to the workspace, or bumps a
  dependency another crate uses, then runs nothing here.

Usage: changes.py <own-regex> <package>... [--not <package>]... (with GH_TOKEN,
GITHUB_REPOSITORY, PR_NUMBER set). `--not` leaves a workspace crate out of the path walk, for one
whose own workflow already covers its dependents (gpui-core's lints and builds every crate).
Anything it can't work out counts as changed.
"""

import json
import os
import re
import posixpath
import subprocess
import sys
import tomllib


def gh(*args: str) -> str:
    return subprocess.run(["gh", "api", *args], check=True, capture_output=True, text=True).stdout


def raw(repo: str, path: str, sha: str) -> str:
    """A file at a commit, empty when it isn't there."""
    try:
        return gh("-H", "Accept: application/vnd.github.raw", f"repos/{repo}/contents/{path}?ref={sha}")
    except subprocess.CalledProcessError:
        return ""


def packages(lock: str) -> dict[tuple[str, str], str]:
    """Cargo.lock's packages by (name, version), each its whole block."""
    out = {}
    for block in lock.split("[[package]]")[1:]:
        name = re.search(r'^name = "([^"]+)"', block, re.M)
        version = re.search(r'^version = "([^"]+)"', block, re.M)
        if name and version:
            out[(name.group(1), version.group(1))] = block.strip()
    return out


def closure(pkgs: dict[tuple[str, str], str], roots: list[str]) -> set[tuple[str, str]]:
    """The roots and everything they depend on, by the lock's dependency lists."""
    by_name: dict[str, list[tuple[str, str]]] = {}
    for key in pkgs:
        by_name.setdefault(key[0], []).append(key)
    todo = [k for r in roots for k in by_name.get(r, [])]
    seen = set(todo)
    while todo:
        block = pkgs[todo.pop()]
        deps = re.search(r"^dependencies = \[(.*?)^\]", block, re.M | re.S)
        for dep in re.findall(r'"([^"]+)"', deps.group(1) if deps else ""):
            parts = dep.split(" ")
            # "name", or "name version" (and a source) when the lock holds several versions.
            found = [k for k in by_name.get(parts[0], []) if len(parts) == 1 or k[1] == parts[1]]
            for k in found:
                if k not in seen:
                    seen.add(k)
                    todo.append(k)
    return seen


def without_members(toml: str) -> str:
    return re.sub(r"^members\s*=\s*\[.*?\]", "", toml, count=1, flags=re.M | re.S)


def workspace(repo: str, sha: str) -> dict[str, tuple[str, set[str]]]:
    """The workspace's crates at a commit: by package name, its folder and the folders it
    depends on by path (each kind of dependency, on any target)."""
    root = tomllib.loads(raw(repo, "Cargo.toml", sha))
    out = {}
    for folder in root.get("workspace", {}).get("members", []):
        manifest = tomllib.loads(raw(repo, f"{folder}/Cargo.toml", sha))
        tables = [manifest] + list(manifest.get("target", {}).values())
        deps = set()
        for t in tables:
            for kind in ("dependencies", "dev-dependencies", "build-dependencies"):
                for d in (t.get(kind) or {}).values():
                    if isinstance(d, dict) and "path" in d:
                        deps.add(posixpath.normpath(posixpath.join(folder, d["path"])))
        out[manifest["package"]["name"]] = (folder, deps)
    return out


def dependency_folders(crates: dict[str, tuple[str, set[str]]], roots: list[str], skip: list[str]) -> set[str]:
    """The folders of the workspace crates `roots` depend on by path, walked through, without
    the roots' own or `skip`'s (nor what only they lead to)."""
    by_folder = {folder: name for name, (folder, _) in crates.items()}
    own = {crates[r][0] for r in roots if r in crates}
    skipped = {crates[s][0] for s in skip if s in crates}
    seen: set[str] = set()
    todo = [d for r in roots if r in crates for d in crates[r][1]]
    while todo:
        f = todo.pop()
        if f in seen or f in skipped or f in own:
            continue
        seen.add(f)
        todo.extend(crates[by_folder[f]][1] if f in by_folder else [])
    return seen


def main() -> None:
    args, skip = sys.argv[1:], []
    while "--not" in args:
        i = args.index("--not")
        skip.append(args[i + 1])
        del args[i : i + 2]
    own_re, roots = re.compile(args[0]), args[1:]
    repo, number = os.environ["GITHUB_REPOSITORY"], os.environ["PR_NUMBER"]
    files = gh("--paginate", f"repos/{repo}/pulls/{number}/files", "--jq", ".[].filename").split()
    own = any(own_re.search(f) for f in files)
    run, why = own, "its own files" if own else ""
    if not run and "rust-toolchain.toml" in files:
        run, why = True, "the toolchain"
    pr = json.loads(gh(f"repos/{repo}/pulls/{number}"))
    head = pr["head"]["sha"]
    if not run:
        try:
            deps = dependency_folders(workspace(repo, head), roots, skip)
        except (KeyError, tomllib.TOMLDecodeError):
            deps = None
        if deps is None:
            run, why = True, "the workspace: unreadable"
        else:
            hit = sorted({d for d in deps for f in files if f.startswith(d + "/")})
            if hit:
                run, why = True, "a crate it depends on: " + ", ".join(hit)
    if not run and ("Cargo.toml" in files or "Cargo.lock" in files):
        base = json.loads(gh(f"repos/{repo}/compare/{pr['base']['sha']}...{head}"))[
            "merge_base_commit"
        ]["sha"]
        if "Cargo.toml" in files:
            before, after = raw(repo, "Cargo.toml", base), raw(repo, "Cargo.toml", head)
            if not before or without_members(before) != without_members(after):
                run, why = True, "the root Cargo.toml past its members"
        if not run and "Cargo.lock" in files:
            before, after = packages(raw(repo, "Cargo.lock", base)), packages(raw(repo, "Cargo.lock", head))
            changed = {k for k in before.keys() | after.keys() if before.get(k) != after.get(k)}
            used = closure(before, roots) | closure(after, roots)
            hit = sorted(f"{n} {v}" for n, v in changed & used)
            if not before or not after or not used or hit:
                run, why = True, "Cargo.lock: " + (", ".join(hit[:10]) or "unreadable")
    print(f"own={str(own).lower()}")
    print(f"run={str(run).lower()}")
    print(f"changes: own={own} run={run} {why}".strip(), file=sys.stderr)


if __name__ == "__main__":
    main()
