#!/usr/bin/env python3
# Copyright (c) 2026-present, the Zendia developers.
#
# SPDX-License-Identifier: BSD-2-Clause

"""Builds, runs and checks Zendia.

Zendia runs upstream Ladybird's Hub and engine processes, so `build` and `run` build upstream first with
Meta/ladybird.py, then build the Zendia/ Cargo workspace. This wrapper exists so that no upstream build file has to
know about Zendia.
"""

import argparse
import subprocess
import sys

from pathlib import Path

SOURCE_ROOT = Path(__file__).resolve().parents[2]
ZENDIA_ROOT = SOURCE_ROOT / "Zendia"


def run(command: list[str], cwd: Path) -> None:
    print("+", " ".join(command), flush=True)
    subprocess.run(command, cwd=cwd, check=True)


def build_upstream() -> None:
    run([sys.executable, str(SOURCE_ROOT / "Meta" / "ladybird.py"), "build"], SOURCE_ROOT)


def cargo_profile(args: argparse.Namespace) -> list[str]:
    return [] if args.debug else ["--release"]


def command_build(args: argparse.Namespace) -> None:
    if not args.skip_upstream:
        build_upstream()
    run(["cargo", "build", "--workspace", *cargo_profile(args)], ZENDIA_ROOT)


def command_run(args: argparse.Namespace) -> None:
    if not args.skip_upstream:
        build_upstream()
    arguments = args.arguments[1:] if args.arguments[:1] == ["--"] else args.arguments
    run(["cargo", "run", "-p", "zendia", *cargo_profile(args), "--", *arguments], ZENDIA_ROOT)


def command_check(args: argparse.Namespace) -> None:
    run(["cargo", "fmt", "--all", "--check"], ZENDIA_ROOT)
    run(["cargo", "clippy", "--workspace", "--all-targets"], ZENDIA_ROOT)
    run(["cargo", "test", "--workspace"], ZENDIA_ROOT)
    run([str(ZENDIA_ROOT / "Meta" / "fork-surface.sh")], SOURCE_ROOT)
    run([sys.executable, str(ZENDIA_ROOT / "Meta" / "check-licenses.py")], SOURCE_ROOT)


def main() -> int:
    parser = argparse.ArgumentParser(description="Builds, runs and checks Zendia")
    commands = parser.add_subparsers(dest="command", required=True)

    build = commands.add_parser("build", help="Build upstream Ladybird, then the Zendia workspace")
    build.add_argument("--skip-upstream", action="store_true", help="Build only the Zendia workspace")
    build.add_argument("--debug", action="store_true", help="Build Zendia's crates without optimizations")
    build.set_defaults(handler=command_build)

    run_parser = commands.add_parser("run", help="Build, then start Zendia. Arguments after -- go to the browser")
    run_parser.add_argument("--skip-upstream", action="store_true", help="Do not build upstream Ladybird first")
    run_parser.add_argument("--debug", action="store_true", help="Build Zendia's crates without optimizations")
    run_parser.add_argument("arguments", nargs=argparse.REMAINDER, help="Arguments for the browser, such as URLs")
    run_parser.set_defaults(handler=command_run)

    check = commands.add_parser("check", help="Format check, clippy, tests, the fork check and the license check")
    check.set_defaults(handler=command_check)

    args = parser.parse_args()
    try:
        args.handler(args)
    except subprocess.CalledProcessError as error:
        return error.returncode
    return 0


if __name__ == "__main__":
    sys.exit(main())
