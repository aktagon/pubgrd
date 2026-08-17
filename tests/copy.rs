//! `pubgrd cp` — copy fidelity and the one check it runs.
//!
//! Every assertion here comes from the exit contract, which had to state each of these
//! explicitly because `tar`, `cp -R` and a hand-written walk each answer them
//! differently.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn tree(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().expect("create temp tree");
    for (path, contents) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().expect("has a parent")).expect("create parent");
        fs::write(full, contents).expect("write fixture file");
    }
    dir
}

fn config(contents: &str) -> TempDir {
    let dir = tempfile::tempdir().expect("create temp config dir");
    fs::write(dir.path().join("pubgrd.toml"), contents).expect("write config");
    dir
}

fn cp(private: &Path, public: &Path, config_dir: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pubgrd"))
        .args(["cp", "--private"])
        .arg(private)
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

const SCRIPTS: &str = r#"
[allow]
paths  = ["scripts", "README.md"]
reason = "everything needed to read and run the artifact"

[deny]
paths  = ["scripts/setup-hooks.sh"]
reason = "the installer references a directory the public tree does not have"
"#;

/// A code review found this. `verify` refuses to grade an empty set; `cp` was exempt from its own
/// project's founding rule and reported `copied 0 files` at exit 0.
///
/// This is how a code review presented — a double-stripped prefix dropped every tracked
/// path — but the guard is general: a wrong `--private`, an empty tree, and a
/// ref with nothing tracked under the prefix all reach it.
#[test]
fn a_copy_that_selects_no_file_at_all_is_refused() {
    let private = tree(&[("src/main.rs", "fn main() {}\n"), ("README.md", "# hi\n")]);
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(
        "[allow]\npaths  = [\"lib\", \"NOTICE.md\"]\nreason = \"a set naming nothing here\"\n",
    );

    let out = cp(private.path(), public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(2),
        "a copy that selects nothing is not a copy, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        said.contains("--private"),
        "the refusal must name the flag most likely to be wrong\n{said}"
    );
}

/// The other side of a code review, and the reason the guard is TOTAL vacuity rather
/// than per-entry. `NOTICE.md` does not exist until the transform step runs, so
/// an allow entry matching nothing is a legitimate workflow and must stay
/// silent. Only an allow-set where NOTHING matched is an error.
#[test]
fn an_allow_entry_matching_nothing_is_still_tolerated_alongside_one_that_matches() {
    let private = tree(&[("src/main.rs", "fn main() {}\n")]);
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(
        "[allow]\npaths  = [\"src\", \"NOTICE.md\"]\nreason = \"NOTICE.md is generated later\"\n",
    );

    let out = cp(private.path(), public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "a not-yet-generated file must not read as an error, got {:?}\n{said}",
        out.status.code()
    );
    assert!(public.path().join("src/main.rs").exists(), "{said}");
}

/// The quiet one. `scripts/` ships, and a `.sh` arriving without `+x` produces
/// a repository whose documented commands fail for a stranger while every gate
/// stays green.
#[cfg(unix)]
#[test]
fn an_executable_script_arrives_executable() {
    use std::os::unix::fs::PermissionsExt;

    let private = tree(&[
        ("scripts/publish.sh", "#!/bin/sh\necho publishing\n"),
        ("README.md", "# public\n"),
    ]);
    fs::set_permissions(
        private.path().join("scripts/publish.sh"),
        fs::Permissions::from_mode(0o755),
    )
    .expect("mark the source executable");
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SCRIPTS);

    let out = cp(private.path(), public.path(), cfg.path());
    let said = said(&out);

    let mode = fs::metadata(public.path().join("scripts/publish.sh"))
        .expect("the script was copied")
        .permissions()
        .mode();
    assert_eq!(
        mode & 0o111,
        0o111,
        "the executable bit must survive the copy, got {mode:o}\n{said}"
    );
}

/// The precedence rule: deny is applied during the copy, so a denied file is never
/// written. Detecting one afterwards is not equivalent — between write and
/// detection the file exists in a directory something else may be watching.
#[test]
fn a_denied_file_is_never_written() {
    let private = tree(&[
        ("scripts/publish.sh", "#!/bin/sh\n"),
        (
            "scripts/setup-hooks.sh",
            "#!/bin/sh\ngit config core.hooksPath\n",
        ),
        ("README.md", "# public\n"),
    ]);
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SCRIPTS);

    let out = cp(private.path(), public.path(), cfg.path());
    let said = said(&out);

    assert!(
        !public.path().join("scripts/setup-hooks.sh").exists(),
        "a denied file must never reach the public tree\n{said}"
    );
    assert!(
        public.path().join("scripts/publish.sh").exists(),
        "the rest of the directory must still arrive\n{said}"
    );
}

/// A directory exists in the public tree because a file needed it (the path-matching rules,
/// where a directory is never a match target).
#[test]
fn an_empty_directory_is_not_created() {
    let private = tree(&[
        ("scripts/setup-hooks.sh", "#!/bin/sh\n"),
        ("README.md", "# public\n"),
    ]);
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SCRIPTS);

    let out = cp(private.path(), public.path(), cfg.path());
    let said = said(&out);

    assert!(
        !public.path().join("scripts").exists(),
        "a directory whose only file was denied must not be created\n{said}"
    );
}

