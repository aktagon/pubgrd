//! Loading `pubgrd.toml`.
//!
//! Two blocks, `[allow]` and `[deny]`, each with `paths` and `reason`.
//! `[allow]` is the mechanism and `[deny]` is a redundant second assertion, so
//! a configuration with no `[allow]` block is a usage error rather than a
//! blacklist-only run. That rule regressed once already, during a
//! simplification pass that made the allow-set optional, and the trivial case
//! it left blacklist-only is the project that had already leaked to a CDN.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use crate::matcher::{Entry, PathSet};

/// The filename every resolution step looks for.
pub const FILENAME: &str = "pubgrd.toml";

/// The placeholder `init` writes. Every later command refuses it.
pub const UNSET_REASON: &str = "TODO";

/// The message a run with no `[allow]` block dies with. Kept as one string
/// because README.md advertises it and a diagnostic that drifts from its
/// documentation is worse than no documentation.
pub const NO_ALLOW_BLOCK: &str = "no [allow] block — comparing a tree against nothing and printing \
     PASS is how a gate becomes decoration. `[deny]` may never be the only \
     check";

/// The message an empty allow-set dies with.
///
/// The block being PRESENT and empty was not refused, only a missing one was,
/// and the gap was reachable: every allow rule reported `skipped (0
/// configured)` and `verify` printed a `FAIL` detail block above a `PASS`
/// summary at exit 0. An earlier design of `init` asserted this case was already handled; it was
/// not.
///
pub const EMPTY_ALLOW_PATHS: &str = "paths is empty. Name the files that may be published — a \
     tree graded against nothing is the empty-set-against-empty-set failure this \
     tool exists to detect, arriving as a configuration value";

/// The conventional deny set: applied on every run, in addition to whatever
/// `[deny]` names, and not overridable.
///
/// Secrets only. A merely unwanted file is already excluded by the whitelist,
/// and being reported as unlisted is the correct recoverable outcome for it;
/// `[deny]` earns its place only where a slip is irreversible. That is the
/// admission test for anything added here, and "most projects would not publish
/// it" is NOT the test.
///
/// `dist` in particular must never join this list: it is `ai-economy`'s allowed
/// directory, and the precedence rule admits no re-admit, so a built-in naming it would
/// permanently break a surveyed consumer with no way out.
///
/// `.env` here stands for the whole family — `.env`, `.env.local`,
/// `.env.production` — minus the template suffixes in
/// [`crate::matcher::ENV_TEMPLATE_SUFFIXES`]. The first version of this constant
/// was matched by component EQUALITY, which passed `.env.local` and
/// `.env.production` because neither is equal to `.env`. Those are exactly where
/// Next.js, Vite and Create React App put live credentials, and both shipped.
///
pub const CONVENTIONAL_DENY: [&str; 2] = [".env", ".envrc"];

/// What a conventional denial says for itself.
///
/// It has to carry three things: that the operator did not write this rule,
/// that there is no override, and what the way out actually is. The four
/// template suffixes are named because the rule's phrasing invites the opposite
/// reading — a reader who has just been told the `.env` family is denied needs
/// telling in the same breath that `.env.example` still publishes.
pub const CONVENTIONAL_REASON: &str = ".envrc and the .env family (.env, .env.local, .env.production, ...) are denied on every \
     run and cannot be re-admitted by [allow]. Remove the file from the public tree, or rename \
     it — .env.example, .env.sample, .env.template and .env.dist are template suffixes and are \
     NOT matched";

/// One configured block.
#[derive(Debug)]
pub struct Block {
    pub paths: PathSet,
    pub reason: String,
}

/// A loaded configuration, and where it came from.
#[derive(Debug)]
pub struct Config {
    pub allow: Block,
    /// The `reason` of the project's `[deny]` block, when one was written.
    ///
    /// Absent means the operator declined the redundant second assertion, which
    /// is legitimate — the four-line `[allow]`-only config README.md advertises
    /// is `ai-economy`'s real one. It does NOT mean nothing is denied: the
    /// conventional set applies regardless, and a violation it produces is
    /// attributed to itself rather than to this reason.
    pub deny_reason: Option<String>,
    /// Every deny entry: the project's, plus the conventional set. This is what
    /// grading uses, and it is never empty, so the deny rules always have a
    /// configured set.
    pub deny: PathSet,
    pub source: PathBuf,
}

