//! `pubgrd init` — a template that cannot go green, and reads no tree.
//!
//! The `init` template design. It used to seed an allow-set from the existing public tree, and
//! that is removed rather than repaired. `seed()` collapsed every path holding a
//! `/` to its top-level component, so a public tree containing only
//! `docs/adrs/001.md` produced the entry `docs` — which, applied to the
//! *private* tree, also admits `docs/handoffs/`. `confirm()` compared public
//! against private and marked files present in public and absent from private,
//! the opposite direction, so the widening was structurally unmarkable. It
//! reported `0 entry/entries marked` while doing it: a clean bill of health from
//! an examination that could not produce a finding.
//!
//! Its one production run, against a live public tree, proposed **allowing**
//! `.envrc`, an entire build directory, a compiler target directory, and every
//! file underneath them.
//!
//! What survives from an earlier design of `init` is the shape of the refusals: the write target,
//! the overwrite refusal, the `reason = "TODO"` placeholder, and the no-git
//! rule. What replaces the seeding is convention over configuration — an inert
//! template, plus the defaults in the conventional deny set covering what every project would
//! otherwise have to remember.
//!

use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::EXIT_USAGE;
use crate::config::{FILENAME, UNSET_REASON};

/// Write the template to `<private>/pubgrd.toml`.
///
/// Always returns [`EXIT_USAGE`]. The configuration it writes has an empty
/// allow-set *and* no `reason`, and both are refused at load, so running the
/// command cannot satisfy the gate.
pub fn init(private: Option<&Path>) -> Result<i32> {
    // No tree is read, so `init` cannot propose a path that is not there — nor
    // one that is. It also makes no git call, which follows the copy-source rules and is now
    // trivially true.
    let target = private.unwrap_or(Path::new(".")).join(FILENAME);
    if target.exists() {
        bail!(
            "{} already exists. Regenerating over it would discard the `reason` strings that are \
             the file's whole value",
            target.display()
        );
    }

    std::fs::write(&target, TEMPLATE).with_context(|| format!("writing {}", target.display()))?;

    println!("==> wrote {}", target.display());
    // A checklist, not one instruction. The template carries BOTH defects, so a
    // message naming only the reason is false, and a reader who obeys it meets a
    // second exit 2 disclosing the second requirement. The exit contract requires the
    // loader to report both together for the same reason.
    println!(
        "\nFAIL: this configuration refuses to grade anything until you make two edits:\n\
         \n  1. list the files that may be published, in `[allow] paths`\n\
         \x20    it is empty, and a tree graded against nothing is not a check\n\
         \n  2. replace `reason = {UNSET_REASON:?}` with what the allow-set is for\n\
         \x20    a scaffold that goes green on generation teaches the reader that\n\
         \x20    the gate is satisfied by running a command\n\
         \nThe file explains both, and `pubgrd --help` has the rest."
    );
    Ok(EXIT_USAGE)
}

/// The generated file.
///
/// It documents rather than configures. Nothing here is a guess about the
/// reader's tree, because `init` never looked at one.
const TEMPLATE: &str = r#"# pubgrd.toml — what may be published from this repository, and why.
#
# Written by `pubgrd init`. It grades nothing until you edit it: `paths` is
# empty and `reason` is a placeholder, and both are refused. See `pubgrd --help`
# for the full surface.
#
#
# DENY ALWAYS WINS. A path matching [deny] is excluded no matter how
# specifically [allow] names it. There is no re-admit, and order is irrelevant.
# Under-denying publishes something private and is irreversible; over-denying
# withholds a file and is recoverable, and both directions have a detector. So a
# broad deny pattern is safe to write, and is the intended posture.
#
#
# HOW A PATH IS MATCHED. Two shapes:
#
#   "src"            a literal. Matches `src` and everything under `src/`.
#                    Does NOT match `vendor/src` or `srcfile.rs`.
#   "**/CLAUDE.md"   a glob. `*` and `?` stop at `/`; `**` crosses it. So
#                    `*.md` matches `README.md` but not `docs/README.md`.
#
# A trailing slash is a habit and is stripped: "dist/" and "dist" are the same
# entry. Matching is case-sensitive on every platform, because a case-insensitive
# match passes on macOS and leaks on the Linux host that serves the tree.
#
#
# ALREADY DENIED, on every run, without being listed here:
#
#   .envrc           credentials, matched in any case.
#   .env             and its whole family — .env.local, .env.production, and
#                    any other .env.<suffix>. Not overridable: remove or
#                    rename the file.
#
#                    STILL PUBLISHABLE: .env.example, .env.sample,
#                    .env.template, .env.dist. Those are templates, and
#                    shipping one is the right thing to do.
#
# ALREADY PRUNED, never graded either way:
#
#   .git/            the only one, because a prune runs BEFORE grading and is
#                    therefore a hole in this whitelist. Everything else in
#                    your tree gets graded, node_modules/ included.
#
#
# WHERE TO START, if this repository already has a published tree:
#
#   1. put one file you know belongs in `paths`, below
#   2. run `pubgrd verify --public <tree>` — every OTHER file is listed as
#      unlisted, which is your inventory
#   3. promote the ones that belong, by hand
#
# By hand on purpose. Transcribing a path means you adjudicated it; pasting a
# generated block means you did not, and a generated allow-set is how a tool
# launders an existing leak into policy.

