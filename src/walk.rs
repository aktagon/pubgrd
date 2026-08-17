//! Enumerating a tree.
//!
//! The whitelist is only as good as the list of files it is applied to, so the
//! walk deliberately refuses every filter `ignore` offers by default. A
//! `.gitignore` inside the public tree hiding a file from `pubgrd` is fail-open
//! in exactly the direction this tool exists to catch: the file ships, and
//! nothing examined it.
//!
//! `.git/` is the one exception, pruned because every public repository holds
//! one legitimately and grading it would bury a real violation under thousands
//! of entries. Nothing else is pruned: a prune runs before grading,
//! so it is a blacklist upstream of the whitelist, and the one other entry that
//! was here published a live secret under a `PASS` line.
//!
//! A pruned directory is NAMED in the summary. Pruning and ignoring are not the
//! same thing to a reader: a bare `filtered` count is indistinguishable from a
//! walk losing files to a bug, and this walk is the list the whole whitelist is
//! applied to.
//!

use std::path::Path;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, bail};
use ignore::WalkBuilder;

/// Every file in a tree, and what the walk did to get them.
pub struct Tree {
    /// Paths relative to the tree root, `/`-separated, no leading `./`, sorted.
    /// Directories are absent: the path-matching rules makes a directory never a match target.
    pub files: Vec<String>,
    /// Every filesystem entry the walk saw, including the ones it discarded.
    pub walked: usize,
    /// Entries discarded rather than offered to the rules: directories, and
    /// the trees that were pruned. `walked - filtered == files.len()`.
    pub filtered: usize,
    /// Directories pruned whole, relative to the tree root, sorted.
    ///
    /// No file count: producing one would mean walking the tree the prune
    /// exists to avoid walking, and a number this walk did not measure is
    /// exactly the kind of claim this project keeps finding in its own records.
    pub pruned: Vec<String>,
}

