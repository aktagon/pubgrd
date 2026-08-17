# pubgrd

Copy a public repository tree from a private one, and verify nothing else got
in.

```console
$ pubgrd verify --public ../myproject-public
==> allow.unlisted: 12 configured, 41 examined, 2 found

FAIL: 2 files match no [allow] entry

  TODO.md
  internal-notes.md
      reason (allow): "everything a reader needs to build and run the tool"
```

## Why this exists

Plenty of projects keep two repositories: a private one where the work happens,
and a public one holding a subset of it. Something has to move files from the
first to the second. That something is usually a shell script, and it usually
grew one line at a time.

Such a script decides what stays private by listing what to leave out. A list of
exclusions fails in one direction only. Forget an entry and the script does not
stop. It copies the file and reports success. The file that leaks is the file
nobody thought to name.

In June 2026 a project of exactly this shape published its working notes and its
internal tool configuration to a public web server. The publish step matched
files with a wildcard where it needed a list of approved names. Nothing
malfunctioned. The wildcard matched two files nobody had considered, and the
deployment went out clean and green.

`pubgrd` reverses the list. You write down what may leave the private
repository, and anything you did not write down fails the check. Forgetting an
entry now withholds a file instead of publishing one. You can see that mistake
and fix it, rather than hearing about it from someone else.

## When you need it

You need this when all three are true:

- You maintain a private repository and a public one derived from it.
- Some script, command, or habit copies files between them.
- You cannot presently prove what that step emitted last time it ran.

The third condition matters most. A publish step you trust is a publish step
nobody has audited.

## When you do not

- **You have one repository.** Then `.gitignore` is your tool. `pubgrd` compares
  two trees and has nothing to say about one.
- **You want it to build the tree.** It will not run your transforms, render
  your templates, or generate your files. Those stay in your publish script.
- **You want it to commit and push.** It writes files into a directory and stops
  there. What you do with that directory is yours.
- **You author your public repository directly.** If nothing derives it, no
  second tree exists to check it against.
- **You want to know whether the published tree matches your build.** A
  directory entry is satisfied by any non-empty subset, so a tree missing half
  of `src/` still passes. The `coverage` block reports how many files each entry
  matched. Clone the published tree and `diff -rq` it against your build for the
  comparison itself.

## Install

```bash
cargo install --path .
```

Then follow **[Getting started](docs/guides/getting-started.md)**, which walks
you from an empty config to a publish step you can prove, in about fifteen
minutes. The rest of this page is reference.

## How it works

You write one file. It holds two blocks.

`[allow]` is the mechanism. It names every path that may be published, and the
paths it names are the allow-set. `[deny]` is a second, redundant assertion for
paths you want to name explicitly.

You can omit `[deny]` entirely. You cannot omit `[allow]`. A run with no
allow-set exits 2 instead of reporting a pass. Comparing a tree against nothing
and printing PASS turns a gate into decoration.

```toml
# pubgrd.toml
#
# DENY ALWAYS WINS. A path matching [deny] never ships, no matter how
# specifically [allow] names it. There is no re-admit, and order does not
# matter.

[allow]
paths  = ["src", "tests", "docs/guides", "Makefile", "Cargo.toml",
          "README.md", "LICENSE"]
reason = "everything a reader needs to build, run and audit the tool"

[deny]
paths  = ["**/NOTES.md", "**/TODO.md", "scripts/deploy-internal.sh"]
reason = "working state, and one script that talks to a private host"
```

`pubgrd init` writes a starter file that explains both blocks and proposes no
paths. It leaves the allow-set empty on purpose and exits 2 until you fill it
in. A scaffold that passes the moment you generate it teaches you that running a
command satisfies the gate.

**Deny wins absolutely, and nothing re-admits a denied path.** That rule looks
severe until you weigh the two ways to get it wrong. Denying too much withholds
a file, and you find out at once, because the run fails and names the entry.
Denying too little publishes something private, and you may never find out. The
rule resolves every conflict toward the mistake you can recover from. That is
what makes a broad `[deny]` pattern safe to write.

A project whose entire publish step copies four files needs four lines and no
`[deny]` at all. An exhaustive allow-set already covers every deny list anyone
could write:

```toml
[allow]
paths  = ["index.html", "LICENSE", "README.md", "dist/"]
reason = "the upload set"
```

Every violation prints the `reason` you wrote. A published path with no stated
reason is a path nobody can audit six months later. Every command therefore
requires `reason`, and refuses a blank one.

