//! The two git commands `--ref` needs.
//!
//! The copy-source rules settles this: `cp` copies from the filesystem, always. `--ref`
//! adds a guard and does not change the source — the allow-set expands only
//! over paths tracked at the ref, and the copy is refused if any of them is
//! dirty in the worktree. Under that condition the bytes on disk are known
//! equal to the ref's bytes, so the result matches a `git archive` extraction
//! without `pubgrd` reading a single blob.
//!
//! git is a subprocess rather than a library. Two stable, plumbing-adjacent
//! commands do not justify `gix` or `git2`, and the path-matching rules requires a sixth crate
//! to carry its own justification.
//!

use std::collections::HashSet;
use std::path::Path;
use std::process::Command;

use anyhow::{Result, anyhow, bail};

/// Paths tracked at `reference`, relative to `private`.
///
/// `--full-name` is load-bearing. Without it `ls-tree` emits paths
/// relative to the INVOCATION directory, so with `-C private` they are already
/// relative to `--private` — and the `prefix` subtraction below then removed a
/// component that was never there, dropping every path through `filter_map`.
/// `cp` copied nothing and exited 0. With `--full-name` both this and
/// [`dirty_against`] speak repository-root coordinates, so one subtraction is
/// correct for both.
pub fn tracked_at(private: &Path, reference: &str) -> Result<Vec<String>> {
    // No pathspec, so the `fatal: pathspec … did not match any files` message
    // an earlier draft of the copy-source rules told the implementer to wrap cannot arise
    // here. The reachable failures are an unknown ref and a --private that is
    // not a worktree, and both are wrapped below.
    let out = run(
        private,
        &["ls-tree", "-r", "--name-only", "--full-name", reference],
    )
    .map_err(|error| {
        anyhow!(
            "--ref {reference}: {error}\nIf {} is not a git worktree, drop --ref — `cp` reads the \
             filesystem either way, it just loses the committed-only guarantee.",
            private.display()
        )
    })?;

    let prefix = prefix(private)?;
    Ok(out
        .lines()
        .filter_map(|path| under(&prefix, path))
        .collect())
}

/// Every path differing between `reference` and the worktree, relative to
/// `private`.
///
/// **This function must take the ref, and the one it replaced could not.**
/// The guard was specified and implemented as `git status --porcelain`,
/// which reports differences against `HEAD` and accepts no revision argument.
/// The two assertions "the worktree matches `HEAD`" and "the worktree matches
/// `<ref>`" coincide only when `HEAD == <ref>`, so once `HEAD` moved past a tag
/// the guard saw a clean tree, approved, and `cp` copied the later commit's
/// bytes while reporting the tag's file list — at exit 0.
///
/// `git diff <ref>` compares the ref against the working tree, staged and
/// unstaged both, which is what the guard always claimed to do. It also needs
/// no status-column parsing: one NUL-terminated path per record, no `R  old ->
/// new` to reconstruct. A rename now arrives as two independent paths, which is
/// the correct reading — a path renamed away is no longer what the ref says,
/// and neither is the path renamed into place.
///
/// **Untracked files are consulted, and the argument that they need not be was
/// wrong.** It ran: the allow-set expands only over paths tracked at the ref, so
/// an untracked file is never a copy candidate. The premise is true and the
/// conclusion does not follow. Candidacy comes from [`tracked_at`]; the *bytes*
/// come from disk. So a path tracked at the ref, renamed away since, and holding
/// an untracked file today is a candidate whose contents git never vouched for —
/// and `git diff` says nothing at all about untracked files. Reproduced: a
/// `src/statute.py` tracked at `v1`, `git mv`d away, then rewritten untracked
/// with a live key in it, copied at exit 0.
///
/// **Rename detection is off.** `git diff` defaults to reporting a rename as its
/// destination path only, which drops the source path — the one still listed at
/// the ref, and therefore the one `cp` would write.
///
/// **`diff.relative` is neutralised.** Set anywhere in the config chain it makes
/// `diff` emit paths already relative to `--private`, so the [`prefix`]
/// subtraction below removes a component that is not there and every path falls
/// out of the `filter_map` — an empty dirty set over a modified worktree. That
/// is a code review's double-strip reappearing inside the function written to close a code review.
pub fn dirty_against(private: &Path, reference: &str) -> Result<HashSet<String>> {
    let changed = run(
        private,
        &[
            "-c",
            "diff.relative=false",
            "diff",
            "--name-only",
            "--no-renames",
            "-z",
            reference,
            "--",
        ],
    )?;
    // `--full-name` for the same reason `tracked_at` needs it: without it these
    // paths are relative to the invocation directory and the subtraction below
    // is wrong for exactly the layouts a code review was about.
    let untracked = run(
        private,
        &[
            "ls-files",
            "--others",
            "--exclude-standard",
            "--full-name",
            "-z",
        ],
    )?;
    let prefix = prefix(private)?;

    Ok(changed
        .split('\0')
        .chain(untracked.split('\0'))
        .filter(|path| !path.is_empty())
        .filter_map(|path| under(&prefix, path))
        .collect())
}

/// Where `private` sits inside its repository, as `sub/dir/` or empty.
///
/// git reports paths relative to the repository root, and `--private` is
/// allowed to be a subdirectory of one.
fn prefix(private: &Path) -> Result<String> {
    Ok(run(private, &["rev-parse", "--show-prefix"])?
        .trim_end_matches('\n')
        .to_string())
}

/// `path` relative to `private`, or `None` when it sits outside it.
fn under(prefix: &str, path: &str) -> Option<String> {
    if prefix.is_empty() {
        return Some(path.to_string());
    }
    path.strip_prefix(prefix).map(str::to_string)
}

/// Run git in `private` and return its stdout.
fn run(private: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(private)
        // Without this git C-quotes any path holding a non-ASCII byte. Turning
        // it off is cheaper and less wrong than reimplementing git's unquoting.
        .args(["-c", "core.quotePath=false"])
        .args(args)
        .output()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => anyhow!(
                "`git` is not on PATH, and --ref needs it. Drop --ref to copy from the filesystem \
                 without the guard"
            ),
            _ => anyhow!("running git: {error}"),
        })?;

    if !out.status.success() {
        // git's own text, quoted rather than passed through, so the reader can
        // tell which tool is speaking.
        bail!(
            "git {} failed. git said: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}
