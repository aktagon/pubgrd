# Contributing to pubgrd

`pubgrd` copies a public repository tree from a private one and verifies that
nothing else got in. You write an allow-set naming every path that may be
published. Anything you did not name fails the check.

Thanks for wanting to help. This page tells you how to build the tool, how to
test it the way this project tests, and how to propose a change that lands.

Source: <https://github.com/aktagon/pubgrd>

## Before you start

Install a Rust toolchain that supports edition 2024. Then clone the repository
and build it:

```bash
git clone https://github.com/aktagon/pubgrd
cd pubgrd
cargo build
cargo test
```

Run the tool against a scratch directory to see what it does:

```bash
cargo run -- init --private .
cargo run -- verify --public /tmp/some-tree
```

## The gate

`make check` decides whether your change is done. It runs these targets, cheapest
first:

| target | what it runs |
| --- | --- |
| `fmt` | `cargo fmt --check` |
| `clippy` | `cargo clippy --all-targets -- -D warnings` |
| `test` | `cargo test --no-fail-fast`, plus the guard described below |

Every target uses the Rust toolchain and nothing else, so `make check` runs on a
clean clone with no extra installation.

The `test` target does more than call `cargo test`. It reads the pass counts out
of the output and fails when the total is zero. A suite of zero tests prints
`test result: ok. 0 passed` and exits 0, which is a green gate over an empty set.
That is the exact failure `pubgrd` exists to catch one level up, so the gate
refuses it here too.

## Testing discipline

This is the part that differs from most projects, and it is the part that
matters most.

**A test that has never failed proves nothing.** Write your test first, run it
against the unfixed code, and watch it go red. Only then write the fix and watch
it go green. A test you have only ever seen pass may be asserting nothing at
all.

The reason is mechanical. An assertion over an observed set passes when the set
is empty, and empty is exactly what a regression produces. So assert that the
observation happened, then check the count, in that order.

**A check that cannot fail is not a check.** Every rule in `pubgrd` reports the
count it examined, and a rule that examined nothing fails rather than passing.
Hold your tests to the same standard.

**Never state a verification more broadly than what it covered.** If you watched
one test go red and green, say that. It tells you nothing about the test beside
it. Write in your pull request what you actually watched happen, naming the test
and the outcome.

## Commit messages

Write a short subject line in the imperative mood, under about 70 characters:
`Refuse a symlink in the allow-set`, not `Fixed symlinks` or `symlink stuff`.

Use the body to explain why. The diff already shows what changed. What it cannot
show is the case you hit, the option you rejected, and the reason the obvious
approach does not work. Wrap the body at 72 characters.

No emojis, in commit messages or anywhere else.

## Proposing a behaviour change

Open an issue before you write the code.

Several rules here look severe and are deliberate. Deny always wins over allow.
Nothing re-admits a denied path. Order and specificity do not matter. An empty
allow-set exits 2 instead of reporting a pass. Each of those resolves a conflict
toward the mistake you can recover from, because withholding a file is visible
and publishing a private one is not.

A pull request that changes one of these without a prior discussion will be
declined, however good the code is. An issue costs you ten minutes and may save
you an afternoon.

Small fixes need no issue. Send the pull request.

## Reporting a bug

For a bug in `pubgrd` itself, include four things:

1. Your `pubgrd.toml`. Redact paths if you must, but keep the structure and the
   glob syntax intact.
2. The exact command you ran, with all flags.
3. The full output, including the `==>` rule count lines. Those lines say how
   many entries each rule had configured, how many files it examined, and how
   many it found. A wrong count usually locates the bug on its own.
4. The exit code. Run `echo $?` right after the command.

Say what you expected instead. "It failed" and "it passed" are both bugs here,
and the interesting question is which files moved which way.

## Security

Do not open a public issue for a security problem, especially one where
`pubgrd` passed a tree it should have failed. Follow the process in
[SECURITY.md](SECURITY.md) instead.

## Licence

`pubgrd` ships under [Elastic-2.0](LICENSE). By contributing, you agree that
your contribution ships under the same licence.