## Defaults you do not write

Two rules apply on every run whether you configure them or not.

**`.envrc` and the `.env` family never ship.** The family means `.env` itself
and any `.env.<suffix>`, such as `.env.local`, `.env.production` and
`.env.staging`. Matching compares whole path components and ignores case, so
`env`, `environment.md` and `src/env.ts` pass through untouched. Four suffixes
still publish, because they name a template rather than a secret:
`.env.example`, `.env.sample`, `.env.template` and `.env.dist`. Nothing
re-admits the rest. Rename the file, or remove it.

Reports mark which entries you did not write:

```
==> deny.present: 3 configured + 2 built-in, 41 examined, 1 found

FAIL: 1 file matches [deny]

  src/.env — built-in
```

**`pubgrd` skips `.git/`, and that is the only directory it skips.** Reports
call a checked file graded, and skipping happens before any rule runs. A skipped
path therefore reaches neither `[allow]` nor `[deny]`. It becomes invisible to
both.

Skipping is safe only for a directory that every public repository carries and
that no repository publishes as content. `.git` qualifies. `node_modules` does
not, although `pubgrd` skipped it once. A package inside it held a `.env` file
with a live cloud key. `pubgrd` stepped over the whole directory, and the run
then printed a pass claiming it had checked every file. `pubgrd` now grades
everything except `.git/`, and a `node_modules` sitting in a public tree fails
loudly.

Each run names what it skipped, which keeps a large filtered count readable:

```
==> 41302 entries walked, 41291 filtered
    pruned whole, not graded: .git/
```

## Commands

```
pubgrd init   --private .                          # write a starter config
pubgrd cp     --private . --public /tmp/out        # copy the allow-set
pubgrd cp     --private . --public /tmp/out --ref v1.0.0
                                                   # ...only what v1.0.0 tracks
pubgrd verify              --public /tmp/out       # check the tree that ships
pubgrd verify              --public /tmp/out --format json
                                                   # ...as one JSON object
```

**`cp` does not check its own output, and that is deliberate.** Anything that
reshapes the tree afterwards changes what ships: a `sed` pass, a generated
notice file, an injected build script. The tree `cp` produced is then not the
tree that ships, and checking it would check the wrong one. Run `verify` last,
over whatever actually ships:

```make
public:
	pubgrd cp --private . --public $(OUT) --ref $(VERSION)
	bash scripts/transform-public.sh $(OUT)
	pubgrd verify --public $(OUT)
```

[Getting started](docs/guides/getting-started.md) builds this target step by
step, including how to watch it fail on purpose before you trust it.

**`--ref` adds a guard without changing where the bytes come from.** A ref is a
git tag, branch or commit. Files still come from the filesystem. The flag
restricts the allow-set to paths that the ref tracks, then compares every path it
would copy against the ref. Any difference in your working tree refuses the copy
outright, whether you modified, staged, deleted or never tracked the file.

The guard covers the copy set rather than everything the ref tracks, so a dirty
file that `[deny]` excludes does not stop the release.

The comparison targets the ref you named rather than your latest commit. Tagging
a release and continuing to work therefore does not quietly publish the later
commits. Omit the flag when your source is build output that lives in no commit.

`verify` needs no git repository and no private tree. It takes a directory and a
config. That matters, because the incident described above happened in a staging
directory under `/tmp`. The tree that ships is the tree you have to check.

**`--format json` writes one object to stdout and nothing else.** Assert against
it rather than against the report: the English pluralises, so `1 file matches`
and `2 files match` break a test that anchors on the sentence. The object
carries the rule counters, the per-entry counts, and the paths behind each
violation. Exit codes do not change.

```console
$ pubgrd verify --public /tmp/out --format json
{
  "version": 1,
  "exit_code": 0,
  "config": "/path/to/pubgrd.toml",
  "examined": 33,
  "rules": [
    {"name": "allow.unlisted", "configured": 14, "built_in": 0, "examined": 33, "outcome": "ran", "found": 0},
    ...
  ],
  "coverage": [{"entry": "src", "matched": 11}, {"entry": "tests", "matched": 10}],
  "unlisted": [], "denied": [], "unmatched": [], "collisions": [], "warnings": []
}
```

`version` is the schema version. It exists so a consumer can refuse an object it
does not understand instead of misreading one.

## Read the counts, not only the verdict

A passing run also reports how many files each `[allow]` entry matched:

