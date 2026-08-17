//! What a `[allow] paths` or `[deny] paths` string means.
//!
//! The path-matching rules settles this and the rule is short enough to state here: an entry
//! `globset` treats as holding syntax is a glob, everything else is literal.
//! The set is NOT enumerated, here or in the ADR: see `is_glob`. A literal
//! matches a path it equals, or a path it prefixes followed by `/`. A trailing slash is a habit and is stripped. Matching is
//! case-sensitive on every platform, because a case-insensitive match passes on
//! macOS and leaks on the Linux host that serves the tree.
//!

use anyhow::{Context, Result, bail};
use globset::{GlobBuilder, GlobMatcher};

/// Whether `globset` treats anything in `raw` as syntax.
///
/// **Asked of `globset` rather than listed here, and that is the whole point**
///. This was `const METACHARACTERS: [char; 3] = ['*', '?', '[']` — a
/// hand-written copy of another crate's lexer, and it was missing `{`. So
/// `docs/{adrs,prds}` took the literal branch and compiled to a path no
/// filesystem will ever hold.
///
/// On the allow side that is loud: `allow.missing` reports an entry matching
/// nothing. On the deny side there is no counterpart rule, deliberately
///, so it was a silent no-op in the direction
/// The precedence rule calls
/// irreversible.
///
/// `escape` is `globset`'s own answer to "which characters would I have to
/// neutralise here", so if escaping changes the string, the string held syntax.
/// A list written into a record or a constant drifts from the crate that
/// implements it, which is the same "a count in prose goes stale silently"
/// failure this project already names about its `Makefile` targets.
///
/// **It answers "could this hold syntax", not "is this a glob", and the gap is
/// reachable.** `escape` neutralises `? * [ ] { }` unconditionally, unpaired
/// ones included — and an unpaired `}` is not syntax, it is a character in a
/// filename. So `notes}v2.md`, an ordinary name in a Cookiecutter or Handlebars
/// template tree, took the glob branch, failed to compile, and refused the whole
/// configuration on every command with `unopened alternate group; missing '{'`.
/// [`Entry::parse`] therefore treats a failure to compile as evidence the entry
/// was a literal all along, and records that it did so.
fn is_glob(raw: &str) -> bool {
    globset::escape(raw) != raw
}

/// The `.env.` suffixes that name a template rather than a secret.
///
/// A project publishing one is doing the right thing. Everything
/// else in the family is a credentials file by convention.
///
pub const ENV_TEMPLATE_SUFFIXES: [&str; 4] = ["example", "sample", "template", "dist"];

/// Is this one path component a secret-bearing member of the `.env` family?
///
/// Lowercased first, deliberately against the case-sensitivity rule.
/// That rule exists because a loose ALLOW match publishes something; here the
/// argument inverts, because on a case-insensitive filesystem `.ENVRC` **is**
/// `.envrc` and a case-sensitive test misses a file that is genuinely there.
/// Over-denial is the recoverable direction.
///
fn is_env_secret(component: &str) -> bool {
    let lower = component.to_ascii_lowercase();
    if lower == ".env" {
        return true;
    }
    let Some(suffix) = lower.strip_prefix(".env.") else {
        return false;
    };
    !ENV_TEMPLATE_SUFFIXES.contains(&suffix)
}

/// Where an entry came from, which decides how a violation it produces is
/// attributed.
///
/// Without this, a conventional denial printed the project's `[deny] reason` —
/// the empty string when no block was written — and rendered a `pubgrd.toml:NN`
/// location for a rule that is not in `pubgrd.toml`. Both are false, and the
/// rule has no override, so the reader was handed an unappealable
/// refusal announced by nothing at all.
///
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    /// Written in the configuration file.
    Config,
    /// Applied by `pubgrd` on every run. In no file, and not
    /// overridable.
    ///
    Conventional,
}

/// One compiled `paths` entry.
#[derive(Debug)]
pub struct Entry {
    /// The entry exactly as the configuration wrote it. Diagnostics quote this
    /// rather than the stripped form, so a reader can find the line.
    raw: String,
    /// The 1-based line the entry sits on, when it came from a file. the requirement
    /// requires a collision warning to name both source lines, and `toml`
    /// yields byte offsets rather than line numbers, so the conversion happens
    /// at load time and the answer is carried here.
    line: Option<usize>,
    origin: Origin,
    kind: Kind,
    /// Held glob syntax, would not compile, and was read as a literal.
    fell_back: bool,
}

/// Compile `stripped` as a glob, with the subtree matcher beside it.
fn glob(stripped: &str) -> Result<Kind> {
    let build = |pattern: &str| {
        GlobBuilder::new(pattern)
            .literal_separator(true)
            .build()
            .with_context(|| format!("path entry `{stripped}` is not a valid glob"))
    };
    Ok(Kind::Glob {
        exact: Box::new(build(stripped)?.compile_matcher()),
        subtree: Box::new(build(&format!("{stripped}/**"))?.compile_matcher()),
    })
}

