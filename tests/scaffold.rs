//! `pubgrd init` — the template, and the two refusals that outlived the seeding.
//!
//! The `init` template design replaced this command's behaviour wholesale. The five tests that
//! used to live here exercised `seed()` and the `# CONFIRM:` marker, and both
//! are gone with the mechanism they described — kept in git history rather than
//! as `#[ignore]`, since a test that cannot run is not a test.
//!
//! What is tested instead is that `init` proposes nothing, that its output is
//! inert on both counts, and that the first run discloses both required edits
//! at once rather than one exit code at a time.
//!

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn init(private: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pubgrd"))
        .args(["init", "--private"])
        .arg(private)
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

fn tree(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().expect("create temp tree");
    for (path, contents) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().expect("has a parent")).expect("create parent");
        fs::write(full, contents).expect("write fixture file");
    }
    dir
}

/// A scaffold that can go green on generation teaches the reader that the gate
/// is satisfied by running a command. This survives an earlier design of `init` unchanged.
#[test]
fn init_writes_a_configuration_and_exits_two() {
    let private = tempfile::tempdir().expect("temp dir");

    let out = init(private.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(2),
        "the scaffold must not go green on generation, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        private.path().join("pubgrd.toml").is_file(),
        "the configuration must still be written\n{said}"
    );
}

/// **The whole of the `init` template design in one assertion.** `init` reads no tree, so the
/// allow-set it writes is empty — it cannot widen past a tree it never
/// consulted, and it cannot launder an existing leak into policy.
#[test]
fn init_proposes_no_path_even_with_a_tree_to_look_at() {
    let private = tree(&[
        ("src/main.rs", "fn main() {}\n"),
        ("docs/adrs/001.md", "# adr\n"),
        ("docs/handoffs/001.md", "SECRET\n"),
        (".envrc", "export TOKEN=live\n"),
    ]);

    let out = init(private.path());
    let said = said(&out);
    let written = fs::read_to_string(private.path().join("pubgrd.toml")).expect("written");

    assert!(
        written.contains("paths  = []"),
        "the allow-set must be empty\n{written}"
    );
    // The BLOCK, not the whole file: the template documents `"src"` as a
    // worked example of a literal entry, which is teaching rather than
    // proposing.
    let block = written
        .split("\n[allow]\n")
        .nth(1)
        .expect("the file has an [allow] block");
    for guess in ["\"src\"", "\"docs\"", "\"src/main.rs\"", "\".envrc\""] {
        assert!(
            !block.contains(guess),
            "init proposed {guess}; an allow-set is authored, never generated\n{written}"
        );
    }
    assert!(
        !said.contains("seeded"),
        "nothing was seeded, so nothing may claim to have been\n{said}"
    );
}

/// The first run must disclose BOTH required edits. A message naming only the
/// reason is false once the template ships with an empty `paths`, and a reader
/// who obeys it meets a second exit 2 carrying the second requirement.
#[test]
fn the_first_run_names_both_required_edits() {
    let private = tempfile::tempdir().expect("temp dir");

    let out = init(private.path());
    let said = said(&out);

    assert!(
        said.contains("paths"),
        "the empty allow-set must be named\n{said}"
    );
    assert!(
        said.contains("reason"),
        "the placeholder reason must be named\n{said}"
    );
}

/// And the loader agrees: running a command against the freshly written file
/// reports both defects in one go, rather than one per run.
#[test]
fn the_written_file_is_refused_for_both_reasons_at_once() {
    let private = tempfile::tempdir().expect("temp dir");
    init(private.path());

    let out = Command::new(env!("CARGO_BIN_EXE_pubgrd"))
        .args(["verify", "--public"])
        .arg(private.path())
        .arg("--config")
        .arg(private.path().join("pubgrd.toml"))
        .output()
        .expect("run pubgrd");
    let said = said(&out);

    assert_eq!(out.status.code(), Some(2), "{said}");
    assert!(
        said.contains("empty") && said.contains("TODO"),
        "one run must disclose both required edits\n{said}"
    );
}

/// Regenerating over an edited configuration would discard the `reason` strings
/// that are the file's whole value. Survives an earlier design of `init` unchanged.
#[test]
fn init_refuses_to_overwrite() {
    let private = tempfile::tempdir().expect("temp dir");
    fs::write(
        private.path().join("pubgrd.toml"),
        "[allow]\npaths = [\"src\"]\nreason = \"written by a human\"\n",
    )
    .expect("write an existing config");

    let out = init(private.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(2),
        "overwriting is a usage error, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        said.contains("already exists"),
        "the refusal must name what stopped it\n{said}"
    );
    assert_eq!(
        fs::read_to_string(private.path().join("pubgrd.toml")).expect("still there"),
        "[allow]\npaths = [\"src\"]\nreason = \"written by a human\"\n",
        "the existing configuration must be untouched\n{said}"
    );
}

/// The conventional set is stated in the file the operator will actually open.
/// A default that is invisible in the output is a default nobody can audit.
#[test]
fn the_written_file_names_the_defaults_the_operator_did_not_choose() {
    let private = tempfile::tempdir().expect("temp dir");
    init(private.path());
    let written = fs::read_to_string(private.path().join("pubgrd.toml")).expect("written");

    // `.env` is a prefix of `.envrc`, so testing for each with a bare
    // `contains` is one assertion wearing two names — `.envrc` alone satisfies
    // both. Anchored instead to how the template lays the set out.
    assert!(
        written.contains("#   .envrc "),
        "the always-denied set must name .envrc in its own right\n{written}"
    );
    assert!(
        written.contains("#   .env  "),
        "the always-denied set must name the .env family in its own right\n{written}"
    );
    assert!(
        written.contains(".env.example"),
        "the template must say which .env suffixes are still publishable\n{written}"
    );
    assert!(
        written.contains(".git/"),
        "the one pruned directory must be stated\n{written}"
    );
    assert!(
        written.contains("pubgrd --help"),
        "the template must point at the rest of the surface\n{written}"
    );
}
