//! ADR-011: `--help` says what the code does, and a test says so rather than a
//! proofreader.
//!
//! A three-lens evaluation on 2026-08-17 found nine defects. Four were false
//! statements about behaviour, and three humans had read the text without
//! catching any of them. Three of those four are pinned here.
//!
//! The regression guards are narrow on purpose. A test asserting "the help is
//! accurate" asserts nothing, so each one names the specific wrong sentence it
//! replaces.

use std::process::Command;

fn help(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_pubgrd"))
        .args(args)
        .output()
        .expect("run pubgrd");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// Clap generates `Print help (see more with '--help')` on its own. A build
/// where the two agree lies in a string nobody wrote.
///
/// Asserted on CONTENT rather than on line count. The first version of this test
/// counted lines and PASSED against the unfixed binary: `--help` renders its two
/// option entries over four lines instead of two, so it was already longer while
/// every authored line was identical. A test that goes green on clap's option
/// layout says nothing about whether anything was held back.
#[test]
fn long_help_holds_detail_that_short_help_does_not() {
    let short = help(&["-h"]);
    let long = help(&["--help"]);

    // The publishable `.env` suffixes are the clearest example of contract
    // detail: a reader needs them before shipping, and not to run the tool.
    assert!(
        long.contains(".env.example"),
        "--help must carry the full built-in deny set, publishable suffixes included\n{long}"
    );
    assert!(
        !short.contains(".env.example"),
        "-h must be a summary; it carried the whole contract, so the two levels were the same \
         text\n{short}"
    );
    assert!(
        short.len() * 2 < long.len(),
        "-h is {} chars against --help's {}; a summary that is nearly the whole thing is not a \
         summary",
        short.len(),
        long.len()
    );
}

/// The help discussed `[allow]` eight times without ever naming the file that
/// holds it, and `--config` said "instead of searching for one" without saying
/// where it searched.
#[test]
fn help_names_the_config_file_and_how_it_is_found() {
    for args in [vec!["-h"], vec!["--help"]] {
        let said = help(&args);
        assert!(
            said.contains("pubgrd.toml"),
            "{args:?} never names the config file\n{said}"
        );
        assert!(
            said.contains("init"),
            "{args:?} must point at `init` as the way to get a documented starter\n{said}"
        );
    }
}

/// Clap takes a doc comment's whole first paragraph as the short description, so
/// the three entries ran 202, 171 and 152 characters and wrapped mid-word at 80
/// columns.
#[test]
fn every_command_table_entry_is_one_clause() {
    let said = help(&["-h"]);
    let table: Vec<&str> = said
        .lines()
        .skip_while(|line| !line.starts_with("Commands:"))
        .skip(1)
        .take_while(|line| line.starts_with("  "))
        .collect();

    // Asserted before the loop. A change to the heading would otherwise leave
    // this test iterating an empty set and reporting ok.
    assert_eq!(
        table.len(),
        4,
        "expected four commands in the table, got {table:?}\n{said}"
    );
    for entry in table {
        assert!(
            entry.len() <= 90,
            "`{}` is {} characters; the table must stay scannable at 80 columns",
            entry.trim(),
            entry.len()
        );
    }
}

/// Every exit code the binary can return is described.
#[test]
fn long_help_names_every_exit_code() {
    let said = help(&["--help"]);
    for code in ["0", "1", "2"] {
        assert!(
            said.contains(&format!("  {code}  ")),
            "exit {code} is not described\n{said}"
        );
    }
}

/// The exit-1 text named three causes, every one phrased about a FILE. An allow
/// ENTRY matching no file also exits 1, and that is the rule FEEDBACK-001 was
/// about. `README.md` carried the same omission.
#[test]
fn the_exit_one_text_covers_an_allow_entry_matching_no_file() {
    let said = help(&["--help"]);
    let exits = said
        .split("Exit codes:")
        .nth(1)
        .expect("the exit-code block is present");
    // The distinguishing word is `entry` on the subject side. "a file matches no
    // [allow] entry" is the OTHER rule, so matching on `entry` alone passes over
    // the defect.
    assert!(
        exits.contains("entry matches no file") || exits.contains("entry matching no file"),
        "exit 1 must say an allow ENTRY matching no file is a violation, not only a file matching \
         no entry\n{exits}"
    );
}

/// Vacuity is decided on configured plus built-in. The help said "a rule with a
/// non-empty configured set examined nothing", which the tool's own
/// `0 configured + 2 built-in ... VIOLATION` line contradicts in the same run.
#[test]
fn the_vacuity_wording_does_not_say_configured_set_alone() {
    let said = help(&["--help"]);
    assert!(
        !said.contains("non-empty configured set"),
        "`non-empty configured set` is contradicted by `0 configured + 2 built-in — VIOLATION`; \
         say `entries to apply` or name both counts\n{said}"
    );
}

/// A successful `init` returns exit 2 deliberately, and "config or usage error"
/// alone makes that look like a failure.
#[test]
fn the_exit_two_text_admits_that_a_successful_init_returns_it() {
    let said = help(&["--help"]);
    let exits = said
        .split("Exit codes:")
        .nth(1)
        .expect("the exit-code block is present");

    assert!(
        exits.contains("init"),
        "exit 2 must say a successful `init` returns it\n{exits}"
    );
}

/// The commonest exit-2 cause was absent from the list.
#[test]
fn the_exit_two_text_names_the_missing_config_case() {
    let said = help(&["--help"]);
    let exits = said
        .split("Exit codes:")
        .nth(1)
        .expect("the exit-code block is present");

    assert!(
        exits.contains("no pubgrd.toml") || exits.contains("no config"),
        "a missing pubgrd.toml is the first failure a new user meets and must be listed\n{exits}"
    );
}

/// ADR-004 says the guard covers "any path it would copy". The help said "any of
/// it is dirty", which reads as everything committed at the ref.
#[test]
fn the_ref_guard_is_described_at_its_real_scope() {
    let said = help(&["cp", "--help"]);
    assert!(
        said.contains("would copy"),
        "the --ref guard covers only paths surviving [allow] and [deny]; ADR-004 says `any path it \
         would copy`\n{said}"
    );
}

/// A tool whose precedence rule needs a paragraph needs a worked invocation.
#[test]
fn help_carries_examples() {
    let said = help(&["-h"]);
    assert!(
        said.contains("Examples:"),
        "the siblings all ship an Examples section\n{said}"
    );
    assert!(
        said.contains("pubgrd verify --public"),
        "the examples must show a real invocation\n{said}"
    );
}