#[derive(Debug)]
enum Kind {
    /// No glob syntax. Matches on equality or on an `entry + "/"` prefix.
    Literal(String),
    /// Compiled with `literal_separator(true)`, so `*` and `?` stop at `/`
    /// while `**` crosses it.
    ///
    /// **Two matchers, because a glob needs the literal branch's subtree
    /// semantics.** `docs` as a literal covers `docs/adrs/001.md`;
    /// `docs/{adrs,prds}` as a glob compiles to `^docs/(adrs|prds)$` and matches
    /// only the DIRECTORY paths, which `walk` never offers a matcher. So the
    /// entry denied nothing, silently, both before and after a code review made `{` a
    /// metacharacter — that fix moved it from one no-op to another. `subtree` is
    /// the same pattern with `/**` appended.
    Glob {
        exact: Box<GlobMatcher>,
        subtree: Box<GlobMatcher>,
    },
    /// A path component name, matched by case-insensitive EQUALITY against each
    /// `/`-separated component. Used only by the conventional deny
    /// set.
    ///
    /// A third mode rather than a glob, because `**/.envrc` under
    /// `literal_separator(true)` matches the file and not a subtree, so "this
    /// name, anywhere, and everything under it" takes two glob entries whose
    /// relationship a reader has to reconstruct.
    ///
    Component(String),
    /// The `.env` family: `.env` itself and every `.env.<suffix>` outside
    /// [`ENV_TEMPLATE_SUFFIXES`].
    ///
    /// A fourth mode rather than a set of `Component` entries, because the
    /// membership test is open-ended: `.env.staging`, `.env.local.old` and
    /// whatever the next framework invents are all credentials, and enumerating
    /// them is the losing side of the trade this project already knows about
    /// from the metacharacter list.
    ///
    EnvFamily,
}

impl Entry {
    /// Compile one entry.
    ///
    /// Fails on an entry that is empty once its trailing slashes are gone: a
    /// literal empty string prefixes every path, so accepting it would let one
    /// stray entry allow the whole tree.
    pub fn parse(raw: &str) -> Result<Self> {
        let stripped = raw.trim_end_matches('/');
        if stripped.is_empty() {
            bail!("empty path entry `{raw}` — an empty entry would match every path in the tree");
        }

        // A compile failure means `is_glob` was over-eager, not that the
        // operator wrote a broken pattern: the only way to reach it is a
        // character `escape` neutralises that does not open a valid construct.
        // Falling back to literal keeps a real filename usable; `fell_back`
        // carries the fact upward so it can be reported rather than swallowed.
        let (kind, fell_back) = match is_glob(stripped).then(|| glob(stripped)) {
            Some(Ok(kind)) => (kind, false),
            Some(Err(_)) => (Kind::Literal(stripped.to_string()), true),
            None => (Kind::Literal(stripped.to_string()), false),
        };

        Ok(Self {
            raw: raw.to_string(),
            line: None,
            origin: Origin::Config,
            kind,
            fell_back,
        })
    }

    /// One entry of the conventional deny set.
    ///
    /// Infallible: the names are this crate's own, not the operator's, so there
    /// is no parse to fail. It carries no line, because it is in no file.
    ///
    pub fn conventional(name: &str) -> Self {
        Self {
            raw: name.to_string(),
            line: None,
            origin: Origin::Conventional,
            // `.env` stands for its whole family; every other conventional name
            // is exactly itself.
            kind: if name == ".env" {
                Kind::EnvFamily
            } else {
                Kind::Component(name.to_string())
            },
            fell_back: false,
        }
    }

    /// Did this entry hold something `globset` treats as syntax, fail to
    /// compile, and get read as a literal instead?
    ///
    /// Reported as a warning rather than swallowed: a deny entry that quietly
    /// stopped being a glob is an under-deny, and the path-matching rules has no
    /// `deny.missing` rule to catch one.
    ///
    pub fn fell_back(&self) -> bool {
        self.fell_back
    }

    /// Where this entry came from, which decides how a violation it produces is
    /// attributed.
    pub fn origin(&self) -> Origin {
        self.origin
    }

