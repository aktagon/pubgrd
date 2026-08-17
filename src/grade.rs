//! Two sets and a subtraction.
//!
//! Precedence is absolute: a path matching `[deny]` is
//! excluded however specifically `[allow]` names it. There is no re-admit, no
//! ordering, and no specificity ranking, because under-denying publishes
//! something private and over-denying withholds a file, and a precedence rule
//! should resolve toward the recoverable error.
//!
//! `verify` and `cp` both grade a file list this way. They differ in what they
//! do with the answer, not in how they reach it.
//!

use crate::matcher::{Entry, PathSet};

/// A path named by both sets. Excluded, and warned about.
pub struct Collision<'a> {
    pub path: &'a str,
    pub allow: &'a Entry,
    pub deny: &'a Entry,
}

/// How many files one `[allow]` entry claimed.
///
/// This is the number `allow.missing` collapses. That rule asks whether an
/// entry matched anything, so it answers with a boolean. An entry covering 11
/// of the 13 files a build produces then reads the same as one covering all 13.
///
/// FEEDBACK-001 measured that case in a public tree four sessions stale.
/// `pubgrd` cannot know 13 was expected and does not try. The second tree is
/// the only place that number lives, and `verify` reads one tree.
///
/// Counted over `included`, so a file the entry names and `[deny]` removed does
/// not count. Counting before `[deny]` would print a healthy number for an
/// entry that [`Unmatched`] reports, four lines below, as covering nothing.
///
/// Entries may overlap. `scripts` and `scripts/setup-hooks.sh` both claim the
/// same file, so these counts do not sum to `included.len()`, and the report
/// says so above them. Attributing each file to its first matching entry would
/// sum. It would also make the column depend on entry order, which the
/// precedence rule fixes as meaningless.
pub struct Coverage<'a> {
    pub entry: &'a Entry,
    /// Files in `included` this entry matches.
    pub matched: usize,
}

/// An allow entry that ended up covering nothing.
pub struct Unmatched<'a> {
    pub entry: &'a Entry,
    /// The deny entry that swallowed it, when the allow entry matched
    /// something before `[deny]` was applied and nothing after.
    ///
    /// `None` means it never matched anything at all, which is a different
    /// finding: over-denial versus a file that is simply not there. The exit contract
    /// makes the distinction load-bearing, because one surveyed project
    /// generates `NOTICE.md` after the copy and it is on the allow-set.
    pub swallowed_by: Option<&'a Entry>,
}

/// What the two sets say about one file list.
pub struct Grading<'a> {
    /// Allowed and not denied. The set that may ship.
    pub included: Vec<&'a str>,
    /// Matching no `[allow]` entry.
    pub unlisted: Vec<&'a str>,
    /// Matching `[deny]`, whether or not `[allow]` also names it. A file that
    /// is both denied and unlisted is reported here only: two findings for one
    /// path would inflate the count without telling the reader anything the
    /// stronger of the two does not already say.
    pub denied: Vec<&'a str>,
    pub collisions: Vec<Collision<'a>>,
    /// the requirement, read as amended: after `[deny]` is applied.
    pub unmatched: Vec<Unmatched<'a>>,
    /// Every `[allow]` entry and how many files it claimed. `unmatched` is the
    /// subset of this where `matched` is zero.
    pub coverage: Vec<Coverage<'a>>,
}