/// `pubgrd.toml` as TOML sees it, before any of it is compiled or checked.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawConfig {
    allow: Option<RawBlock>,
    deny: Option<RawBlock>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawBlock {
    /// Spanned, because the requirement needs the line a colliding entry sits on and
    /// `toml` reports byte offsets rather than lines.
    paths: Vec<toml::Spanned<String>>,
    reason: String,
}

/// Find the configuration.
///
/// First hit wins: `--config <PATH>`, then `<private>/pubgrd.toml`, then
/// `<public>/pubgrd.toml`, then `<cwd>/pubgrd.toml`. There is no upward search —
/// `--config` existing is precisely what licenses one, and a tool holding two
/// trees at once has no business guessing which of them a parent directory
/// belongs to.
///
/// `private` outranks `public` because policy outranks the tree it governs, and
/// it is in this list at all because of a code review: the `init` template design requires `init` to write
/// to `<--private>/pubgrd.toml`, a location no resolution step looked at, so the
/// scaffold's output was unreachable from any cwd but the private tree itself.
/// `verify` passes `None` — it has no `--private`.
///
pub fn resolve(
    explicit: Option<&Path>,
    private: Option<&Path>,
    public: Option<&Path>,
    cwd: &Path,
) -> Result<PathBuf> {
    if let Some(path) = explicit {
        if !path.is_file() {
            bail!("--config {} does not exist", path.display());
        }
        return Ok(path.to_path_buf());
    }

    let mut looked = Vec::new();
    for candidate in [
        private.map(|dir| dir.join(FILENAME)),
        public.map(|dir| dir.join(FILENAME)),
        Some(cwd.join(FILENAME)),
    ]
    .into_iter()
    .flatten()
    {
        if candidate.is_file() {
            return Ok(candidate);
        }
        looked.push(candidate.display().to_string());
    }

    bail!(
        "no {FILENAME} found. Looked at {}. There is no upward search; pass --config to name one \
         elsewhere",
        looked.join(", ")
    )
}

/// Report every entry that held glob syntax, would not compile, and was read as
/// a literal instead.
///
/// A warning rather than a refusal, because refusing made ordinary filenames
/// like `notes}v2.md` unusable. It must not be silent, though: a
/// deny entry that quietly stopped being a glob is an under-deny, and there is
/// no `deny.missing` rule to catch one.
///
pub fn warn_fallbacks(config: &Config) {
    for warning in fallback_warnings(config) {
        println!("{warning}");
    }
}

/// The same warnings, returned rather than printed.
///
/// `verify --format json` keeps stdout to the object alone, and these must not
/// be dropped on the way: a deny entry that stopped being a glob is an
/// under-deny, and no rule in the family catches one. Both renderers read this,
/// so neither can carry a warning the other does not.
pub fn fallback_warnings(config: &Config) -> Vec<String> {
    let mut warnings = Vec::new();
    for (block, entries) in [
        ("allow", config.allow.paths.fallbacks()),
        ("deny", config.deny.fallbacks()),
    ] {
        for entry in entries {
            warnings.push(format!(
                "warning: [{block}] `{entry}` holds a character globset treats as syntax but is \
                 not a valid glob, and was read as a literal path"
            ));
        }
    }
    warnings
}

/// Read and compile a configuration.
pub fn load(path: &Path) -> Result<Config> {
    let text =
        std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    parse(&text, path)
}

/// Compile `text` as the configuration at `path`.
fn parse(text: &str, path: &Path) -> Result<Config> {
    let raw: RawConfig =
        toml::from_str(text).with_context(|| format!("parsing {}", path.display()))?;

    let Some(allow) = raw.allow else {
        bail!("{}: {NO_ALLOW_BLOCK}", path.display());
    };

    // Both blocks, then every entry, into ONE list. Collecting per block was
    // what the first implementation did, and it delivered less than the exit contract
    // claims: `[allow]` defects were reported without ever reading `[deny]`, and
    // a malformed entry surfaced only after both `reason` fields were fixed,
    // because entry compilation ran after the block's defect list was already
    // fatal. Editing the the `init` template design template with one typo in it still cost three
    // runs.
    let mut defects = Vec::new();
    let allow = block(allow, "allow", true, text, &mut defects);
    let written = raw
        .deny
        .map(|raw| block(raw, "deny", false, text, &mut defects));

    if !defects.is_empty() {
        let listed = defects
            .iter()
            .map(|defect| format!("  - {defect}"))
            .collect::<Vec<_>>()
            .join("\n");
        bail!("{}:\n{listed}", path.display());
    }

    // The project's entries first, so a collision warning naming "the first
    // matching entry" names one the operator can actually find.
    let (deny_reason, mut entries) = match written {
        Some(block) => (Some(block.reason), block.paths.into_entries()),
        None => (None, Vec::new()),
    };
    entries.extend(
        CONVENTIONAL_DENY
            .iter()
            .map(|name| Entry::conventional(name)),
    );

    Ok(Config {
        allow,
        deny_reason,
        deny: PathSet::from_entries(entries),
        source: path.to_path_buf(),
    })
}

