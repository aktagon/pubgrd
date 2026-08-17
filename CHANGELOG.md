# Changelog

## 0.2.0 — 2026-08-17

Two interfaces gained, four false statements removed. Everything here came from
one piece of adopter feedback and one evaluation of `--help`.

### Added

- `pubgrd verify --format json` — one JSON object on stdout and nothing else, so
  a caller can assert on the outcome without parsing English. Five of an
  adopting project's assertions broke at once on `match` against `matches` in
  the verdict sentences. The object carries the rule counters, the per-entry
  counts, the paths behind each violation, and the fallback warnings; `text`
  stays the default and is unchanged. Exit codes do not move. Written without
  taking `serde_json`, which would be a sixth dependency; the escaping is one
  tested function. Requested as FEEDBACK-001 and recorded in ADR-010, which also
  fixes why it waited for the counts below.

- A `coverage` block under `verify`'s rule lines, printing every `[allow]` entry
  and how many files it matched, fewest first and capped at 20. It prints on
  passing runs too, which no other detail block does. `allow.missing: 0 found`
  means every entry matched at least one file, and a directory entry is
  satisfied by any non-empty subset of itself — so a tree missing two of
  thirteen scripts passed in silence. Counts are taken after `[deny]`, and
  overlapping entries each count the file they share, so the column does not sum
  to the examined total and the header says so. Requested as FEEDBACK-001 after
  a four-session-stale public tree passed a real adoption.

### Fixed

- **Four false statements in `--help`, and one of them was also in the README.**
  The exit-1 causes were all phrased about a *file*, omitting an `[allow]`
  *entry* that matches no file — the rule the coverage counts above are about.
  "a rule with a non-empty configured set examined nothing" was contradicted by
  the tool's own `deny.present: 0 configured + 2 built-in ... VIOLATION` line,
  because vacuity counts built-in entries too. The exit-2 list named five of
  thirteen causes, omitting a missing `pubgrd.toml` and the successful `init`
  that returns the same code. And `cp --ref` claimed to refuse on any dirty path
  at the ref, where it covers only paths it would copy. ADR-003 was amended for
  the first three, which `--help` had copied from it; ADR-004 was already correct
  and `--help` now uses its words. Recorded in ADR-011 with tests, because three
  humans read the text without catching any of it.

### Changed

- **`-h` is now a summary and `--help` is the contract.** They printed identical
  content, so clap's own "see more with `--help`" hint pointed at nothing. `-h`
  is 1199 characters against `--help`'s 3301.
- Help names `pubgrd.toml` and its search order, and points at `pubgrd init` for
  the documented starter. The filename appeared nowhere in any help surface
  while `[allow]` appeared eight times.
- The `Commands:` entries are one clause each. They ran 202, 171 and 152
  characters, which wrapped mid-word at 80 columns.
- Help carries an `Examples:` section, as `wrkgrd` and `trtlgrd` do.
- Option layout is consistent across all three subcommands.
- `verify` no longer computes the unmatched-entry rule separately from the new
  counts. `allow.missing` is now the subset of the coverage counts equal to
  zero, so the rule and the column cannot disagree.
- Every rule's outcome is decided once, before either renderer runs. The text
  report and the JSON object read the same values, so they cannot disagree about
  an outcome or about the exit code.

## 0.1.0 — 2026-08-16

First release. Three commands, three flags, two configuration blocks, one
precedence rule.

### Added

- `pubgrd verify` — grade a tree against `[allow]` and `[deny]`. Four rules,
  each reporting the count it examined: `allow.unlisted`, `allow.missing`,
  `deny.present`, `deny.collision`. A rule with entries to apply that examined
  nothing is a violation, so an empty tree fails rather than passing. (Corrected
  2026-08-17: this said "a non-empty configured set", which is wrong in the same
  way `--help` was — vacuity counts built-in entries too.)
  A detail block lists at most 20 paths and then says how many more there are;
  the count in the heading above it is always the whole of it.
- `pubgrd cp` — copy the allow-set from a private tree into a public one. Mode
  bits preserved, mtimes not, symbolic links refused, empty directories not
  created, existing files never deleted. It does not verify its own output.
- `pubgrd cp --ref <REF>` — restrict the allow-set to paths tracked at a ref and
  refuse, before writing anything, if any of them differs from the ref in the
  worktree. "Differs" covers modified, staged, deleted and **untracked**, and
  rename detection is off, so both halves of a rename are reported.
- `pubgrd init` — write a documented configuration template and exit 2. It reads
  no tree and proposes no path: `paths = []` and `reason = "TODO"` are both
  refused at load, so the scaffold cannot go green on generation. The one
  production run of the earlier tree-seeding version proposed allowing `.envrc`
  along with an entire build directory, which is why it does not do that any
  more.

### Always applied, without being configured

- **Denied on every run, not overridable by `[allow]`:** `.envrc`, and the
  `.env` family — `.env` itself plus any `.env.<suffix>`. Matched by path
  component, case-insensitively.
  Still publishable: `.env.example`, `.env.sample`, `.env.template`,
  `.env.dist`.
- **Pruned, never graded either way:** `.git/`, and nothing else. A prune runs
  before grading, so anything pruned is invisible to both sets; that is sound
  only for a directory every public repository is expected to hold.

**This is a compatibility surface.** A project publishing any member of the
`.env` family outside the four template suffixes will newly fail, with no
override — that is the whole point of the rule, and there is no flag to turn it
off. Check a tree with `pubgrd verify` before upgrading.

### Verified against real repositories

- Reproduces the 2026-06-07 CDN leak as a failure, against a fixture holding the
  six files that repository publishes plus the two that leaked.
- `cp --ref HEAD` over the primary consumer produces a tree byte-identical to
  the `git archive` extraction its publish script performs, mode bits included.

### Known limits

- No rename during copy. An allow-list cannot express a move; have the build
  write to the final path.
- A `[deny]` entry is literal unless it carries something `globset` treats as
  syntax, so a filename to be denied anywhere must be written `**/NAME`. An
  entry that holds such a character but is not a valid glob — `notes}v2.md` —
  is read as a literal, with a warning.
- There is no `deny.missing` rule. A deny entry matching nothing is usually
  correct, since a defensive pattern written against a file that does not exist
  yet is the intended posture. The cost is that a misspelled deny entry stays
  undetected when the file it meant to catch is also on the allow-set.
