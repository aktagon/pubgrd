---
title: Getting started
diataxis:
  type: tutorial
---

# Getting started

Take a private repository and a public one derived from it, and end with a
publish step you can prove. This takes about fifteen minutes.

You need two directories and a terminal. You do not need a git repository until
the last step, and you do not need one at all if your public tree comes from
build output.

## 1. Install

```bash
cargo install --path .
pubgrd --help
```

## 2. Write a starter config

Run `init` inside your private repository:

```bash
cd ~/code/myproject
pubgrd init --private .
```

This writes `pubgrd.toml` and exits 2. The non-zero exit is the point. The file
it wrote names no paths and carries a placeholder reason, so every command
refuses it until you edit it. A starter config that passes on creation teaches
you that running a command satisfies the gate.

Open the file. It explains both blocks and proposes nothing:

```toml
[allow]
paths  = []
reason = "TODO"
```

## 3. Discover what your public tree already holds

If you already publish somewhere, let `pubgrd` list that tree for you rather
than reconstructing it from memory.

Put one path you know belongs in `paths`, and write a real reason:

```toml
[allow]
paths  = ["README.md"]
reason = "the public API surface and the docs a reader needs"
```

Now check your existing public tree:

```bash
pubgrd verify --public ../myproject-public --config pubgrd.toml
```

Every file except `README.md` comes back as unlisted. That list is your
inventory:

```
FAIL: 23 files match no [allow] entry

  Cargo.toml
  LICENSE
  src/lib.rs
  src/main.rs
  ...
```

Read it. Move the paths that belong into `paths`, one at a time, by hand.

Transcribe them rather than pasting the whole list. A path you typed is a path
you judged. A block you pasted is a block you accepted, and accepting your
current public tree turns any file that leaked into policy.

Prefer directories to individual files where a whole directory belongs. `src`
covers `src` and everything under it, so you write one entry instead of forty.

## 4. Copy

```bash
pubgrd cp --private . --public /tmp/out
```

`cp` reports what it selected and what it wrote:

```
==> 11 allow entries → 35 candidates
==> copied 35 files

==> NOTHING VERIFIED. Run `pubgrd verify --public /tmp/out`
    after anything that post-processes this tree.
```

`cp` never checks its own output. If your publish step reshapes the tree
afterwards, then the tree `cp` produced is not the tree that ships.

## 5. Verify the tree that ships

```bash
pubgrd verify --public /tmp/out
```

A clean run looks like this:

```
==> allow.unlisted: 11 configured, 35 examined, 0 found
==> allow.missing:  11 configured, 35 examined, 0 found
==> deny.present:    0 configured + 2 built-in, 35 examined, 0 found

PASS: 35 files, all of them named by [allow]
```

Read the counts, not only the verdict. Each rule reports how many files it
examined. A rule that examined nothing fails rather than passing, because a
check that cannot fail is not a check.

Below the rules, each `[allow]` entry reports how many files it matched:

```
    coverage (files matched per [allow] entry; entries may overlap, so these need not sum to 35)

      LICENSE        1
      README.md      1
      docs/guides    3
      src           11
      tests         19
```

Nothing fails on these numbers. They are there because `allow.missing: 0 found`
means every entry matched at least one file. It does not mean the tree is
complete. An entry covering 11 of the 13 files your build produces passes here,
so compare each count against the build you just ran.

`pubgrd` cannot make that comparison for you. The expected count lives in your
private tree, and `verify` reads one tree. Clone the published tree and
`diff -rq` it against your build when you want the two compared directly.

## 6. Wire it into your publish step

Put `verify` last, after everything that touches the tree:

```make
OUT ?= /tmp/out

public:
	pubgrd cp --private . --public $(OUT) --ref $(VERSION)
	bash scripts/transform-public.sh $(OUT)
	pubgrd verify --public $(OUT)
```

Add `--ref` once your public tree comes from committed files. It restricts the
copy to paths that the ref tracks, and refuses to run if any of them differs
from that ref in your working tree. Omit it when your source is build output
that lives in no commit.

Run the target and confirm it fails when it should:

```bash
touch $(OUT)/surprise.txt
make public        # expect exit 1, naming surprise.txt
```

A gate you have never seen fail is a gate you have not tested.

## When it fails

**`FAIL: N files match no [allow] entry`** — the public tree holds something
your config never approved. Either add the path to `paths`, or find out how it
got there.

**`FAIL: N allow entries match no file in the tree`** — you named a path that
does not exist. Usually a typo, a renamed directory, or a file your build step
generates later than you expected.

**`FAIL: N allow entries matched something before [deny] and nothing after`** —
a deny pattern swallowed a path you meant to publish. Narrow the deny entry.
Nothing re-admits a denied path, so you cannot fix this from the allow side.

**`exit 2`** with a message about `paths` or `reason` — you have not finished
editing the config. Both fields must carry real values.

**A denied file marked `built-in`** — you hit `.envrc` or the `.env` family.
Those never publish and nothing overrides them. Rename the file or remove it.
The template suffixes `.env.example`, `.env.sample`, `.env.template` and
`.env.dist` still publish normally.

## Where to go next

Read `pubgrd --help` for the full command surface, and the comments inside the
`pubgrd.toml` that `init` wrote. Both explain the precedence rule and the
matching syntax in more detail than this guide does.
