//! A code review found a gate that printed `FAIL` and `PASS` about the same
//! files, and exited 0.
//!
//! `verify` derived its exit code from the rule functions and its detail blocks
//! from the `Grading` directly, so the two could disagree within one report.
//! With `[allow] paths = []` every rule reported `skipped (0 configured)`,
//! `violations` stayed at zero, and the unlisted-file detail printed under a
//! `FAIL` heading above a `PASS` summary.
//!
//! The first test here pins the specific reachable case. The second pins the
//! CLASS: whatever the configuration, a report that says `FAIL` must not exit
//! 0. Closing only the first leaves the second live for the next author.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

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

fn said(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// The reachable case: an empty allow-set over a non-empty tree.
#[test]
fn an_empty_allow_set_never_reports_a_pass() {
    let public = tree(&[("src/main.rs", "fn main() {}\n"), ("README.md", "# hi\n")]);
    let config = config("[allow]\npaths  = []\nreason = \"deliberately written\"\n");

    let out = verify(public.path(), config.path());
    let said = said(&out);

    assert_ne!(
        out.status.code(),
        Some(0),
        "an empty allow-set graded a two-file tree and reported success:\n{said}"
    );
    assert!(
        !said.contains("PASS"),
        "nothing was compared, so nothing may pass:\n{said}"
    );
}

/// The class. Four configurations, three of which are legitimate; whatever the
/// shape, the word `FAIL` and an exit code of 0 must never co-occur.
#[test]
fn no_report_says_fail_and_exits_zero() {
    let public = tree(&[
        ("src/main.rs", "fn main() {}\n"),
        ("README.md", "# hi\n"),
        ("TODO.md", "private\n"),
    ]);

    let cases = [
        (
            "empty allow-set",
            "[allow]\npaths  = []\nreason = \"written\"\n",
        ),
        (
            "an unlisted file present",
            "[allow]\npaths  = [\"src\"]\nreason = \"written\"\n",
        ),
        (
            "an allow entry matching nothing",
            "[allow]\npaths  = [\"src\", \"README.md\", \"TODO.md\", \"NOTICE.md\"]\nreason = \"written\"\n",
        ),
        (
            "a denied file present",
            "[allow]\npaths  = [\"src\", \"README.md\", \"TODO.md\"]\nreason = \"written\"\n\n\
             [deny]\npaths  = [\"**/TODO.md\"]\nreason = \"working state\"\n",
        ),
    ];

    // Counted, and asserted non-zero after the loop. The assertion used to sit
    // inside `if said.contains("FAIL")`, so a change that renamed the heading,
    // moved detail to stderr, or made `verify` return before `report()` would
    // skip every case and still report ok. An assertion over an observed set
    // passes when the set is empty, and empty is what a regression produces.
    let mut observed = 0;

    for (name, text) in cases {
        let config = config(text);
        let out = verify(public.path(), config.path());
        let said = said(&out);

        if said.contains("FAIL") {
            observed += 1;
            assert_ne!(
                out.status.code(),
                Some(0),
                "case {name:?} printed FAIL and exited 0 — the detail printer and the exit code \
                 disagree:\n{said}"
            );
        }
    }

    // Three, not four: the "empty allow-set" case is now refused at load, and
    // its exit-2 message carries no `FAIL`. Pinned exactly, because a count that
    // drifts up is a case silently changing shape and a count that drifts down
    // is this test going hollow again.
    assert_eq!(
        observed, 3,
        "three of the four cases must actually reach a FAIL report; if none do, this test is \
         asserting nothing at all"
    );
}