[allow]
paths  = []
reason = "TODO"

# [deny] is optional and is a SECOND assertion, never the only one. A config with
# no [allow] block is refused outright: comparing a tree against nothing and
# printing PASS is how a gate becomes decoration.
#
# [deny]
# paths  = ["**/CLAUDE.md", "**/TODO.md", "docs/internal"]
# reason = "working state and house conventions"
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CONVENTIONAL_DENY;

    /// The template is inert on BOTH counts, which is what makes it safe to
    /// ship an empty allow-set at all.
    #[test]
    fn the_template_carries_an_empty_allow_set_and_no_reason() {
        assert!(
            TEMPLATE.contains("paths  = []"),
            "the allow-set must be empty: init proposes nothing"
        );
        assert!(
            TEMPLATE.contains(&format!("reason = {UNSET_REASON:?}")),
            "the reason must be the placeholder every command refuses"
        );
    }

    /// The template must not propose entries. A generated allow-set is the
    /// failure the `init` template design removed, and a template that grew one back would be the
    /// same defect wearing a different name.
    #[test]
    fn the_template_proposes_no_path() {
        // Split on the BLOCK, not the word: `[allow]` appears in the prose
        // above it, explaining precedence.
        let allow = TEMPLATE
            .split("\n[allow]\n")
            .nth(1)
            .expect("the template has an [allow] block");
        let paths = allow
            .lines()
            .find(|line| line.starts_with("paths"))
            .expect("the block has a paths line");

        assert_eq!(
            paths.trim(),
            "paths  = []",
            "init must not seed an entry, not even a plausible one"
        );
    }

    /// The conventional set is stated where the operator will read it. A default
    /// nobody can see is a default nobody can audit.
    #[test]
    fn the_template_names_the_conventional_deny_set() {
        // `.env` is a PREFIX of `.envrc`, so a bare `contains` for each is
        // satisfied by the single string `.envrc` and proves nothing about the
        // other. Anchored to the column the template actually lays them out in.
        for name in CONVENTIONAL_DENY {
            assert!(
                TEMPLATE.contains(&format!("#   {name} ")),
                "{name} is denied on every run and the template must name it in its own right"
            );
        }
        assert!(
            TEMPLATE.contains("family"),
            "the template must say .env stands for a family, or a reader assumes .env.local is \
             safe"
        );
        for suffix in crate::matcher::ENV_TEMPLATE_SUFFIXES {
            assert!(
                TEMPLATE.contains(&format!(".env.{suffix}")),
                ".env.{suffix} is publishable and the rule's phrasing invites the opposite reading"
            );
        }
    }

    /// The bootstrap loop, since nothing is seeded. Without it the reader has no
    /// route from an existing public tree to an allow-set.
    #[test]
    fn the_template_teaches_the_bootstrap_loop() {
        assert!(
            TEMPLATE.contains("pubgrd verify --public"),
            "verify over a one-entry config IS the inventory, and it must be documented"
        );
        assert!(
            TEMPLATE.contains("pubgrd --help"),
            "the template must point at the rest of the surface"
        );
    }
}