```console
$ pubgrd verify --public /tmp/out
==> allow.unlisted: 14 configured, 33 examined, 0 found
==> allow.missing: 14 configured, 33 examined, 0 found

    coverage (files matched per [allow] entry; entries may overlap, so these need not sum to 33)

      CHANGELOG.md        1
      LICENSE             1
      README.md           1
      docs/guides         1
      tests               10
      src                 11

PASS: 33 files, all of them named by [allow]
```

Nothing fails on those numbers. They are there because `allow.missing: 0 found`
means every entry matched **at least one** file, which is weaker than it reads. A
`src` entry covering 11 of the 13 files your build produces passes, so compare
each count against the build you just ran.

Entries may overlap, so the column need not sum to `examined`. `pubgrd` cannot
make the comparison for you — the expected count lives in your private tree, and
`verify` reads one tree.

## Where the config comes from

`pubgrd.toml`, looked for in this order, with **no upward search**:

1. `--private`, when the command takes it
2. `--public`
3. the working directory

Pass `--config <PATH>` to name one anywhere else. `verify` takes no `--private`,
so a config written by `pubgrd init --private ../myproject` is not on `verify`'s
search path unless you are standing in that directory. Name it with `--config`,
or run `verify` from the private tree.

## Exit codes

| exit | meaning |
| --- | --- |
| 0 | the tree matches what the config says it should be |
| 1 | a violation — see the list below |
| 2 | configuration or usage error, and a successful `pubgrd init` |

Exit 1 covers four shapes, and the first two are easy to confuse:

- a **file** matches no `[allow]` entry
- an `[allow]` **entry** matches no file in the tree, or matched something
  before `[deny]` and nothing after
- a file matches `[deny]`
- a rule with entries to apply examined nothing. Built-in entries count toward
  that, so `deny.present` can be vacuous with no `[deny]` block at all

Exit 2 covers a missing or unreadable `pubgrd.toml`, TOML that will not parse, a
path entry that will not compile, no `[allow]` block or an empty `[allow] paths`,
a blank `reason`, a `--public` or `--private` that is not a directory, a symlink
in the allow-set, a copy that selected no file at all, a missing `git` or an
unresolvable `--ref`, a `--ref` whose worktree has moved, and `init` refusing to
overwrite.

It is also what a **successful** `init` returns. That is deliberate: a scaffold
that goes green on generation teaches you that running a command satisfies the
check. Run `pubgrd --help` for the full list.

**No exit code means "checked nothing".** A rule that examined zero files fails
instead of passing, and so does an empty allow-set. Any scheme that treats an
empty comparison as a clean one reports success for both. An empty comparison is
exactly what a broken publish step produces.

## Both directions fail loudly

Denying too much fails as visibly as denying too little. That symmetry lets you
write a broad pattern without worrying about it:

```console
$ pubgrd cp --private . --public /tmp/out
FAIL: 1 allow entry matched something before [deny] and nothing after

  README.md — on [allow] paths, excluded by [deny] `**/*.md`
              reason (deny): "working state and house conventions"
```

```console
$ pubgrd verify --public ../myproject-public
FAIL: 2 files match no [allow] entry

  TODO.md
  build-config.json
      reason (allow): "the upload set"
```

The first run withheld a file you meant to publish. The second found files in a
public tree that nobody approved. Both stop the pipeline, and both name the
entry and the reason behind it.

## What it will not do

- **Build the tree.** Transforms, generated files and templating stay in your
  publish script. A survey of four repository pairs found that three needed
  structurally different builders. A configuration format that expresses all
  three becomes a programming language with worse ergonomics.
- **Touch git in the destination.** Commit, tag and push semantics differ
  irreconcilably between projects.
- **Rename files while copying.** An allow-set cannot express a move. Have your
  build write to the final path.
- **Lint your documents.** [`ctxgrd`](https://github.com/aktagon/ctxgrd) checks
  link resolution and frontmatter shape. Point it at the built tree.

## Status

Working. `init`, `cp` and `verify` all run, and the test suite covers them.
`make check` is the gate. Read its targets from the `Makefile` rather than
trusting a count written here. It refuses a suite of zero tests as loudly as a
failing one.

`pubgrd` builds its own public tree. The repository you are reading is the
output of `pubgrd cp --ref`, and it passes its own `make check` from a clean
checkout.

## Guides

- [Getting started](docs/guides/getting-started.md) — install, write your first
  allow-set, copy, verify, and wire it into a publish target you have watched
  fail.

## License

[Elastic-2.0](LICENSE)