    /// Record which line of the configuration this entry came from.
    pub fn at_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }

    /// The entry as written, for diagnostics.
    pub fn raw(&self) -> &str {
        &self.raw
    }

    /// The line this entry came from, when it came from a file.
    pub fn line(&self) -> Option<usize> {
        self.line
    }

    /// Does this entry match `path`?
    ///
    /// `path` is relative to the tree root, with no leading `./` and `/` as the
    /// separator. Directories are never offered here; only files are compared
    /// against the sets.
    pub fn matches(&self, path: &str) -> bool {
        match &self.kind {
            Kind::Literal(literal) => {
                path == literal
                    || path
                        .strip_prefix(literal.as_str())
                        .is_some_and(|rest| rest.starts_with('/'))
            }
            Kind::Glob { exact, subtree } => exact.is_match(path) || subtree.is_match(path),
            Kind::Component(name) => path
                .split('/')
                .any(|component| component.eq_ignore_ascii_case(name)),
            Kind::EnvFamily => path.split('/').any(is_env_secret),
        }
    }
}

/// A whole `paths` list, compiled once.
#[derive(Debug)]
pub struct PathSet {
    entries: Vec<Entry>,
}

impl PathSet {
    /// Compile every entry, reporting the first that will not compile.
    pub fn parse<S: AsRef<str>>(raw: &[S]) -> Result<Self> {
        let entries = raw
            .iter()
            .map(|entry| Entry::parse(entry.as_ref()))
            .collect::<Result<Vec<_>>>()?;
        Ok(Self { entries })
    }

    /// Assemble a set from entries already compiled, so a loader can attach
    /// line numbers before handing them over.
    pub fn from_entries(entries: Vec<Entry>) -> Self {
        Self { entries }
    }

    /// How many entries this set holds, the built-in ones included.
    ///
    /// This is what decides whether a rule runs at all, so it is the total
    /// rather than [`Self::configured`]. A report never prints this number on
    /// its own: `verify` renders `7 configured + 2 built-in`, because a bare `9`
    /// is a count nobody can reconcile against their own file.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// Give up the entries, so a caller can merge two sets into one.
    pub fn into_entries(self) -> Vec<Entry> {
        self.entries
    }

