//! The conventional deny set: the always-on deny set, and what the walk prunes.
//!
//! Two defaults with opposite mechanisms, because "ignored" and "denied" are
//! opposites for this tool. An IGNORED `.env` sails through `verify` silently,
//! which is the one job. A DENIED one fails loudly. But `.git` must be ignored,
//! because every public repository legitimately has one, and denying it would
//! report thousands of violations on any real tree.
//!

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

/// The README's advertised four-line config: `[allow]` only, no `[deny]`.
const ALLOW_ONLY: &str = r#"
[allow]
paths  = ["src", "README.md", "dist"]
reason = "the wrangler upload set"
"#;

/// A credential file is denied even when the allow-set admits its directory and
/// the project wrote no `[deny]` block at all.
#[test]
fn a_dotenv_is_denied_with_no_deny_block_written() {
    let public = tree(&[
        ("src/main.rs", "fn main() {}\n"),
        ("src/.env", "API_KEY=leaked\n"),
        ("README.md", "# hi\n"),
    ]);
    let cfg = config(ALLOW_ONLY);

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(1),
        "src/.env is admitted by `src` and must be denied anyway, got {:?}\n{said}",
        out.status.code()
    );
    assert!(said.contains("src/.env"), "the report must name it\n{said}");
}

/// **The attribution rule.** A conventional hit must not print the project's
/// `[deny] reason` — which is the empty string when no block was written — nor
/// a `pubgrd.toml:NN` location for a rule that is not in `pubgrd.toml`. The
/// user greps their config, finds nothing, and has no way out of an
/// unappealable rule announced by nothing at all.
#[test]
fn a_conventional_denial_says_where_it_came_from() {
    let public = tree(&[
        ("src/main.rs", "fn main() {}\n"),
        ("src/.envrc", "export K=1\n"),
    ]);
    let cfg = config("[allow]\npaths  = [\"src\"]\nreason = \"the upload set\"\n");

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    // **The PER-PATH line, not the bare substring.** This asserted
    // `said.contains("built-in")`, which the `deny.present: 0 configured + 2
    // built-in` rule line satisfies on every single run — so deleting the whole
    // attribution mechanism left the test green. It could not fail for the thing
    // it is named after.
    assert!(
        said.contains("src/.envrc — built-in"),
        "the denied PATH must be attributed, not merely the rule line\n{said}"
    );
    assert!(
        said.contains("reason (built-in):"),
        "a rule the operator did not write must explain itself\n{said}"
    );
    assert!(
        !said.contains("reason (deny):"),
        "no [deny] block was written, so nothing may be attributed to one\n{said}"
    );
}

/// The same attribution, in the OTHER block that prints it. `Origin` was
/// applied to `deny.present` and never reached the swallowed-entry report, so
/// one run printed the built-in denial correctly in one place and as
/// `excluded by [deny] `.env`` — the operator's own words — four lines later.
#[test]
fn a_swallowed_allow_entry_says_the_deny_was_built_in() {
    let public = tree(&[("config/.env", "TOKEN=live\n"), ("README.md", "# hi\n")]);
    let cfg =
        config("[allow]\npaths  = [\"config\", \"README.md\"]\nreason = \"the upload set\"\n");

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert!(
        said.contains("excluded by the built-in deny set"),
        "the swallowed-entry block must attribute a built-in rule to itself\n{said}"
    );
    assert!(
        !said.contains("excluded by [deny] `.env`"),
        "attributing a built-in rule to the operator's [deny] block sends them to grep a file \
         that does not contain it\n{said}"
    );
}

/// **The family, not the single name.** `.env.local` and `.env.production` are
/// where Next.js, Vite and Create React App put live credentials. Component
/// EQUALITY passed both, because neither string equals `.env`.
#[test]
fn the_dotenv_family_is_denied_not_only_the_bare_name() {
    for name in [
        ".env.local",
        ".env.production",
        ".env.development",
        ".env.staging",
    ] {
        let public = tree(&[
            ("app/main.js", "// app\n"),
            (&format!("app/{name}"), "DB_PASSWORD=live\n"),
        ]);
        let cfg = config("[allow]\npaths  = [\"app\"]\nreason = \"the app is published\"\n");

        let out = verify(public.path(), cfg.path());
        let said = said(&out);

        assert_eq!(
            out.status.code(),
            Some(1),
            "app/{name} carries live credentials and must be denied, got {:?}\n{said}",
            out.status.code()
        );
        assert!(
            said.contains(&format!("app/{name} — built-in")),
            "the report must name app/{name}\n{said}"
        );
    }
}

/// On a case-insensitive filesystem `.ENVRC` IS `.envrc`, so a case-sensitive
/// test misses a file that is genuinely there. the case rule is about
/// ALLOW entries, where a loose match publishes something; here the argument
/// runs the other way and over-denial is the recoverable direction.
#[test]
fn the_conventional_set_is_case_insensitive() {
    let public = tree(&[
        ("src/main.rs", "fn main() {}\n"),
        ("src/.ENVRC", "export K=1\n"),
    ]);
    let cfg = config("[allow]\npaths  = [\"src\"]\nreason = \"the upload set\"\n");

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(1),
        "`.ENVRC` is the same file as `.envrc` on macOS, got {:?}\n{said}",
        out.status.code()
    );
}