/// A link's target is a route into the public tree that neither set inspects.
/// Exit 2, a usage error, because the tree was never graded — and nothing is
/// written, so a second run after the fix starts clean.
#[cfg(unix)]
#[test]
fn a_symlink_stops_the_copy_before_anything_is_written() {
    let private = tree(&[
        ("scripts/publish.sh", "#!/bin/sh\n"),
        ("README.md", "# public\n"),
    ]);
    std::os::unix::fs::symlink("publish.sh", private.path().join("scripts/release.sh"))
        .expect("create the link");
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SCRIPTS);

    let out = cp(private.path(), public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(2),
        "a symlink is a usage error, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        said.contains("scripts/release.sh"),
        "the refusal must name the link\n{said}"
    );
    assert!(
        !public.path().join("README.md").exists(),
        "nothing may be written when the copy is refused\n{said}"
    );
}

/// The exit contract, the amendment: `cp` copies, reports, and says to verify the final
/// tree. It never grades its own output.
#[test]
fn cp_says_it_verified_nothing() {
    let private = tree(&[
        ("scripts/publish.sh", "#!/bin/sh\n"),
        ("README.md", "# public\n"),
    ]);
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(SCRIPTS);

    let out = cp(private.path(), public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(out.status.code(), Some(0), "a clean copy exits 0\n{said}");
    assert!(
        said.contains("NOTHING VERIFIED"),
        "cp must say it graded nothing\n{said}"
    );
}

/// The one check `cp` runs. An entry that matched before `[deny]` and nothing
/// after is a config defect. An entry that never matched is NOT reported:
/// `NOTICE.md` is on the allow-set and does not exist until the transforms
/// run, and the two must not read the same.
#[test]
fn an_over_denied_entry_fails_while_a_not_yet_generated_one_does_not() {
    let private = tree(&[("README.md", "# public\n"), ("LICENSE", "Elastic-2.0\n")]);
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(
        r#"
[allow]
paths  = ["README.md", "LICENSE", "NOTICE.md"]
reason = "the published set; NOTICE.md is generated by the transforms"

[deny]
paths  = ["**/*.md"]
reason = "working state and house conventions"
"#,
    );

    let out = cp(private.path(), public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(1),
        "an over-denied entry is a violation, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        said.contains("README.md — on [allow] paths, excluded by [deny]"),
        "the over-denied entry must be named\n{said}"
    );
    assert!(
        !said.contains("NOTICE.md"),
        "an entry that never matched is not this failure, and reporting it here would make a \
         generated file indistinguishable from an over-denial\n{said}"
    );
}

/// The exit contract: `cp` does not remove what it did not put there. Naming those files
/// is what stops the first adoption on an existing public repository reading
/// as the tool malfunctioning.
#[test]
fn a_file_already_in_the_public_tree_is_left_alone_and_named() {
    let private = tree(&[("README.md", "# public\n")]);
    let public = tree(&[("CNAME", "example.invalid\n")]);
    let cfg = config("[allow]\npaths = [\"README.md\"]\nreason = \"the published readme\"\n");

    let out = cp(private.path(), public.path(), cfg.path());
    let said = said(&out);

    assert!(
        public.path().join("CNAME").exists(),
        "cp must not delete what it did not write\n{said}"
    );
    assert!(
        said.contains("CNAME"),
        "cp must name what was already there, or a later verify failure reads as a \
         malfunction\n{said}"
    );
}

/// The arithmetic README.md documents: candidates minus excluded is copied.
/// Found by the T8 dry run, where a 3-file copy out of a repository with a
/// `node_modules/` reported "3 allow entries → 10 candidates" because every
/// deny match in the private tree was counted, including files no [allow]
/// entry ever named. `verify` must report those — they are IN the graded tree.
/// `cp` must not — they were never going to be copied.
#[test]
fn the_copy_report_counts_only_what_the_allow_set_selected() {
    let private = tree(&[
        ("scripts/publish.sh", "#!/bin/sh\n"),
        ("scripts/setup-hooks.sh", "#!/bin/sh\n"),
        ("README.md", "# public\n"),
        // NOT node_modules: the conventional deny set prunes that whole, so a file inside it
        // never reaches grading and this test would pass with the defect
        // reinstated. It needs a deny-matching file the walk actually yields.
        ("vendor/bun-types/CLAUDE.md", "# house conventions\n"),
    ]);
    let public = tempfile::tempdir().expect("temp dir");
    let cfg = config(
        r#"
[allow]
paths  = ["scripts", "README.md"]
reason = "everything needed to read and run the artifact"

[deny]
paths  = ["scripts/setup-hooks.sh", "**/CLAUDE.md"]
reason = "the installer, and house conventions anywhere in the tree"
"#,
    );

    let out = cp(private.path(), public.path(), cfg.path());
    let said = said(&out);

    assert!(
        said.contains("2 allow entries → 3 candidates"),
        "a candidate is a file the allow-set selected, not every deny match in the private \
         tree\n{said}"
    );
    assert!(
        said.contains("deny excluded 1"),
        "only the allow-then-deny collision is an exclusion\n{said}"
    );
    assert!(
        said.contains("copied 2 files"),
        "candidates minus excluded is copied\n{said}"
    );
    assert!(
        !said.contains("vendor"),
        "a file no [allow] entry names was never a candidate and must not be reported as \
         excluded\n{said}"
    );
}
