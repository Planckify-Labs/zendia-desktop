# Contributing to Zendia

## License of contributions

Zendia is under the BSD 2-Clause license (`Zendia/LICENSE`). By sending a contribution, you agree that it is under
that license too. There is no contributor license agreement to sign.

Do not add `Signed-off-by` lines to commit messages. Upstream's `Meta/lint-commit.sh` rejects them, and changes we
offer to Ladybird have to pass it.

## Where code goes

- Zendia's own code goes in `Zendia/`. Upstream never edits that directory, so it never conflicts in a merge.
- Change an upstream file only when the change cannot live in `Zendia/`, which in practice means one of the seams that
  `Zendia/Documentation/Architecture.md` describes. Keep the change small and generic, and list the file in
  `Zendia/FORK.md` with the reason. CI fails when a changed upstream file is missing from that list.
- Do not add copyright lines to upstream files you change. Headers that differ from upstream conflict on every merge,
  and `Zendia/FORK.md` already records the change.
- New files in `Zendia/` start with the `the Zendia developers` copyright header and the `BSD-2-Clause` SPDX line, as
  the existing ones do.
- New Rust dependencies must be under a license `Zendia/Meta/check-licenses.py` allows. CI runs it.

## Commit messages

Commits follow Ladybird's style, which `Meta/lint-commit.sh` checks: a `Category: Title` line of at most 72
characters, starting with a capital letter after the category and without a period at the end, then an empty line and
a body that explains why.

- For Zendia's own code, the category is `Zendia` or the area inside it, for example `Zendia: Add the license check`
  or `Zendia/BrowserCore: Add the command registry`.
- For a change to upstream files, the title starts with `zendia:` and then upstream's category, for example
  `zendia: LibWeb: Add a hook for isolated worlds`. That keeps the fork's changes easy to find with
  `git log --grep '^zendia:'`.

Run the linter on a message with `Meta/lint-commit.sh <file>`.

## Before you send a change

```sh
Zendia/Meta/zendia.py check
```

It runs rustfmt, clippy and the tests of the `Zendia/` workspace, checks that every changed upstream file is listed in
`Zendia/FORK.md`, and checks the licenses of the linked crates.
