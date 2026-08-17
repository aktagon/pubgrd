//! `pubgrd cp --ref <REF>` — the guard, not a change of source.
//!
//! The copy-source rules. One surveyed project extracts with `git archive HEAD` because
//! publishing from the worktree "would reintroduce exactly the defect found on
//! 2026-08-14, when a `test -f` guard passed an uncommitted file". `--ref`
//! preserves that protection while the bytes still come from disk, which is
//! what `ai-economy` needs, since its `dist/` is gitignored and in no commit.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

/// A git repository with one commit holding `files`.
fn repository(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().expect("create temp repo");
    write(dir.path(), files);
    git(dir.path(), &["init", "--quiet"]);
    git(dir.path(), &["add", "-A"]);
    git(
        dir.path(),
        &[
            "-c",
            "user.name=pubgrd tests",
            "-c",
            "user.email=tests@example.invalid",
            "commit",
            "--quiet",
            "-m",
            "the committed tree",
        ],
    );
    dir
}

/// Commit whatever is in the worktree, so `HEAD` moves and the tree is clean.
fn commit(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c",
            "user.name=pubgrd tests",
            "-c",
            "user.email=tests@example.invalid",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    );
}

/// Annotated, because this environment's git refuses a lightweight `git tag`
/// with "fatal: no tag message?".
///
/// The identity and the signing switch come from [`git`], which injects them on
/// every call. Without that this helper inherited the author's global
/// `tag.gpgsign=true` and `gpg.format=ssh` and died with "no tag message?" —
/// green on one machine, red on any CI runner without that key.
fn tag(dir: &Path, name: &str) {
    git(dir, &["tag", "-a", name, "-m", name]);
}

fn write(root: &Path, files: &[(&str, &str)]) {
    for (path, contents) in files {
        let full = root.join(path);
        fs::create_dir_all(full.parent().expect("has a parent")).expect("create parent");
        fs::write(full, contents).expect("write file");
    }
}

/// Every git call in this file, with the ambient configuration neutralised.
///
/// The identity and both signing switches are injected here rather than at each
/// call site: a fixture that inherits the author's `~/.gitconfig` passes on the
/// author's machine and fails everywhere else, and the failure looks like a
/// defect in `pubgrd` rather than in the fixture.
fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "user.name=pubgrd tests",
            "-c",
            "user.email=tests@example.invalid",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
        ])
        .args(args)
        .output()
        .expect("run git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn config(contents: &str) -> TempDir {
    let dir = tempfile::tempdir().expect("create temp config dir");
    fs::write(dir.path().join("pubgrd.toml"), contents).expect("write config");
    dir
}

fn cp_ref(private: &Path, public: &Path, config_dir: &Path, reference: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pubgrd"))
        .args(["cp", "--private"])
        .arg(private)
        .arg("--public")
        .arg(public)
        .arg("--config")
        .arg(config_dir.join("pubgrd.toml"))
        .arg("--ref")
        .arg(reference)
        .output()
        .expect("run pubgrd")
}

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

const SRC: &str = r#"
[allow]
paths  = ["src", "README.md"]
reason = "everything needed to read and run the artifact"
"#;

/// The one T6b names. A publish run started during uncommitted work fails
/// instead of quietly publishing the committed version.
#[test]
fn a_dirty_allowed_path_blocks_the_copy() {
    let private = repository(&[
        ("src/host/statute.py", "def cite():\n    pass\n"),
        ("README.md", "# public\n"),
    ]);
    write(
        private.path(),
        &[(
            "src/host/statute.py",
            "def cite():\n    return 'uncommitted'\n",
        )],
    );
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SRC);

    let out = cp_ref(private.path(), public.path(), cfg.path(), "HEAD");
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(2),
        "a dirty allowed path is a usage error, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        said.contains("src/host/statute.py"),
        "the refusal must name the dirty path\n{said}"
    );
    assert!(
        !public.path().join("README.md").exists(),
        "the refusal must come before anything is written\n{said}"
    );
}

/// A path dirty but NOT on the allow-set is not this tool's business.
#[test]
fn a_dirty_path_outside_the_allow_set_does_not_block_the_copy() {
    let private = repository(&[
        ("src/host/statute.py", "def cite():\n    pass\n"),
        ("README.md", "# public\n"),
        ("TODO.md", "# TODO\n"),
    ]);
    write(
        private.path(),
        &[("TODO.md", "# TODO\n\n- still working\n")],
    );
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SRC);

    let out = cp_ref(private.path(), public.path(), cfg.path(), "HEAD");
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "a dirty unpublished path is not a reason to refuse, got {:?}\n{said}",
        out.status.code()
    );
    assert!(public.path().join("README.md").exists(), "{said}");
}

