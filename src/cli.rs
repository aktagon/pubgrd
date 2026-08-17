//! The command line.
//!
//! Three commands and four flags. Two design reviews cut the surface from six
//! commands and a six-field generated manifest; the reasoning is in the
//! records and the shape is not to be grown back casually.
//!
//! ADR-011 fixes what help must say and how it splits between `-h` and
//! `--help`. Four statements here were false before that record: the exit-1
//! causes were all phrased about a file, "non-empty configured set" was
//! contradicted by the tool's own output, the exit-2 list named five of
//! thirteen causes, and the `--ref` guard was described wider than it is.
//! `tests/help.rs` pins each one, because three humans read the text without
//! catching any of them.

use std::path::PathBuf;

use clap::{Parser, Subcommand, ValueEnum};

/// The summary, shown by `-h`.
///
/// ADR-011: this may not be the same text as [`CONTRACT`]. Clap generates
/// `Print help (see more with '--help')` on its own, so a build where the two
/// agree lies in a string nobody wrote — which is what it did, printing the
/// whole contract at both levels.
const SUMMARY: &str = "\
DENY ALWAYS WINS. A path matching [deny] never ships, however specifically
[allow] names it, and entry order is irrelevant.

Config:
  pubgrd.toml, read from --private, then --public, then the working directory.
  There is no upward search. `pubgrd init` writes a documented starter that
  explains both blocks and the path syntax, which is the fastest way to learn
  the file.

Examples:
  pubgrd init   --private .
  pubgrd cp     --private . --public /tmp/out --ref v1.0.0
  pubgrd verify --public /tmp/out
  pubgrd verify --public /tmp/out --format json

Exit: 0 clean, 1 violation, 2 config or usage error -- and 2 from a SUCCESSFUL
`init`, which is deliberate.

Run --help for the full exit contract, the built-in deny set, and the prune
rule.";

/// The contract, shown by `--help`.
///
/// The precedence rule is stated where a user will actually meet it: precedence
/// documented only in an ADR is precedence nobody reads. The exit codes are
/// [ADR-003](../docs/adrs/003-copy-fidelity-and-the-exit-contract.md)'s, whose
/// own list was incomplete in the same way this one was.
const CONTRACT: &str = "\
DENY ALWAYS WINS. A path matching [deny] is excluded no matter how specifically
[allow] names it. There is no re-admit, and entry order is irrelevant.

Config:
  pubgrd.toml, read from --private, then --public, then the working directory.
  There is no upward search; pass --config to name one elsewhere. `pubgrd init`
  writes a documented starter covering the path syntax, the built-in deny set,
  and a bootstrap loop for adopting pubgrd on a public tree that already
  exists. It is not copied here, because a second copy drifts.

Examples:
  pubgrd init   --private .
  pubgrd cp     --private . --public /tmp/out --ref v1.0.0
  pubgrd verify --public /tmp/out
  pubgrd verify --public /tmp/out --format json

Always denied, listed or not, and not overridable:
  .envrc           credentials, matched in any case
  .env             and the whole family: .env.local, .env.production, and any
                   other .env.<suffix>. Remove or rename the file; there is no
                   override.
                   NOT matched, and safe to publish: .env.example, .env.sample,
                   .env.template, .env.dist.

Always pruned, never graded in either direction:
  .git/            the only one. A prune runs BEFORE grading, so anything
                   pruned is invisible to [allow] and [deny] alike -- which is
                   sound only for a directory every public repository is
                   expected to have.

Exit codes:
  0  the tree is what the config says it is. Warnings may have been printed
  1  a violation, which is any of:
       a file matches no [allow] entry
       an [allow] entry matches no file in the tree
       an [allow] entry matched something before [deny] and nothing after
       a file matches [deny]
       a rule with entries to apply examined nothing. Built-in entries count
       toward that, so deny.present can be vacuous with no [deny] block
  2  config or usage error, and also a successful `init`:
       no pubgrd.toml found, or a --config path that does not exist
       TOML that will not parse, or an unknown key
       a path entry that will not compile, or is empty once slashes are gone
       no [allow] block, or an empty [allow] paths
       a `reason` left at \"TODO\" or blank
       --public or --private naming something that is not a directory
       a symlink in the allow-set, or a cp that selected no file at all
       git absent from PATH, a --ref that does not resolve, or a --ref whose
       worktree has moved away from it
       init refusing to overwrite an existing configuration
       init SUCCEEDING -- a scaffold must not go green on generation, so this
       shares its code with the failures above

