#!/usr/bin/env python3
# Copyright (c) 2026-present, the Zendia developers.
#
# SPDX-License-Identifier: BSD-2-Clause

"""Checks that every crate linked into Zendia's binaries is under a license we allow.

Zendia is BSD-2-Clause, so what it links must be under a permissive license. A crate passes if one of the alternatives
of its license expression (the parts joined by OR) uses only allowed licenses. Only crates that end up in a binary are
checked: build dependencies such as code generators and dev-dependencies are not shipped. Upstream's own crates carry no
license field, since the whole tree is under the BSD-2-Clause license at its root.

Usage: Zendia/Meta/check-licenses.py [--verbose]
"""

import argparse
import json
import re
import subprocess
import sys

from pathlib import Path

SOURCE_ROOT = Path(__file__).resolve().parents[2]
ZENDIA_ROOT = SOURCE_ROOT / "Zendia"

ALLOWED = {
    "0BSD",
    "Apache-2.0",
    "Apache-2.0 WITH LLVM-exception",
    "BSD-2-Clause",
    "BSD-3-Clause",
    "BSL-1.0",
    "CC0-1.0",
    "ISC",
    "MIT",
    "NCSA",
    "Unicode-3.0",
    "Unlicense",
    "Zlib",
}


def alternatives(expression: str) -> list[list[str]]:
    """Splits an SPDX expression into its OR alternatives, each a list of licenses that all apply."""
    # Old crates write "MIT/Apache-2.0" for "MIT OR Apache-2.0".
    expression = expression.replace("/", " OR ")
    # Each crate in this tree nests at most one level of parentheses, as in "(MIT OR Apache-2.0) AND NCSA". Expand
    # "(A OR B) AND C" into "A AND C OR B AND C".
    match = re.fullmatch(r"\((.+?)\)\s+AND\s+(.+)", expression.strip())
    if match:
        inner, rest = match.groups()
        expression = " OR ".join(f"{choice.strip()} AND {rest}" for choice in inner.split(" OR "))
    expression = expression.replace("(", "").replace(")", "")
    return [[part.strip() for part in choice.split(" AND ")] for choice in expression.split(" OR ")]


def is_allowed(expression: str) -> bool:
    return any(all(license in ALLOWED for license in choice) for choice in alternatives(expression))


def linked_packages(metadata: dict) -> list[dict]:
    """The packages reachable from the workspace members through normal dependencies only."""
    packages = {package["id"]: package for package in metadata["packages"]}
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    seen = set(metadata["workspace_members"])
    queue = list(seen)
    while queue:
        for dependency in nodes[queue.pop()]["deps"]:
            if any(kind["kind"] is None for kind in dependency["dep_kinds"]) and dependency["pkg"] not in seen:
                seen.add(dependency["pkg"])
                queue.append(dependency["pkg"])
    return [packages[package_id] for package_id in seen]


def main() -> int:
    parser = argparse.ArgumentParser(description="Checks the licenses of the crates linked into Zendia's binaries")
    parser.add_argument("--verbose", action="store_true", help="List every linked crate and its license")
    args = parser.parse_args()

    metadata = json.loads(
        subprocess.run(
            ["cargo", "metadata", "--format-version", "1", "--locked"],
            cwd=ZENDIA_ROOT,
            check=True,
            capture_output=True,
            text=True,
        ).stdout
    )

    problems = []
    linked = sorted(linked_packages(metadata), key=lambda package: package["name"])
    for package in linked:
        license = package.get("license")
        manifest = Path(package["manifest_path"]).resolve()
        if not license:
            if package["source"] is None and SOURCE_ROOT in manifest.parents:
                license = "BSD-2-Clause (the tree's LICENSE)"
            else:
                problems.append(f"{package['name']} {package['version']}: no license field")
                continue
        elif not is_allowed(license):
            problems.append(f"{package['name']} {package['version']}: {license}")
            continue
        if args.verbose:
            print(f"{package['name']} {package['version']}: {license}")

    print(f"Checked the licenses of {len(linked)} linked crates")
    if problems:
        print("Not under a license we allow (see ALLOWED in this script):")
        for problem in problems:
            print(f"  {problem}")
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