    /// How many entries the operator wrote. Reported separately from the
    /// built-in count so a reader can reconcile the number with their own file
    /// -- seven deny lines reading `9 configured` is a count nobody can check.
    pub fn configured(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.origin() == Origin::Config)
            .count()
    }

    /// How many entries `pubgrd` applied on its own.
    pub fn built_in(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.origin() == Origin::Conventional)
            .count()
    }

    /// Entries that held glob syntax, would not compile, and were read as
    /// literals. Reported so the fallback is never silent.
    pub fn fallbacks(&self) -> Vec<&str> {
        self.entries
            .iter()
            .filter(|entry| entry.fell_back())
            .map(Entry::raw)
            .collect()
    }

    /// Does any entry match `path`?
    pub fn matches(&self, path: &str) -> bool {
        self.entries.iter().any(|entry| entry.matches(path))
    }

    /// The first entry matching `path`, for a diagnostic that has to name the
    /// line responsible.
    pub fn matching(&self, path: &str) -> Option<&Entry> {
        self.entries.iter().find(|entry| entry.matches(path))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matches(entry: &str, path: &str) -> bool {
        Entry::parse(entry).expect("entry compiles").matches(path)
    }

    /// `src`, from a surveyed project's allow-set. The whole question
    /// The path-matching rules was written to answer.
    #[test]
    fn a_bare_directory_matches_itself_and_everything_under_it() {
        assert!(matches("src", "src"));
        assert!(matches("src", "src/host/statute.py"));
    }

    /// The two near-misses the path-matching rules names for `src`. Both are prefix accidents
    /// that a naive `starts_with` would accept.
    #[test]
    fn a_bare_directory_does_not_match_a_lookalike_prefix_or_suffix() {
        assert!(!matches("src", "vendor/src"));
        assert!(!matches("src", "srcfile.rs"));
    }

    /// `Makefile`, from the same allow-set.
    #[test]
    fn a_bare_filename_matches_only_that_file_at_the_root() {
        assert!(matches("Makefile", "Makefile"));
        assert!(!matches("Makefile", "docs/Makefile"));
    }

    /// The third near-miss the path-matching rules names. A deny entry that quietly widened is
    /// the one failure mode this project cannot detect from the outside.
    #[test]
    fn a_bare_filename_does_not_match_the_same_name_in_a_subdirectory() {
        assert!(!matches("README.md", "docs/README.md"));
    }

    /// `dist/`, from the ai-economy allow-set. The slash is a habit.
    #[test]
    fn a_trailing_slash_is_stripped() {
        assert!(matches("dist/", "dist"));
        assert!(matches("dist/", "dist/graph.graphology.json"));
        assert!(!matches("dist/", "vendor/dist"));
    }

    /// A code review found this. `{` was absent from the hand-written metacharacter list, so a
    /// brace entry compiled as a literal naming a directory that cannot exist,
    /// and matched nothing forever. Silent on the deny side, where nothing
    /// reports an entry that never matched.
    #[test]
    fn a_brace_alternation_is_a_glob_and_not_a_literal() {
        let entry = Entry::parse("docs/{adrs,prds}").expect("compiles");

        // **FILE paths, which are the only thing a matcher is ever offered.**
        // The first version of this test asserted `matches("docs/adrs")` — a
        // DIRECTORY path that `walk` discards before grading — so it went green
        // over an entry that still matched nothing real. Making `{` a
        // metacharacter had moved the entry from one no-op to another.
        assert!(
            entry.matches("docs/adrs/001.md"),
            "a glob naming a directory must cover the files under it, as a literal does"
        );
        assert!(
            entry.matches("docs/prds/001-verify.md"),
            "the second alternate must cover its subtree too"
        );
        assert!(
            !entry.matches("docs/handoffs/001.md"),
            "an unnamed sibling must not match"
        );
        assert!(
            !entry.matches("docs/{adrs,prds}"),
            "the entry must not have compiled to a literal naming itself"
        );
    }

    /// The subtree rule, stated on its own rather than only through braces.
    /// A glob and a literal must agree about what naming a directory means, or
    /// the choice of syntax silently changes the semantics.
    #[test]
    fn a_glob_covers_its_subtree_exactly_as_a_literal_does() {
        assert!(matches("doc?", "docs/adrs/001.md"));
        assert!(matches("src/*", "src/host/statute.py"));
        assert!(!matches("doc?", "documents/x.md"));
    }

    /// `escape` neutralises unpaired `}` and `]`, which are not syntax but
    /// characters in a filename — ordinary in a Cookiecutter or Handlebars
    /// template tree. Refusing them made the whole configuration unloadable on
    /// every command, with no way to force the literal reading.
    #[test]
    fn an_entry_that_looks_like_a_glob_but_will_not_compile_is_a_literal() {
        let entry = Entry::parse("notes}v2.md").expect("must not refuse an ordinary filename");

        assert!(
            entry.matches("notes}v2.md"),
            "the literal must match itself"
        );
        assert!(
            entry.fell_back(),
            "the fallback must be recorded, or a deny entry that quietly stopped being a glob is \
             an under-deny nothing reports"
        );
    }

    /// And the fallback keeps the literal branch's subtree semantics, which is
    /// the half that was silently lost rather than loudly refused.
    #[test]
    fn a_fallback_literal_still_covers_its_subtree() {
        let entry = Entry::parse("release}v1").expect("compiles as a literal");

        assert!(entry.matches("release}v1/notes.md"));
        assert!(entry.fell_back());
    }

    /// The set is derived from `globset`, so a character it gains syntax for
    /// later is picked up without editing a list here. This pins the mechanism
    /// rather than the membership.
    #[test]
    fn the_glob_test_is_globsets_own() {
        for literal in ["src", "docs/adrs", "README.md", "Makefile"] {
            assert!(!is_glob(literal), "{literal} holds no glob syntax");
        }
        for glob in [
            "**/CLAUDE.md",
            "*.md",
            "docs/{a,b}",
            "file?.txt",
            "[abc].rs",
        ] {
            assert!(is_glob(glob), "{glob} holds glob syntax");
        }
    }

    /// `**/CLAUDE.md`, from every deny list. `**` crosses `/`.
    #[test]
    fn a_double_star_crosses_separators() {
        assert!(matches("**/CLAUDE.md", "CLAUDE.md"));
        assert!(matches("**/CLAUDE.md", "docs/CLAUDE.md"));
        assert!(matches("**/CLAUDE.md", "docs/adrs/CLAUDE.md"));
    }

    /// `literal_separator(true)`: `*` means "at this level", not "anywhere".
    /// This surprises anyone expecting shell behaviour, and it errs narrow,
    /// which is the direction the path-matching rules chose.
    #[test]
    fn a_single_star_does_not_cross_a_separator() {
        assert!(matches("*.md", "README.md"));
        assert!(!matches("*.md", "docs/README.md"));
    }

    /// macOS resolves a path that differs only in case and Linux does not, so
    /// case-insensitive matching would pass here and leak on the host.
    #[test]
    fn matching_is_case_sensitive() {
        assert!(!matches("README.md", "readme.md"));
        assert!(!matches("**/CLAUDE.md", "docs/claude.md"));
    }

    /// An empty literal prefixes every path in the tree.
    #[test]
    fn an_entry_that_is_only_slashes_is_rejected() {
        assert!(Entry::parse("/").is_err());
        assert!(Entry::parse("").is_err());
    }

    #[test]
    fn a_set_matches_when_any_entry_does_and_names_the_entry() {
        let set = PathSet::parse(&["src", "Makefile"]).expect("set compiles");
        assert_eq!(set.len(), 2);
        assert!(set.matches("src/host/statute.py"));
        assert!(!set.matches("docs/README.md"));
        assert_eq!(
            set.matching("src/host/statute.py").map(Entry::raw),
            Some("src")
        );
    }
}