There is no exit code for \"checked nothing\". A rule that examined zero files
fails instead of passing, and so does an empty allow-set.";

/// How `verify` renders its report.
///
/// The value names match `wrkgrd` and `trtlgrd`, so one habit works across the
/// family. `text` is the default and stays English: a report should read as a
/// sentence rather than as a form.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, ValueEnum)]
pub enum Format {
    /// The human report.
    #[default]
    Text,
    /// One JSON object on stdout and nothing else.
    Json,
}

#[derive(Parser)]
#[command(
    name = "pubgrd",
    version,
    about = "Copy a public repository tree from a private one, and verify nothing else got in",
    after_long_help = CONTRACT,
    after_help = SUMMARY
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Write a starter configuration, and exit 2
    ///
    /// Reads no tree and proposes no path: an allow-set is authored, never
    /// generated. A generated one would launder an existing leak into policy.
    ///
    /// Exits 2 because the file it writes has an empty `paths` and an unwritten
    /// `reason`, both of which every command refuses. A scaffold that goes
    /// green the moment you generate it teaches you that running a command
    /// satisfies the gate.
    Init {
        /// Where the configuration is written
        ///
        /// Defaults to the working directory. `--public` is not accepted,
        /// because `init` reads no tree at all.
        #[arg(long, value_name = "DIR")]
        private: Option<PathBuf>,
    },
    /// Copy the allow-set from a private tree into a public one
    ///
    /// Does NOT verify its own output. If anything post-processes the tree — a
    /// sed pass, a generated notice, an injected build script — then the tree
    /// `cp` produced is not the tree that ships, and checking it would check
    /// the wrong one. Run `verify` last, over whatever actually ships.
    Cp {
        /// The tree to read from
        ///
        /// A directory, and not necessarily a git repository.
        #[arg(long, value_name = "DIR")]
        private: PathBuf,
        /// The tree to write into
        ///
        /// Created as needed. Never emptied, and existing files are never
        /// deleted.
        #[arg(long, value_name = "DIR")]
        public: PathBuf,
        /// Use this configuration instead of searching for one
        ///
        /// The search order is `--private`, then `--public`, then the working
        /// directory, with no upward search.
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
        /// Copy only what is committed at this ref
        ///
        /// A guard, and it does not change where the bytes come from — those
        /// still come from the filesystem. The allow-set is restricted to paths
        /// the ref tracks, and `cp` then refuses, before writing anything, if
        /// any path it would copy differs from the ref in the worktree.
        /// "Differs" covers modified, staged, deleted and untracked.
        ///
        /// The guard covers the copy set rather than everything the ref tracks,
        /// so a dirty file that `[deny]` excludes does not stop the release.
        #[arg(long = "ref", value_name = "REF")]
        reference: Option<String>,
    },
    /// Verify a tree against its configuration
    ///
    /// Needs no git repository and no private tree: it takes a directory and a
    /// config, because the tree that actually ships is the tree that has to be
    /// checked. The 2026-06-07 incident this tool exists for happened in a
    /// staging directory under /tmp.
    Verify {
        /// The tree to grade
        #[arg(long, value_name = "DIR")]
        public: PathBuf,
        /// Use this configuration instead of searching for one
        ///
        /// The search order is `--public`, then the working directory, with no
        /// upward search. `verify` takes no `--private`.
        #[arg(long, value_name = "PATH")]
        config: Option<PathBuf>,
        /// Output serialisation
        ///
        /// `json` writes one object to stdout and nothing else, so a caller can
        /// assert on the outcome without parsing English. The exit code is the
        /// same either way.
        #[arg(long, value_name = "FORMAT", default_value = "text")]
        format: Format,
    },
}
