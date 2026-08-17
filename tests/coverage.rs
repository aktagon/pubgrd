//! FEEDBACK-001, reproduced: a directory entry is satisfied by any non-empty
//! subset of itself, and the report does not say which subset.
//!
//! Measured in `Project A` while adopting `pubgrd` as a leakage
//! check. The public remote was four sessions stale. Its `scripts/` held 11
//! files where the private tree builds 13, and `verify` reported
//! `allow.missing: 16 configured, 93 examined, 0 found` — true, and it reads as
//! a stronger claim than it makes. It means every entry matched at least one
//! file. The two entries that matched NOTHING were reported; the entry that
//! matched two thirds of itself was not.
//!
//! These tests pin the count, not the verdict. `pubgrd` cannot know 13 was
//! expected and must not try: that comparison needs the second tree and
//! `verify` reads one. What it can do is print a number a person can compare
//! against the build they just ran.

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

/// The coverage block, parsed back into the pairs it renders.
///
/// Parsed rather than substring-matched, so a test cannot pass on the entry
/// name appearing somewhere else in the report — every one of these names also
/// appears in a `reason` or a violation line.
fn coverage(said: &str) -> Vec<(String, usize)> {
    let mut pairs = Vec::new();
    let mut inside = false;
    for line in said.lines() {
        if line.contains("coverage (") {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if line.trim().is_empty() {
            continue;
        }
        if !line.starts_with("      ") {
            break;
        }
        let mut fields = line.split_whitespace();
        let (Some(entry), Some(matched)) = (fields.next(), fields.next()) else {
            break;
        };
        let Ok(matched) = matched.parse::<usize>() else {
            break;
        };
        pairs.push((entry.to_string(), matched));
    }
    pairs
}

/// The field report, at the size that fits in a fixture. `scripts` is on the
/// allow-set and the tree holds two of the three files it should.
///
/// Every rule passes. That is the finding: the run exits 0, prints `0 found`
/// twice, and nothing in it distinguishes this tree from a complete one.
#[test]
fn a_partly_shipped_directory_reports_how_much_of_itself_it_matched() {
    let public = tree(&[
        ("scripts/build_index.py", "print('build')\n"),
        ("scripts/render_site.py", "print('render')\n"),
        ("README.md", "# public\n"),
    ]);
    let cfg = config(
        r#"
[allow]
paths  = ["scripts", "README.md"]
reason = "the published subset; scripts/ builds three files and this tree holds two"
"#,
    );

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "the tree is a passing one — the count is the whole finding, not a new violation\n{said}"
    );
    assert_eq!(
        coverage(&said),
        vec![("README.md".to_string(), 1), ("scripts".to_string(), 2)],
        "every allow entry must report the number of files it matched, fewest first\n{said}"
    );
}

/// The same tree with the third script restored. Both runs pass and both print
/// `0 found`; only the count tells them apart.
///
/// Asserted as a DIFFERENCE rather than as two independent counts, because the
/// defect is exactly that these two reports were identical where it mattered.
/// `examined` already differed between them, and a reader has no expected value
/// for `examined`.
#[test]
fn a_complete_directory_and_a_partly_shipped_one_report_different_counts() {
    let cfg = config(
        r#"
[allow]
paths  = ["scripts", "README.md"]
reason = "the published subset"
"#,
    );
    let partial = tree(&[
        ("scripts/build_index.py", "print('build')\n"),
        ("scripts/render_site.py", "print('render')\n"),
        ("README.md", "# public\n"),
    ]);
    let complete = tree(&[
        ("scripts/build_index.py", "print('build')\n"),
        ("scripts/render_site.py", "print('render')\n"),
        ("scripts/check_law.py", "print('check')\n"),
        ("README.md", "# public\n"),
    ]);

    let thin = verify(partial.path(), cfg.path());
    let full = verify(complete.path(), cfg.path());
    let (thin, full) = (said(&thin), said(&full));

    let scripts = |said: &str| {
        coverage(said)
            .into_iter()
            .find(|(entry, _)| entry == "scripts")
            .map(|(_, matched)| matched)
    };

    assert_eq!(scripts(&thin), Some(2), "the stale tree holds two\n{thin}");
    assert_eq!(
        scripts(&full),
        Some(3),
        "the built tree holds three\n{full}"
    );
    assert!(
        thin.contains("0 found") && full.contains("0 found"),
        "both runs must still pass every rule — the count is the only thing separating them"
    );
}

/// The count is taken after `[deny]`, not before.
///
/// Counting before would print a healthy number for an entry that the
/// swallowed-entry rule reports, four lines below, as having matched nothing.
/// A report contradicting itself inside one run is the defect the `Verdict`
/// type was introduced to close.
#[test]
fn the_count_is_what_survived_deny() {
    let public = tree(&[
        ("docs/guides/getting-started.md", "# guide\n"),
        ("docs/adrs/001-precedence.md", "# adr\n"),
        ("README.md", "# public\n"),
    ]);
    let cfg = config(
        r#"
[allow]
paths  = ["docs", "README.md"]
reason = "the guide ships and the records do not"

[deny]
paths  = ["docs/adrs"]
reason = "internal records name other repositories"
"#,
    );

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    let docs = coverage(&said)
        .into_iter()
        .find(|(entry, _)| entry == "docs")
        .map(|(_, matched)| matched);

    assert_eq!(
        docs,
        Some(1),
        "`docs` matches two files and [deny] removed one, so it covered one\n{said}"
    );
}

/// Two entries claiming the same file each report it.
///
/// The alternative attributes each file to the first entry that matches, which
/// makes the column sum to the included total. It also makes the column depend
/// on the order the entries were written, and ADR-001 fixes entry order as
/// carrying no meaning. The report says the counts may overlap instead.
#[test]
fn overlapping_entries_each_report_the_file_they_share() {
    let public = tree(&[
        ("scripts/setup-hooks.sh", "#!/bin/sh\n"),
        ("scripts/publish.sh", "#!/bin/sh\n"),
    ]);
    let forwards = config(
        r#"
[allow]
paths  = ["scripts", "scripts/setup-hooks.sh"]
reason = "the directory, and one file inside it named again"
"#,
    );
    let backwards = config(
        r#"
[allow]
paths  = ["scripts/setup-hooks.sh", "scripts"]
reason = "the directory, and one file inside it named again"
"#,
    );

    let one = said(&verify(public.path(), forwards.path()));
    let other = said(&verify(public.path(), backwards.path()));

    let mut expected = vec![
        ("scripts".to_string(), 2),
        ("scripts/setup-hooks.sh".to_string(), 1),
    ];
    expected.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(&right.0)));

    assert_eq!(
        coverage(&one),
        expected,
        "each entry reports what it matches, so the shared file is counted twice\n{one}"
    );
    assert_eq!(
        coverage(&one),
        coverage(&other),
        "writing the same two entries in the other order must not change the column"
    );
}