/// `.env.example` is a different path component. A project publishing an
/// example environment file is doing the right thing and must not be blocked.
#[test]
fn the_four_template_suffixes_are_not_denied() {
    for name in [".env.example", ".env.sample", ".env.template", ".env.dist"] {
        let public = tree(&[("README.md", "# hi\n"), (name, "API_KEY=\n")]);
        let cfg = config(&format!(
            "[allow]\npaths  = [\"README.md\", \"{name}\"]\nreason = \"the upload set\"\n"
        ));

        let out = verify(public.path(), cfg.path());
        let said = said(&out);

        assert_eq!(
            out.status.code(),
            Some(0),
            "{name} is a template, not a secret, and publishing one is correct, got {:?}\n{said}",
            out.status.code()
        );
    }
}

/// The family rule must not become a substring rule. `env`, `environment.md`
/// and `src/env.ts` say nothing about credentials, and over-denial here would
/// be unappealable under the precedence rule.
#[test]
fn a_component_that_merely_resembles_dotenv_is_not_denied() {
    let public = tree(&[
        ("env/config.yaml", "k: v\n"),
        ("environment.md", "# how to set up\n"),
        ("src/env.ts", "export const env = {}\n"),
        ("docs/.environment", "notes\n"),
    ]);
    let cfg = config(
        "[allow]\npaths  = [\"env\", \"environment.md\", \"src\", \"docs\"]\nreason = \"the set\"\n",
    );

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "none of these is a member of the .env family, got {:?}\n{said}",
        out.status.code()
    );
}

/// **The regression guard that matters most.** `dist/` is `ai-economy`'s
/// ALLOWED directory, quoted as such in the path-matching rules. The precedence rule admits no re-admit, so
/// a built-in denying it would permanently break a surveyed consumer with no
/// override available. Any future addition to the conventional set has to keep
/// this green.
#[test]
fn dist_is_not_denied_because_a_real_consumer_publishes_it() {
    let public = tree(&[
        ("src/main.rs", "fn main() {}\n"),
        ("README.md", "# hi\n"),
        ("dist/index.html", "<!doctype html>\n"),
    ]);
    let cfg = config(ALLOW_ONLY);

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "ai-economy publishes dist/; denying it by default breaks a real consumer, got {:?}\n{said}",
        out.status.code()
    );
}

/// **The leak the `node_modules` prune cut into the whitelist.** A prune runs in
/// `filter_entry`, before grading, so a pruned path is invisible to `[allow]`
/// and `[deny]` alike — a blacklist upstream of the whitelist. This exact tree
/// printed `pruned whole, not graded: node_modules/` and then `PASS: 1 files,
/// all of them named by [allow]` at exit 0, with a live AWS secret in it. The
/// `PASS` line was false as written: one file had never been shown to a rule.
#[test]
fn a_secret_under_node_modules_is_graded_and_not_pruned_past() {
    let public = tree(&[
        ("README.md", "# hi\n"),
        ("node_modules/some-pkg/.env", "AWS_SECRET=live\n"),
    ]);
    let cfg = config("[allow]\npaths  = [\"README.md\"]\nreason = \"the readme is public\"\n");

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(1),
        "a secret under node_modules must be graded, not walked past, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        said.contains("node_modules/some-pkg/.env"),
        "the report must name the file it found\n{said}"
    );
    assert!(
        !said.contains("pruned whole"),
        "node_modules must no longer be pruned; a prune is a hole in the whitelist\n{said}"
    );
}

/// `.git` stays pruned, and it is the ONLY prune. Every public repository holds
/// one legitimately, which is the test a prune has to pass and the one
/// `node_modules` failed.
#[test]
fn the_git_directory_is_still_pruned_and_named() {
    let public = tree(&[
        (".git/config", "[core]\n"),
        (".git/HEAD", "ref: refs/heads/master\n"),
        ("README.md", "# hi\n"),
    ]);
    let cfg = config("[allow]\npaths  = [\"README.md\"]\nreason = \"the readme is public\"\n");

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(0),
        "a public repository legitimately holds a .git, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        said.contains("pruned whole, not graded: .git/"),
        "a pruned directory must be named, or the filtered count is unreadable\n{said}"
    );
}

/// A regular FILE named `.git` — what a submodule or a linked worktree carries,
/// holding `gitdir:` and an absolute path into the private repository. The prune
/// tested the name and not the type, so it was skipped and then reported with a
/// directory's trailing slash.
#[test]
fn a_git_file_is_graded_rather_than_pruned_as_a_directory() {
    let public = tree(&[
        ("README.md", "# hi\n"),
        (
            ".git",
            "gitdir: /Volumes/T7/private-repo/.git/worktrees/pub\n",
        ),
    ]);
    let cfg = config("[allow]\npaths  = [\"README.md\"]\nreason = \"the readme is public\"\n");

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert_eq!(
        out.status.code(),
        Some(1),
        "a FILE named .git is content and must be graded, got {:?}\n{said}",
        out.status.code()
    );
    assert!(
        !said.contains("pruned whole"),
        "a file is not a directory, and reporting it with a trailing slash asserts one that does \
         not exist\n{said}"
    );
}

/// The rule line has to let a reader reconcile the count with their own file.
/// Seven deny lines reading `9 configured` is a number nobody can check.
#[test]
fn the_rule_line_separates_written_entries_from_built_in_ones() {
    let public = tree(&[("src/main.rs", "fn main() {}\n"), ("README.md", "# hi\n")]);
    let cfg = config(
        "[allow]\npaths  = [\"src\", \"README.md\"]\nreason = \"the set\"\n\n\
         [deny]\npaths  = [\"**/CLAUDE.md\"]\nreason = \"house conventions\"\n",
    );

    let out = verify(public.path(), cfg.path());
    let said = said(&out);

    assert!(
        said.contains("1 configured + 2 built-in"),
        "the deny rule must show both counts separately\n{said}"
    );
}