/// The allow-set expands only over paths tracked at the ref. An untracked file
/// under an allowed directory is the 2026-08-14 defect, and it does not ship.
#[test]
fn an_untracked_file_under_an_allowed_directory_is_not_copied() {
    let private = repository(&[
        ("src/host/statute.py", "def cite():\n    pass\n"),
        ("README.md", "# public\n"),
    ]);
    write(
        private.path(),
        &[("src/host/scratch.py", "SECRET = 'not committed'\n")],
    );
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SRC);

    let out = cp_ref(private.path(), public.path(), cfg.path(), "HEAD");
    let said = said(&out);

    assert!(
        !public.path().join("src/host/scratch.py").exists(),
        "an uncommitted file must not reach the public tree under --ref\n{said}"
    );
    assert!(
        public.path().join("src/host/statute.py").exists(),
        "the committed file must still arrive\n{said}"
    );
}

/// a code review, and the case the other five in this file could not reach.
///
/// Every one of them passes `HEAD`, which is the single ref value for which the
/// defect is invisible: the guard ran `git status --porcelain`, which compares
/// against `HEAD` and takes no revision, so `worktree == HEAD` and
/// `worktree == <ref>` are the same assertion only when `HEAD == <ref>`.
///
/// Here `HEAD` has moved one commit past the tag and the worktree is CLEAN.
/// The old guard saw a clean tree, approved, and copied the later commit's
/// bytes while reporting the tag's file list.
#[test]
fn a_ref_that_head_has_moved_past_blocks_the_copy() {
    let private = repository(&[
        ("src/host/statute.py", "def cite():\n    return 'v1'\n"),
        ("README.md", "# public\n"),
    ]);
    tag(private.path(), "v1");
    write(
        private.path(),
        &[(
            "src/host/statute.py",
            "def cite():\n    return 'SECRET-FROM-MASTER'\n",
        )],
    );
    commit(private.path(), "work that happened after the tag");

    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SRC);

    let out = cp_ref(private.path(), public.path(), cfg.path(), "v1");
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(2),
        "the worktree does not match v1, so the copy must refuse. got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        said.contains("src/host/statute.py"),
        "the refusal must name the path that differs from the ref\n{said}"
    );
    assert!(
        !public.path().join("src/host/statute.py").exists(),
        "nothing may be written before the refusal\n{said}"
    );
}

/// The other half of a code review: a ref HEAD has moved past, with the worktree
/// genuinely equal to that ref, must still copy. The fix must refuse a real
/// difference without refusing every non-HEAD ref outright.
#[test]
fn a_ref_that_head_has_moved_past_still_copies_when_the_tree_matches_it() {
    let private = repository(&[
        ("src/host/statute.py", "def cite():\n    return 'v1'\n"),
        ("README.md", "# public\n"),
    ]);
    tag(private.path(), "v1");
    // A commit that touches nothing on the allow-set.
    write(private.path(), &[("TODO.md", "- still working\n")]);
    commit(private.path(), "notes, not published");

    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SRC);

    let out = cp_ref(private.path(), public.path(), cfg.path(), "v1");
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "every allowed path still matches v1, so the copy must proceed. got {:?}\n{said}",
        out.status.code()
    );
    assert_eq!(
        fs::read_to_string(public.path().join("src/host/statute.py")).expect("copied"),
        "def cite():\n    return 'v1'\n",
        "the bytes must be the ref's\n{said}"
    );
}

/// A code review found this. `git ls-tree` emits paths relative to the invocation directory, so
/// when `--private` is a subdirectory those paths are ALREADY relative to it.
/// Subtracting `rev-parse --show-prefix` a second time dropped every one, and
/// `cp` copied nothing and exited 0.
///
/// Invisible in every other test here, and in this project's own `Makefile`,
/// because `--private` is the repository root and the prefix is empty.
#[test]
fn a_private_tree_below_the_repository_root_still_copies() {
    let root = tempfile::tempdir().expect("create temp repo");
    write(
        root.path(),
        &[
            ("proj/src/host/statute.py", "def cite():\n    pass\n"),
            ("proj/README.md", "# public\n"),
            ("unrelated.md", "not part of the project\n"),
        ],
    );
    git(root.path(), &["init", "--quiet"]);
    commit(root.path(), "the committed tree");

    let private = root.path().join("proj");
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SRC);

    let out = cp_ref(&private, public.path(), cfg.path(), "HEAD");
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "a subdirectory --private is ordinary, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        public.path().join("src/host/statute.py").exists()
            && public.path().join("README.md").exists(),
        "both allowed paths must arrive, relative to --private\n{said}"
    );
    assert!(
        !public.path().join("unrelated.md").exists(),
        "a file outside --private must not be copied\n{said}"
    );
}

