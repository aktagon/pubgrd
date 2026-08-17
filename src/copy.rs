//! `pubgrd cp` — the extraction step of a publish script, and nothing else.
//!
//! It copies, reports, and tells the reader to verify the final tree. It does
//! NOT verify. An earlier draft did, on the argument that copying without
//! checking is the failure mode; that holds only when the copy produces the
//! final tree, and it does not in the primary consumer. `publish-public.sh`
//! runs three text transforms and generates `NOTICE.md` after extraction, so
//! the tree `cp` would have graded is not the tree that ships — and `NOTICE.md`
//! is on the allow-set, so the intermediate tree legitimately fails the
//! unmatched-entry rule.
//!

use std::collections::HashSet;
use std::path::Path;

use anyhow::{Context, Result, bail};

use crate::config::{CONVENTIONAL_REASON, Config};
use crate::grade::grade;
use crate::matcher::Origin;
use crate::{EXIT_OK, EXIT_VIOLATION, git, walk};

/// Copy the allow-set from `private` into `public`.
///
/// `reference` is the optional `--ref` guard. It never changes where the bytes
/// come from — that is always the filesystem — only which paths are eligible
/// and whether the copy is allowed to proceed at all.
pub fn copy(
    private: &Path,
    public: &Path,
    config: &Config,
    reference: Option<&str>,
) -> Result<i32> {
    // Which file's policy is being applied. `verify` printed this and `cp` did
    // not, so an unattended publish script's log did not record which config
    // governed the copy — and the fallback warnings below had nothing to
    // attach themselves to.
    println!("==> config {}", config.source.display());
    crate::config::warn_fallbacks(config);

    let files = match reference {
        Some(reference) => {
            let tracked = git::tracked_at(private, reference)?;
            println!("==> {} paths tracked at {reference}", tracked.len());
            tracked
        }
        None => {
            let tree = walk::walk(private)?;
            println!("==> {}", tree.summary());
            tree.files
        }
    };

    let graded = grade(&files, &config.allow.paths, Some(&config.deny));

    // Only the collisions. `graded.denied` also holds files that match [deny]
    // and no [allow] entry, which `verify` must report — they are IN the public
    // tree — and which `cp` must not, because they were never going to be
    // copied. Printing them here made a 3-file copy from a repository with a
    // node_modules/ read as "3 allow entries → 10 candidates", and the
    // arithmetic README.md documents (candidates - excluded = copied) is what
    // showed it up.
    let excluded: Vec<_> = graded
        .collisions
        .iter()
        .map(|collision| collision.path)
        .collect();
    let candidates = graded.included.len() + excluded.len();
    println!(
        "==> {} allow entries → {candidates} candidates",
        config.allow.paths.len()
    );

    if !excluded.is_empty() {
        println!("==> deny excluded {}:", excluded.len());
        for path in &excluded {
            println!("                  {path}");
        }
    }

    // A code review found this. `verify` refuses to grade an empty set; `cp` was exempt from its own
    // project's founding rule and reported `copied 0 files` at exit 0.
    //
    // TOTAL vacuity only. The paragraph below on the one check `cp` runs is
    // unchanged: an individual entry matching nothing stays silent, because
    // `NOTICE.md` does not exist until the transforms run and a not-yet-
    // generated file must not read as an over-denial. Some entries matching
    // nothing is a workflow; all of them matching nothing is a wrong
    // `--private`, an empty tree, or a ref with nothing tracked under the
    // prefix — which is how a code review presented, silently.
    if candidates == 0 {
        bail!(
            "{} allow entries matched no file under {}. A copy that selects nothing is not a \
             copy.\nCheck that --private names the tree the allow-set was written for{}. Nothing \
             was written.",
            config.allow.paths.len(),
            private.display(),
            match reference {
                Some(reference) => format!(", and that {reference} tracks anything under it"),
                None => String::new(),
            }
        );
    }

    // Before anything is written. The current script this replaces extracts
    // from HEAD and silently ignores worktree state; `--ref HEAD` refuses
    // outright, which is the more honest of the two behaviours. A path listed
    // here is one whose bytes on disk are not the ref's bytes — including a
    // path deleted from the worktree, which is why the check runs over what
    // the ref says the allow-set covers rather than over what the walk found.
    if let Some(reference) = reference {
        let dirty = git::dirty_against(private, reference)?;
        let mut offending: Vec<_> = graded
            .included
            .iter()
            .filter(|path| dirty.contains(**path))
            .collect();
        offending.sort();
        if !offending.is_empty() {
            bail!(
                // The remedy list grew with a code review. While the guard compared
                // against HEAD, "commit them" was the whole answer. Now that it
                // compares against the ref, the commonest case is a tag the
                // work has moved past, where the path is already committed and
                // the tag is what is stale.
                "{} allowed path(s) differ from {} in the worktree:\n{}\nCommit them, move {} to \
                 include them, or drop them from [allow] paths. Nothing was written.",
                offending.len(),
                reference,
                offending
                    .iter()
                    .map(|path| format!("  {path}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                reference
            );
        }
    }

    // Before anything is written. A symlink is refused rather than copied: its
    // target is a route into the public tree that neither set inspects, and no
    // surveyed project has one. Collect them all — a user fixing links one run
    // at a time learns nothing they could not have learned at once.
    let links: Vec<_> = graded
        .included
        .iter()
        .filter(|path| is_symlink(&private.join(path)))
        .collect();
    if !links.is_empty() {
        bail!(
            "{} symbolic link(s) in the allow-set:\n{}\nReplace each with a real file. A link's \
             target is a route into the public tree that neither [allow] nor [deny] inspects.",
            links.len(),
            links
                .iter()
                .map(|path| format!("  {path}"))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    let already_there = existing(public)?;

    for path in &graded.included {
        let from = private.join(path);
        let to = public.join(path);
        if let Some(parent) = to.parent() {
            // Only the parents of files actually written. The allow-set names
            // files, and a directory exists in the public tree because a file
            // needed it.
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        // Preserves the mode bits, including the executable one, and does not
        // preserve mtime. The public tree is derived, not archived.
        std::fs::copy(&from, &to)
            .with_context(|| format!("copying {} to {}", from.display(), to.display()))?;
    }
    println!("==> copied {} files", graded.included.len());

    // The exit contract: `cp` does not remove what it did not put there, so the verify
    // pass that follows will report files this run never touched. Naming them
    // here is what stops the first adoption on an existing public repository
    // reading as the tool malfunctioning.
    let written: HashSet<&str> = graded.included.iter().copied().collect();
    let stale: Vec<_> = already_there
        .iter()
        .filter(|path| !written.contains(path.as_str()))
        .collect();
    if !stale.is_empty() {
        println!(
            "\nwarning: {} file(s) were already in {} and this run did not write them. \
             `cp` does not delete; verify will report any that are unlisted:",
            stale.len(),
            public.display()
        );
        for path in &stale {
            println!("  {path}");
        }
    }

    println!(
        "\n==> NOTHING VERIFIED. Run `pubgrd verify --public {}`\n    after anything that \
         post-processes this tree.",
        public.display()
    );

    // The one check `cp` runs, and it is not verification. An allow entry that
    // matched something in --private before [deny] and nothing after is a
    // config defect visible at copy time. An entry that never matched is NOT
    // reported: `NOTICE.md` does not exist until the transforms run, and an
    // over-denial and a not-yet-generated file must not read the same.
    let swallowed: Vec<_> = graded
        .unmatched
        .iter()
        .filter_map(|unmatched| {
            unmatched
                .swallowed_by
                .map(|deny| (unmatched.entry.raw(), deny.raw(), deny.origin()))
        })
        .collect();

    if swallowed.is_empty() {
        return Ok(EXIT_OK);
    }

    println!(
        "\nFAIL: {} allow {} matched something before [deny] and nothing after\n",
        swallowed.len(),
        if swallowed.len() == 1 {
            "entry"
        } else {
            "entries"
        }
    );
    // Attributed, for the reason `verify` is: a built-in denial announced as
    // `excluded by [deny]` sends the reader to grep a file that does not hold
    // the rule, and `cp` is the command a publish script runs unattended.
    let mut conventional = false;
    let mut configured = false;
    for (entry, deny, origin) in &swallowed {
        match origin {
            Origin::Conventional => {
                conventional = true;
                println!(
                    "  {entry} — on [allow] paths, excluded by the built-in deny set (`{deny}`)"
                );
            }
            Origin::Config => {
                configured = true;
                println!("  {entry} — on [allow] paths, excluded by [deny] `{deny}`");
            }
        }
    }
    if configured && let Some(reason) = &config.deny_reason {
        println!("      reason (deny): {reason:?}");
    }
    if conventional {
        println!("      reason (built-in): {CONVENTIONAL_REASON}");
    }
    Ok(EXIT_VIOLATION)
}

fn is_symlink(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

/// What is in the destination before this run touches it. An absent
/// destination is ordinary — `cp` creates it as it writes.
fn existing(public: &Path) -> Result<Vec<String>> {
    if !public.exists() {
        return Ok(Vec::new());
    }
    Ok(walk::walk(public)?.files)
}