/// Compile one block, collecting every defect before refusing.
///
/// Collected rather than reported one at a time, following the same rule the
/// symlink refusal follows. The the `init` template design template ships with BOTH an
/// empty `paths` and a placeholder `reason`, so a loader that bails on the
/// first makes the very first run fail twice, each failure disclosing one of
/// the two edits the reader needed up front.
///
/// `require_paths` is true only for `[allow]`. An empty `[deny] paths` is
/// equivalent to no `[deny]` block and stays legal; an empty `[allow] paths` is
/// a tree graded against nothing.
///
fn block(
    raw: RawBlock,
    name: &str,
    require_paths: bool,
    text: &str,
    defects: &mut Vec<String>,
) -> Block {
    if require_paths && raw.paths.is_empty() {
        defects.push(format!("[{name}] {EMPTY_ALLOW_PATHS}"));
    }
    if raw.reason.trim() == UNSET_REASON {
        defects.push(format!(
            "[{name}] reason is still \"{UNSET_REASON}\". Write what the block is for — a \
             scaffold that goes green on generation teaches the reader that the gate is satisfied \
             by running a command"
        ));
    } else if raw.reason.trim().is_empty() {
        defects.push(format!(
            "[{name}] reason is empty. A path with no stated reason is a path nobody can audit \
             later"
        ));
    }

    // Every entry is attempted, and a failure becomes a defect rather than an
    // early return, so one run names every bad entry in the file.
    let entries = raw
        .paths
        .into_iter()
        .filter_map(|spanned| {
            let line = line_of(text, spanned.span().start);
            match Entry::parse(spanned.get_ref()) {
                Ok(entry) => Some(entry.at_line(line)),
                Err(error) => {
                    defects.push(format!("[{name}] paths, line {line}: {error:#}"));
                    None
                }
            }
        })
        .collect::<Vec<_>>();

    Block {
        paths: PathSet::from_entries(entries),
        reason: raw.reason,
    }
}

