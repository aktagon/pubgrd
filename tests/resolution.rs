//! A code review found this. `init` wrote a configuration the next command could not find.
//!
//! The `init` template design requires `init` to write to `<--private>/pubgrd.toml` and never
//! inside the public tree, because the config is policy and belongs with the
//! repository that decides it. the resolution order looked at
//! `<--public>/pubgrd.toml` and then `./pubgrd.toml`, and at nothing else. The
//! two records were written hours apart, neither cited the other, and the
//! contradiction survived three reviews.
//!
//! Invisible in this project's own use: `make public` runs `--private .` from
//! the repository root, where the `./pubgrd.toml` fallback happens to hit.
//!

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

const CONFIG: &str = r#"
[allow]
paths  = ["src"]
reason = "the published source"
"#;

fn tree(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().expect("create temp tree");
    for (path, contents) in files {
        let full = dir.path().join(path);
        fs::create_dir_all(full.parent().expect("has a parent")).expect("create parent");
        fs::write(full, contents).expect("write fixture file");
    }
    dir
}

/// `cp` with NO `--config`, run from `cwd`, exactly as an operator would after
/// `init`.
fn cp_from(cwd: &Path, private: &Path, public: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pubgrd"))
        .current_dir(cwd)
        .args(["cp", "--private"])
        .arg(private)
        .arg("--public")
        .arg(public)
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

/// The reproduction. A config in `--private`, a cwd that is neither tree.
#[test]
fn a_config_in_the_private_tree_is_found() {
    let private = tree(&[("src/main.rs", "fn main() {}\n"), ("pubgrd.toml", CONFIG)]);
    let public = tempfile::tempdir().expect("temp dir");
    let elsewhere = tempfile::tempdir().expect("temp dir");

    let out = cp_from(elsewhere.path(), private.path(), public.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "the config init writes must be reachable by the command that follows it, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        public.path().join("src/main.rs").exists(),
        "the copy must have run\n{said}"
    );
}

/// Ordering. Policy outranks the tree it governs, so a config in `--private`
/// wins over one in `--public`.
#[test]
fn the_private_tree_outranks_the_public_tree() {
    let private = tree(&[("src/main.rs", "fn main() {}\n"), ("pubgrd.toml", CONFIG)]);
    let public = tree(&[(
        "pubgrd.toml",
        "[allow]\npaths  = [\"nothing-here\"]\nreason = \"the wrong config\"\n",
    )]);
    let elsewhere = tempfile::tempdir().expect("temp dir");

    let out = cp_from(elsewhere.path(), private.path(), public.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "the private tree's config must win, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        public.path().join("src/main.rs").exists(),
        "the private config names src, so src must have been copied\n{said}"
    );
}

/// `verify` has no `--private`, so its resolution is unchanged: the public tree
/// then the working directory.
#[test]
fn verify_still_resolves_from_the_public_tree() {
    let public = tree(&[("src/main.rs", "fn main() {}\n"), ("pubgrd.toml", CONFIG)]);
    let elsewhere = tempfile::tempdir().expect("temp dir");

    let out = Command::new(env!("CARGO_BIN_EXE_pubgrd"))
        .current_dir(elsewhere.path())
        .args(["verify", "--public"])
        .arg(public.path())
        .output()
        .expect("run pubgrd");
    let said = said(&out);

    // `pubgrd.toml` sits in the tree it governs, so it is itself unlisted —
    // exit 1, not 0. What matters here is that the config was FOUND, which a
    // resolution failure (exit 2, "no pubgrd.toml found") would not show.
    assert!(
        said.contains("==> config"),
        "verify must still resolve from the public tree\n{said}"
    );
    assert_ne!(
        out.status.code(),
        Some(2),
        "resolution must not fail\n{said}"
    );
}
