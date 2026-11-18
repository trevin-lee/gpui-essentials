#!/usr/bin/env python3
"""What a pull request changed, for one crate's workflow: prints `own=` and `run=` lines for
$GITHUB_OUTPUT.

- own: the crate's own files (or its workflow) changed. The slow jobs (iOS, the shots lint)
  run only then.
- run: anything the crate builds from changed, so its macOS job runs. That's `own`, the
  toolchain, the root Cargo.toml past its `members` line, or a package in Cargo.lock that the
  crate depends on (directly or not, on any target, dev-dependencies included). A pull request
  that only adds a crate to the workspace, or bumps a dependency another crate uses, then runs
  nothing here.

Usage: changes.py <own-regex> <package>... (with GH_TOKEN, GITHUB_REPOSITORY, PR_NUMBER set).
Anything it can't work out counts as changed.
"""

import json
import os
import re
import subprocess
import sys


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


def main() -> None:
    own_re, roots = re.compile(sys.argv[1]), sys.argv[2:]
    repo, number = os.environ["GITHUB_REPOSITORY"], os.environ["PR_NUMBER"]
    files = gh("--paginate", f"repos/{repo}/pulls/{number}/files", "--jq", ".[].filename").split()
    own = any(own_re.search(f) for f in files)
    run, why = own, "its own files" if own else ""
    if not run and "rust-toolchain.toml" in files:
        run, why = True, "the toolchain"
    if not run and ("Cargo.toml" in files or "Cargo.lock" in files):
        pr = json.loads(gh(f"repos/{repo}/pulls/{number}"))
        head = pr["head"]["sha"]
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
