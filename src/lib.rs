//! `pubgrd` — copy a public repository tree from a private one, and verify
//! nothing else got in.
//!
//! The whole CLI lives here rather than in `src/main.rs`. Integration tests
//! under `tests/` are separate crates and cannot reach a bin-only target, and
//! the unit tests for the matcher, the walk and the config loader all want to
//! call into the crate directly. `src/main.rs` is a three-line shim over
//! [`run`].

pub mod cli;
pub mod config;
pub mod copy;
pub mod git;
pub mod grade;
pub mod init;
pub mod matcher;
pub mod verify;
pub mod walk;

use anyhow::Result;
use clap::Parser;

/// No violations. Warnings may have been printed.
pub const EXIT_OK: i32 = 0;
/// One or more violations.
pub const EXIT_VIOLATION: i32 = 1;
/// A configuration or usage error. There is no exit code for "checked
/// nothing" — that is a violation, not a third kind of success.
pub const EXIT_USAGE: i32 = 2;

/// Run the CLI and return the process exit code.
///
/// The exit contract is fixed by the exit contract and is decided by violations only. A
/// warning never moves it, and every error reaching here is a configuration or
/// usage error, which is exit 2 because the tree was never graded.
pub fn run() -> i32 {
    match dispatch() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("pubgrd: {error:#}");
            EXIT_USAGE
        }
    }
}

fn dispatch() -> Result<i32> {
    let cwd = std::env::current_dir()?;
    match cli::Cli::parse().command {
        cli::Command::Init { private } => init::init(private.as_deref()),
        cli::Command::Verify {
            public,
            config,
            format,
        } => {
            // No `--private` on this command, so nothing to offer as the
            // higher-ranked location.
            let path = config::resolve(config.as_deref(), None, Some(&public), &cwd)?;
            let config = config::load(&path)?;
            verify::verify(&public, &config, format)
        }
        cli::Command::Cp {
            private,
            public,
            config,
            reference,
        } => {
            // A code review found this. This used to pass only `--public` and the cwd, on the
            // reasoning that the documented invocation is `pubgrd cp --private
            // . --public $(OUT)` from the private repository root, "so the
            // working-directory step is the one that finds it". That holds
            // exactly while cwd IS `--private`, which is why the contradiction
            // between the write target and the read order was
            // invisible here and in this project's own `Makefile`. `--private`
            // is now passed, and outranks `--public`.
            let path = config::resolve(config.as_deref(), Some(&private), Some(&public), &cwd)?;
            let config = config::load(&path)?;
            copy::copy(&private, &public, &config, reference.as_deref())
        }
    }
}