/// The 1-based line holding byte `offset`.
fn line_of(text: &str, offset: usize) -> usize {
    1 + text[..offset.min(text.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::matcher::Origin;
    use std::fs;

    const REAL: &str = r#"
[allow]
paths  = ["index.html", "LICENSE", "README.md", "dist/"]
reason = "the wrangler upload set"

[deny]
paths  = ["**/CLAUDE.md",
          "**/TODO.md"]
reason = "working state and house conventions"
"#;

    fn at(dir: &Path) -> PathBuf {
        dir.join(FILENAME)
    }

    /// The regression this project has already lived through: a simplification
    /// pass made the allow-set optional so a project with no manifest could
    /// still run, which left the trivial case blacklist-only.
    #[test]
    fn a_config_with_no_allow_block_is_refused() {
        let text = "[deny]\npaths = [\"**/CLAUDE.md\"]\nreason = \"house conventions\"\n";

        let error = parse(text, Path::new("pubgrd.toml")).expect_err("must refuse");

        assert!(
            error.to_string().contains("no [allow] block"),
            "the refusal must say which block is missing, got: {error}"
        );
    }

    /// A code review found this. A missing `[allow]` block was refused; an EMPTY one was not, and
    /// the difference was reachable. `paths = []` made every allow rule report
    /// `skipped (0 configured)`, so `verify` printed a `FAIL` detail block and
    /// a `PASS` summary about the same files and exited 0.
    ///
    /// An earlier design of `init` claimed this case was already handled by the exit contract. It was not.
    #[test]
    fn an_empty_allow_paths_is_refused() {
        let text = "[allow]\npaths = []\nreason = \"deliberately written, not a placeholder\"\n";

        let error = parse(text, Path::new("pubgrd.toml")).expect_err("must refuse");

        assert!(
            error.to_string().contains("empty"),
            "the refusal must say the allow-set is empty, got: {error}"
        );
    }

    /// The exit contract: configuration defects are collected, not reported one per run.
    /// The the `init` template design template ships with BOTH an empty `paths` and a placeholder
    /// `reason`, so bailing on the first makes the first run fail twice, each
    /// failure disclosing one of the two edits the user needed up front.
    #[test]
    fn both_scaffold_defects_are_reported_together() {
        let text = "[allow]\npaths = []\nreason = \"TODO\"\n";

        let error = parse(text, Path::new("pubgrd.toml")).expect_err("must refuse");
        let said = error.to_string();

        assert!(
            said.contains("empty") && said.contains(UNSET_REASON),
            "one run must disclose both required edits, got: {said}"
        );
    }

    /// An earlier design of `init`: a scaffold that can go green on generation teaches the reader
    /// that the gate is satisfied by running a command.
    #[test]
    fn the_scaffold_reason_is_refused() {
        let text = "[allow]\npaths = [\"README.md\"]\nreason = \"TODO\"\n";

        let error = parse(text, Path::new("pubgrd.toml")).expect_err("must refuse");

        assert!(
            error.to_string().contains("still \"TODO\""),
            "the refusal must name the placeholder, got: {error}"
        );
    }

    /// A deny block's placeholder is refused too. The scaffold writes both.
    #[test]
    fn the_scaffold_reason_is_refused_in_the_deny_block_as_well() {
        let text = "[allow]\npaths = [\"README.md\"]\nreason = \"the public readme\"\n\n\
                    [deny]\npaths = [\"**/TODO.md\"]\nreason = \"TODO\"\n";

        let error = parse(text, Path::new("pubgrd.toml")).expect_err("must refuse");

        assert!(
            error.to_string().contains("[deny] reason is still"),
            "the refusal must name the block, got: {error}"
        );
    }

    /// An `[allow]`-only config is the advertised trivial case and must load.
    ///
    /// It no longer leaves the deny side EMPTY, though: the conventional
    /// set applies to every run, so the deny rules always have something
    /// configured. What "allow-only" means now is that the operator wrote no
    /// `reason`, and a conventional denial is attributed to itself rather than
    /// borrowing one.
    #[test]
    fn an_allow_only_config_loads_and_carries_only_the_conventional_deny_set() {
        let text = "[allow]\npaths = [\"index.html\", \"dist/\"]\nreason = \"the upload set\"\n";

        let config = parse(text, Path::new("pubgrd.toml")).expect("loads");

        assert_eq!(config.allow.paths.len(), 2);
        assert!(config.deny_reason.is_none(), "no [deny] block was written");
        assert_eq!(config.deny.configured(), 0);
        assert_eq!(config.deny.built_in(), CONVENTIONAL_DENY.len());
    }

    /// `dist` is `ai-economy`'s ALLOWED directory. The precedence rule admits no re-admit,
    /// so a conventional entry naming it would permanently break a surveyed
    /// consumer with no override. This guards the constant itself, not the
    /// matcher.
    #[test]
    fn the_conventional_set_holds_only_credential_paths() {
        for forbidden in ["dist", "build", "target", "docs", "src"] {
            assert!(
                !CONVENTIONAL_DENY.contains(&forbidden),
                "{forbidden} is a path real projects publish; denying it by default is \
                 unappealable under the precedence rule"
            );
        }
    }

    /// the requirement has to name both source lines, so every entry carries one.
    #[test]
    fn every_entry_carries_the_line_it_came_from() {
        let config = parse(REAL, Path::new("pubgrd.toml")).expect("loads");

        // Written entries only. The conventional set is appended after them and
        // carries no line, because it is in no file.
        let lines: Vec<_> = config
            .deny
            .entries()
            .iter()
            .filter(|entry| entry.origin() == Origin::Config)
            .map(Entry::line)
            .collect();

        assert_eq!(
            lines,
            vec![Some(7), Some(8)],
            "a two-line paths array must report both lines"
        );
    }

    /// The exit contract, and the half the first implementation did not deliver: defects
    /// are collected across the WHOLE FILE, not within one block and one phase.
    /// A user editing the the `init` template design template who fills in `paths` with a typo
    /// before replacing `reason` met three exit-2 runs, each disclosing one
    /// requirement — the exact failure the collect rule was written to remove.
    #[test]
    fn defects_from_both_blocks_and_from_entry_compilation_arrive_together() {
        // `"/"` is empty once its trailing slashes are stripped, which is a
        // hard parse error rather than a glob fallback: an empty literal
        // prefixes every path in the tree.
        let text = "[allow]\npaths = [\"/\"]\nreason = \"TODO\"\n\n\
                    [deny]\npaths = [\"x\"]\nreason = \"TODO\"\n";

        let error = parse(text, Path::new("pubgrd.toml")).expect_err("must refuse");
        let said = error.to_string();

        assert!(
            said.contains("[allow] reason is still"),
            "the allow placeholder must be reported, got: {said}"
        );
        assert!(
            said.contains("[deny] reason is still"),
            "the deny placeholder must be reported in the SAME run; bailing after [allow] means              the second surfaces only on the next one, got: {said}"
        );
        assert!(
            said.contains("[allow] paths, line 2"),
            "the unparseable entry must be reported in the SAME run as the two reasons, and \
             located; entry compilation used to run only after the block's defect list was \
             already fatal, got: {said}"
        );
    }

    /// A typo in a key name is a configuration that enforces something other
    /// than what its author read.
    #[test]
    fn an_unknown_key_is_refused() {
        let text = "[allow]\npaths = [\"README.md\"]\nreason = \"the public readme\"\n\
                    note = \"a key nobody implemented\"\n";

        assert!(
            parse(text, Path::new("pubgrd.toml")).is_err(),
            "an otherwise-valid block with an unknown key must be refused; a typo that also \
             trips `missing field` proves nothing"
        );
    }

    #[test]
    fn an_explicit_config_wins_over_the_public_tree() {
        let explicit = tempfile::tempdir().expect("temp dir");
        let public = tempfile::tempdir().expect("temp dir");
        let cwd = tempfile::tempdir().expect("temp dir");
        fs::write(at(explicit.path()), REAL).expect("write");
        fs::write(at(public.path()), REAL).expect("write");

        let found = resolve(
            Some(&at(explicit.path())),
            None,
            Some(public.path()),
            cwd.path(),
        )
        .expect("resolves");

        assert_eq!(found, at(explicit.path()));
    }

    /// A code review found this. `init` writes to `<--private>/pubgrd.toml`, which was absent from
    /// this list entirely, so the scaffold's output was unreachable from any
    /// cwd but the private tree itself. Policy outranks the tree it governs.
    #[test]
    fn the_private_tree_wins_over_the_public_tree() {
        let private = tempfile::tempdir().expect("temp dir");
        let public = tempfile::tempdir().expect("temp dir");
        let cwd = tempfile::tempdir().expect("temp dir");
        fs::write(at(private.path()), REAL).expect("write");
        fs::write(at(public.path()), REAL).expect("write");

        let found =
            resolve(None, Some(private.path()), Some(public.path()), cwd.path()).expect("resolves");

        assert_eq!(found, at(private.path()));
    }

    #[test]
    fn the_public_tree_wins_over_the_working_directory() {
        let public = tempfile::tempdir().expect("temp dir");
        let cwd = tempfile::tempdir().expect("temp dir");
        fs::write(at(public.path()), REAL).expect("write");
        fs::write(at(cwd.path()), REAL).expect("write");

        let found = resolve(None, None, Some(public.path()), cwd.path()).expect("resolves");

        assert_eq!(found, at(public.path()));
    }

    #[test]
    fn the_working_directory_is_the_last_resort() {
        let public = tempfile::tempdir().expect("temp dir");
        let cwd = tempfile::tempdir().expect("temp dir");
        fs::write(at(cwd.path()), REAL).expect("write");

        let found = resolve(None, None, Some(public.path()), cwd.path()).expect("resolves");

        assert_eq!(found, at(cwd.path()));
    }

    /// No upward search. A config one directory up belongs to whichever tree
    /// that directory holds, and `pubgrd` holds two of them.
    #[test]
    fn a_config_in_the_parent_directory_is_not_found() {
        let parent = tempfile::tempdir().expect("temp dir");
        fs::write(at(parent.path()), REAL).expect("write");
        let public = parent.path().join("public");
        let cwd = parent.path().join("private");
        fs::create_dir_all(&public).expect("create");
        fs::create_dir_all(&cwd).expect("create");

        let error = resolve(None, None, Some(&public), &cwd).expect_err("must not search upward");

        assert!(
            error.to_string().contains("no upward search"),
            "the refusal must say why it did not look up, got: {error}"
        );
    }
}