/// Apply both sets to `files`.
pub fn grade<'a>(
    files: &'a [String],
    allow: &'a PathSet,
    deny: Option<&'a PathSet>,
) -> Grading<'a> {
    let mut included = Vec::new();
    let mut unlisted = Vec::new();
    let mut denied = Vec::new();
    let mut collisions = Vec::new();

    for file in files {
        let path = file.as_str();
        let allowed = allow.matching(path);
        let refused = deny.and_then(|set| set.matching(path));

        match (allowed, refused) {
            (Some(allow), Some(deny)) => {
                collisions.push(Collision { path, allow, deny });
                denied.push(path);
            }
            (None, Some(_)) => denied.push(path),
            (Some(_), None) => included.push(path),
            (None, None) => unlisted.push(path),
        }
    }

    let coverage: Vec<Coverage> = allow
        .entries()
        .iter()
        .map(|entry| Coverage {
            entry,
            matched: included.iter().filter(|path| entry.matches(path)).count(),
        })
        .collect();

    // Derived from `coverage` rather than computed beside it. The rule is
    // `matched == 0`, so the two cannot disagree about which entries matched
    // nothing. Both used to walk the entries independently.
    let unmatched = coverage
        .iter()
        .filter(|covered| covered.matched == 0)
        .map(|covered| Unmatched {
            swallowed_by: files
                .iter()
                .filter(|file| covered.entry.matches(file))
                .find_map(|file| deny.and_then(|set| set.matching(file))),
            entry: covered.entry,
        })
        .collect();

    Grading {
        included,
        unlisted,
        denied,
        collisions,
        unmatched,
        coverage,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files(paths: &[&str]) -> Vec<String> {
        paths.iter().map(|path| (*path).to_string()).collect()
    }

    /// The rule stated as an absolute: an explicit allow does not re-admit.
    #[test]
    fn deny_wins_over_an_allow_entry_naming_the_path_exactly() {
        let files = files(&["scripts/setup-hooks.sh", "scripts/publish.sh"]);
        let allow = PathSet::parse(&["scripts", "scripts/setup-hooks.sh"]).expect("compiles");
        let deny = PathSet::parse(&["scripts/setup-hooks.sh"]).expect("compiles");

        let graded = grade(&files, &allow, Some(&deny));

        assert_eq!(graded.included, vec!["scripts/publish.sh"]);
        assert_eq!(graded.denied, vec!["scripts/setup-hooks.sh"]);
    }

    /// Order carries no meaning, so the same config written backwards grades
    /// the same way.
    #[test]
    fn entry_order_changes_nothing() {
        let files = files(&["scripts/setup-hooks.sh", "scripts/publish.sh"]);
        let forwards = PathSet::parse(&["scripts", "scripts/setup-hooks.sh"]).expect("compiles");
        let backwards = PathSet::parse(&["scripts/setup-hooks.sh", "scripts"]).expect("compiles");
        let deny = PathSet::parse(&["scripts/setup-hooks.sh"]).expect("compiles");

        assert_eq!(
            grade(&files, &forwards, Some(&deny)).included,
            grade(&files, &backwards, Some(&deny)).included
        );
    }

    /// Over-denial and a not-yet-generated file must not produce the same
    /// diagnostic. The exit contract spells this out because `NOTICE.md` is on the
    /// allow-set and does not exist until after the transforms run.
    #[test]
    fn an_entry_swallowed_by_deny_is_distinguishable_from_one_that_never_matched() {
        let files = files(&["README.md"]);
        let allow = PathSet::parse(&["README.md", "NOTICE.md"]).expect("compiles");
        let deny = PathSet::parse(&["**/*.md"]).expect("compiles");

        let graded = grade(&files, &allow, Some(&deny));

        let swallowed: Vec<_> = graded
            .unmatched
            .iter()
            .map(|unmatched| (unmatched.entry.raw(), unmatched.swallowed_by.is_some()))
            .collect();
        assert_eq!(swallowed, vec![("README.md", true), ("NOTICE.md", false)]);
    }

    /// With no `[deny]` block at all, nothing is excluded.
    #[test]
    fn an_allow_only_config_excludes_nothing() {
        let files = files(&["index.html", "TODO.md"]);
        let allow = PathSet::parse(&["index.html"]).expect("compiles");

        let graded = grade(&files, &allow, None);

        assert_eq!(graded.included, vec!["index.html"]);
        assert_eq!(graded.unlisted, vec!["TODO.md"]);
        assert!(graded.denied.is_empty());
    }
}