/// git's own text is wrapped rather than passed through, so the reader can
/// tell which tool is speaking and what to do about it.
#[test]
fn an_unknown_ref_is_reported_as_pubgrds_error() {
    let private = repository(&[("README.md", "# public\n")]);
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SRC);

    let out = cp_ref(
        private.path(),
        public.path(),
        cfg.path(),
        "v9.9.9-nonexistent",
    );
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(2),
        "an unknown ref is a usage error, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        said.contains("--ref v9.9.9-nonexistent"),
        "the error must name the flag and the ref the user passed\n{said}"
    );
    assert!(
        said.contains("drop --ref"),
        "the error must say what to do, since `cp` works without the flag\n{said}"
    );
}

/// A directory that is not a git worktree is ordinary for `cp` and impossible
/// for `cp --ref`. The message has to say which.
#[test]
fn a_private_tree_that_is_not_a_repository_is_refused_with_advice() {
    let private = tempfile::tempdir().expect("temp dir");
    write(private.path(), &[("README.md", "# public\n")]);
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SRC);

    let out = cp_ref(private.path(), public.path(), cfg.path(), "HEAD");
    let said = said(&out);

    assert_eq!(out.status.code(), Some(2), "{said}");
    assert!(
        said.contains("is not a git worktree, drop --ref"),
        "the error must name the way out\n{said}"
    );
}

/// **The untracked-file bypass.** The check was removed on the reasoning that
/// "an untracked file is never a copy candidate". Candidacy comes from
/// `ls-tree`, though, while the BYTES come from disk — so a path tracked at the
/// ref, renamed away since, and holding an untracked file today is a candidate
/// whose contents git never vouched for. `git diff` says nothing about
/// untracked files, so the guard passed and `cp` published a live key at
/// exit 0.
#[test]
fn an_untracked_file_at_a_tracked_path_blocks_the_copy() {
    let private = repository(&[
        ("src/statute.py", "def cite():\n    pass\n"),
        ("README.md", "# public\n"),
    ]);
    tag(private.path(), "v1");
    git(
        private.path(),
        &["mv", "src/statute.py", "src/statute_v2.py"],
    );
    commit(private.path(), "rename the module");
    write(
        private.path(),
        &[(
            "src/statute.py",
            "PRIVATE_KEY = \"a live key would be here\"\n",
        )],
    );

    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SRC);

    let out = cp_ref(private.path(), public.path(), cfg.path(), "v1");
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(2),
        "an untracked file at a path v1 tracks must refuse the copy, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        !public.path().join("src/statute.py").exists(),
        "nothing may be written when the guard refuses\n{said}"
    );
}

/// **The rename bypass.** `git diff` defaults to reporting a rename as its
/// destination path only, so the SOURCE path — still listed at the ref, and
/// therefore still a copy candidate — vanished from the dirty set.
#[test]
fn the_source_half_of_a_rename_is_dirty() {
    let private = repository(&[
        ("src/statute.py", "def cite():\n    pass\n"),
        ("README.md", "# public\n"),
    ]);
    tag(private.path(), "v1");
    git(private.path(), &["mv", "src/statute.py", "src/moved.py"]);
    commit(private.path(), "rename");

    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SRC);

    let out = cp_ref(private.path(), public.path(), cfg.path(), "v1");
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(2),
        "src/statute.py is tracked at v1 and gone from the worktree, got {:?}\n{said}",
        out.status.code()
    );
    // **On the GUARD's message, not merely on exit 2.** A renamed-away path is
    // also missing from disk, so the copy would fail anyway — which means an
    // exit-code assertion alone passes against the unfixed code for the wrong
    // reason. `--no-renames` is defence in depth here rather than an
    // independently exploitable hole, and this assertion is what says so.
    assert!(
        said.contains("differ from v1 in the worktree"),
        "the GUARD must refuse, before any write is attempted; a copy that fails on a missing \
         file is a different failure wearing the same exit code\n{said}"
    );
}

/// **The `diff.relative` bypass** — a code review's double-prefix-strip reappearing
/// inside the function written to close a code review. Under `diff.relative` git emits
/// paths already relative to `--private`, the prefix subtraction removes a
/// component that is not there, every path falls out of the filter, and the
/// guard reports a clean tree over an edited worktree.
#[test]
fn a_relative_diff_setting_does_not_blind_the_guard() {
    let outer = tempfile::tempdir().expect("temp dir");
    let private = outer.path().join("proj");
    fs::create_dir_all(private.join("src")).expect("create");
    write(
        &private,
        &[("src/a.txt", "clean\n"), ("README.md", "# public\n")],
    );
    git(outer.path(), &["init", "--quiet"]);
    git(outer.path(), &["add", "-A"]);
    commit(outer.path(), "the committed tree");
    git(outer.path(), &["config", "diff.relative", "true"]);
    write(&private, &[("src/a.txt", "SECRET UNCOMMITTED\n")]);

    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SRC);

    let out = cp_ref(&private, public.path(), cfg.path(), "HEAD");
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(2),
        "diff.relative must not blind the dirty check, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        !public.path().join("src/a.txt").exists(),
        "the uncommitted bytes must not reach the public tree\n{said}"
    );
}
