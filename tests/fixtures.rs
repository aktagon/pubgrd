//! The two fixtures the design names as reproductions rather than inventions:
//! the 2026-06-07 CDN leak, and an over-denial that swallows an allowed path.
//!
//! Both trees are built at runtime in a `TempDir`. Committing them under
//! `tests/fixtures/` is the obvious alternative and it does not work: a later
//! task needs a `.gitignore` INSIDE a fixture tree, and git would untrack the
//! very file that test depends on.
//!
//! The binary is driven through `env!("CARGO_BIN_EXE_pubgrd")` and
//! `std::process::Command`. `assert_cmd` is the reflex and would be a seventh
//! crate needing an ADR.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

/// Write `files` as a tree in a fresh temporary directory.
fn tree(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().expect("create temp tree");
    for (path, contents) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&full, contents).expect("write fixture file");
    }
    dir
}

/// Write a `pubgrd.toml` in its own directory, OUTSIDE the tree it governs.
///
/// A config inside the public tree is itself an unlisted file, which would add
/// a violation the fixture is not about.
fn config(contents: &str) -> TempDir {
    let dir = tempfile::tempdir().expect("create temp config dir");
    fs::write(dir.path().join("pubgrd.toml"), contents).expect("write config");
    dir
}

fn verify(public: &Path, config_dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pubgrd"))
        .arg("verify")
        .arg("--public")
        .arg(public)
        .arg("--config")
        .arg(config_dir.join("pubgrd.toml"))
        .output()
        .expect("run pubgrd")
}

/// Everything the process said, both streams, for assertion and for the
/// failure message when an assertion does not hold.
fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The 2026-06-07 CDN leak. `ai-economy` publishes six files; `TODO.md` and
/// `ctxgrd.toml` reached the CDN alongside them because a glob sat where an
/// allowlist belonged. The allow-set here names only the six.
#[test]
fn cdn_leak_fails_naming_both_files_that_leaked() {
    let public = tree(&[
        ("index.html", "<!doctype html>\n"),
        ("LICENSE", "Elastic-2.0\n"),
        ("README.md", "# ai-economy\n"),
        ("dist/graph.graphology.json", "{}\n"),
        ("dist/observations.json", "[]\n"),
        ("dist/stack-taxonomy.json", "{}\n"),
        ("TODO.md", "# TODO\n"),
        ("ctxgrd.toml", "[ADR]\n"),
    ]);
    let cfg = config(
        r#"
[allow]
paths  = ["index.html", "LICENSE", "README.md", "dist/"]
reason = "the wrangler upload set; TODO.md and ctxgrd.toml reached the CDN from here on 2026-06-07"
"#,
    );

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        said.contains("TODO.md"),
        "the violation must name TODO.md\n{said}"
    );
    assert!(
        said.contains("ctxgrd.toml"),
        "the violation must name ctxgrd.toml\n{said}"
    );
}

/// Over-denial. The tree CONTAINS `README.md`, the allow-set names it, and a
/// deny of `**/*.md` swallows it. The file has to be there: a fixture whose
/// tree lacks it passes for the wrong reason and demonstrates a missing file
/// rather than an over-denial.
#[test]
fn over_denial_fails_on_the_allow_entry_that_matched_nothing() {
    let public = tree(&[("README.md", "# public\n"), ("LICENSE", "Elastic-2.0\n")]);
    let cfg = config(
        r#"
[allow]
paths  = ["README.md", "LICENSE"]
reason = "the two files this tree is supposed to hold"

[deny]
paths  = ["**/*.md"]
reason = "working state and house conventions"
"#,
    );

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(1),
        "expected exit 1, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        said.contains("README.md"),
        "the violation must name the allow entry that was swallowed\n{said}"
    );
    assert!(
        said.contains("before [deny] and nothing after"),
        "the violation must say the entry matched before [deny] and not after\n{said}"
    );
    assert!(
        said.contains("deny always wins"),
        "a path named by both sets must warn, naming both lines\n{said}"
    );
}

/// The family's own discipline, applied to itself. A configured rule that
/// examined nothing has to fail: a lint that finds nothing must never be
/// comparing two empty sets, and an empty tree is exactly what a regression in
/// the publish step produces.
#[test]
fn an_empty_tree_fails_rather_than_reporting_a_pass() {
    let public = tempfile::tempdir().expect("create empty tree");
    let cfg = config(
        r#"
[allow]
paths  = ["index.html", "dist/"]
reason = "the wrangler upload set"
"#,
    );

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(1),
        "an empty tree must not pass, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        said.contains("a check that cannot fail is not a check"),
        "the failure must say the rule examined nothing\n{said}"
    );
}

/// The other direction. A tool that fails on everything is as useless as one
/// that passes on everything, and only this test tells them apart.
#[test]
fn a_tree_that_matches_its_allow_set_exactly_passes() {
    let public = tree(&[
        ("index.html", "<!doctype html>\n"),
        ("dist/observations.json", "[]\n"),
    ]);
    let cfg = config(
        r#"
[allow]
paths  = ["index.html", "dist/"]
reason = "the wrangler upload set"
"#,
    );

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "a clean tree must pass, got {:?}\n{said}",
        out.status.code()
    );
    assert!(said.contains("PASS"), "a clean tree must say so\n{said}");
}