impl Tree {
    /// The line every command prints before its rules run. The walk is not a
    /// rule and reports its own count separately.
    pub fn summary(&self) -> String {
        let counts = format!("{} entries walked, {} filtered", self.walked, self.filtered);
        if self.pruned.is_empty() {
            return counts;
        }
        format!(
            "{counts}\n    pruned whole, not graded: {}",
            self.pruned
                .iter()
                .map(|path| format!("{path}/"))
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

/// Directory names pruned whole rather than offered to the rules.
///
/// **One entry, and adding a second needs the argument below to hold for it.**
/// A prune runs in `filter_entry`, which is before grading, so a pruned path is
/// invisible to `[allow]` and `[deny]` alike — it is a blacklist placed upstream
/// of the whitelist. That is sound only where the directory's presence is
/// *expected* and its contents are never published content.
///
/// `.git` meets both: every public repository legitimately holds one, and
/// grading it would report thousands of violations on any real tree.
///
/// `node_modules` was here and does not meet either. It is not expected in a
/// published tree, and a `node_modules/some-pkg/.env` holding a live AWS secret
/// was walked past, never graded, and reported under `PASS: 1 files, all of them
/// named by [allow]` at exit 0 — a claim that was false as written, since one
/// file had not been shown to a rule. The volume argument that put
/// it here is real and is answered in `verify`'s reporter, which caps a detail
/// block at [`crate::verify::DETAIL_CAP`] paths.
///
/// Distinct from the conventional DENY set (`config::CONVENTIONAL_DENY`), and
/// the distinction is load-bearing: a denied path FAILS the run, a pruned one is
/// merely not graded.
///
const PRUNED: [&str; 1] = [".git"];

/// Walk `root`, applying no filter but the `PRUNED` directories.
pub fn walk(root: &Path) -> Result<Tree> {
    if !root.is_dir() {
        bail!("{} is not a directory", root.display());
    }

    // `ignore::Walk` is a flat iterator: skipping a directory it yields does
    // NOT stop it descending into that directory. Only `filter_entry` prunes,
    // and its closure is `'static`, so the pruned entries are counted through
    // shared state rather than a captured local.
    let pruned = Arc::new(Mutex::new(Vec::new()));
    let seen = Arc::clone(&pruned);
    let base = root.to_path_buf();

    let mut builder = WalkBuilder::new(root);
    builder
        // Every one of these defaults to hiding something. A whitelist applied
        // to a filtered list is a whitelist with holes in it.
        .standard_filters(false)
        .hidden(false)
        .follow_links(false)
        .filter_entry(move |entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            // The TYPE test is not decoration. `filter_entry`'s predicate fires
            // on files and symlinks too, so a name test alone pruned a regular
            // FILE named `.git` and then reported it as `pruned whole, not
            // graded: .git/` — a trailing slash asserting a directory that is
            // not there. The realistic case is the `.git` file a submodule or a
            // linked worktree carries, whose one line is `gitdir:` followed by
            // an absolute path into the private repository.
            let is_dir = entry.file_type().is_some_and(|kind| kind.is_dir());
            if is_dir && PRUNED.contains(&name.as_str()) {
                if let Ok(path) = relative(&base, entry.path())
                    && let Ok(mut seen) = seen.lock()
                {
                    seen.push(path);
                }
                return false;
            }
            true
        });

    let mut files = Vec::new();
    let mut walked = 0usize;
    let mut filtered = 0usize;

    for entry in builder.build() {
        let entry = entry.context("walking the tree")?;
        let path = entry.path();

        // The root itself is the walker's first yield and is not content.
        if path == root {
            continue;
        }

        walked += 1;

        // A directory is never a match target. Anything that is not
        // a directory is a candidate, symlinks included: `cp` refuses to write
        // one, and one that arrived by another route is a file the
        // allow-set has to name.
        if entry.file_type().is_some_and(|kind| kind.is_dir()) {
            filtered += 1;
            continue;
        }

        files.push(relative(root, path)?);
    }

    // A pruned entry was seen and then discarded, so it counts on both sides
    // and `walked - filtered == files.len()` still holds.
    // Cloned out rather than unwrapped: `builder` still owns the closure that
    // holds the other Arc, so `try_unwrap` would fail and silently yield an
    // empty list — the prune would work and report nothing.
    let mut pruned = pruned.lock().map(|seen| seen.clone()).unwrap_or_default();
    pruned.sort();
    let count = pruned.len();
    files.sort();
    Ok(Tree {
        files,
        walked: walked + count,
        filtered: filtered + count,
        pruned,
    })
}

/// `path` relative to `root`, `/`-separated, as the path-matching rules requires.
fn relative(root: &Path, path: &Path) -> Result<String> {
    let rest = path
        .strip_prefix(root)
        .with_context(|| format!("{} is not under {}", path.display(), root.display()))?;

    rest.components()
        .map(|component| {
            component.as_os_str().to_str().map(str::to_string).context(
                // The compared path is printed verbatim in diagnostics, so a
                // path that cannot be printed cannot be diagnosed either.
                "path is not valid UTF-8, and a path that cannot be printed \
                 cannot be compared against the config either",
            )
        })
        .collect::<Result<Vec<_>>>()
        .map(|parts| parts.join("/"))
        .with_context(|| format!("reading {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Build a tree in a temporary directory. `.git/config` is written because
    /// `ignore` only honours a `.gitignore` inside what it recognises as a
    /// repository, and a public tree usually is one.
    fn tree(files: &[(&str, &str)]) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("create temp tree");
        for (path, contents) in files {
            let full = dir.path().join(path);
            fs::create_dir_all(full.parent().expect("has a parent")).expect("create parent");
            fs::write(full, contents).expect("write file");
        }
        dir
    }

    /// The fail-open case. `ignore`'s default walk honours a `.gitignore` in
    /// the tree, so a file the publish step deliberately gitignored — build
    /// output, most often — would ship unexamined.
    #[test]
    fn a_gitignored_file_is_still_seen() {
        let dir = tree(&[
            (".git/config", "[core]\n"),
            (".gitignore", "dist/\n"),
            ("dist/graph.graphology.json", "{}\n"),
            ("README.md", "# public\n"),
        ]);

        let found = walk(dir.path()).expect("walk succeeds");

        assert!(
            found
                .files
                .contains(&"dist/graph.graphology.json".to_string()),
            "a gitignored file must still be examined, got {:?}",
            found.files
        );
    }

    /// `hidden(false)`. A dotfile is a file, and `.envrc` is on a real deny
    /// list precisely because it can reach a public tree.
    #[test]
    fn a_dotfile_is_still_seen() {
        let dir = tree(&[(".git/config", "[core]\n"), (".gitignore", "dist/\n")]);

        let found = walk(dir.path()).expect("walk succeeds");

        assert!(
            found.files.contains(&".gitignore".to_string()),
            "a dotfile must still be examined, got {:?}",
            found.files
        );
    }

    /// The one prune, and the only one.
    #[test]
    fn the_git_directory_is_pruned() {
        let dir = tree(&[(".git/config", "[core]\n"), ("README.md", "# public\n")]);

        let found = walk(dir.path()).expect("walk succeeds");

        assert!(
            !found.files.iter().any(|path| path.starts_with(".git/")),
            "repository machinery must not reach the rules, got {:?}",
            found.files
        );
        assert_eq!(found.files, vec!["README.md".to_string()]);
    }

    /// The counts a command prints have to add up, or the reported number is
    /// decoration.
    #[test]
    fn the_counts_account_for_every_entry() {
        let dir = tree(&[
            (".git/config", "[core]\n"),
            ("dist/graph.graphology.json", "{}\n"),
            ("README.md", "# public\n"),
        ]);

        let found = walk(dir.path()).expect("walk succeeds");

        assert_eq!(found.walked - found.filtered, found.files.len());
        assert!(
            found.summary().starts_with(&format!(
                "{} entries walked, {} filtered",
                found.walked, found.filtered
            )),
            "got {}",
            found.summary()
        );
        assert_eq!(
            found.pruned,
            vec![".git".to_string()],
            "a pruned directory must be named, not merely counted"
        );
    }
}
