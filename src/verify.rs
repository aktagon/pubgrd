//! `pubgrd verify` — is this tree exactly what it was supposed to be?
//!
//! Four rules, named and fixed by the exit contract: `allow.unlisted`, `allow.missing`,
//! `deny.present`, `deny.collision`. Each prints the count it examined, and a
//! rule with entries to apply that examined nothing is a violation rather than a
//! pass. A lint that finds nothing must never be comparing two empty sets.
//!
//! **"Entries to apply", not "a non-empty configured set".** This said the
//! latter, as did `--help` and the 0.1.0 changelog, and it is wrong: vacuity
//! counts built-in entries, so `deny.present` prints
//! `0 configured + 2 built-in, 0 examined — VIOLATION` with no `[deny]` block at
//! all. The sentence contradicted the line it was meant to explain, using the
//! same two words the report prints separately (ADR-011).
//!
//! Warnings never move the exit code. A self-contradicting configuration is a
//! hygiene problem rather than a leak, so the collision warning has to be
//! visible in the default output instead of behind a flag.
//!

use std::path::Path;

use anyhow::Result;

use crate::cli::Format;
use crate::config::{CONVENTIONAL_REASON, Config};
use crate::grade::{Grading, grade};
use crate::matcher::{Entry, Origin};
use crate::{EXIT_OK, EXIT_VIOLATION, walk};

/// Verify `public` against `config`, printing the report. Returns the exit
/// code, which is decided by violations only.
pub fn verify(public: &Path, config: &Config, format: Format) -> Result<i32> {
    let tree = walk::walk(public)?;
    let graded = grade(&tree.files, &config.allow.paths, Some(&config.deny));

    let examined = tree.files.len();
    let allow_entries = config.allow.paths.len();

    // Every rule's outcome is decided HERE, once, before either renderer runs.
    // ADR-010 requires it and this project has already paid for the
    // alternative: `verify` once took its exit code from the rule functions and
    // its detail from the `Grading`, and printed `FAIL` and `PASS` about the
    // same files at exit 0. Two renderers reading one set of verdicts cannot
    // reproduce that.
    let rules = [
        Rule::new(
            "allow.unlisted",
            allow_entries,
            0,
            examined,
            graded.unlisted.len(),
        ),
        Rule::new(
            "allow.missing",
            allow_entries,
            0,
            examined,
            graded.unmatched.len(),
        ),
        Rule::new(
            "deny.present",
            config.deny.configured(),
            config.deny.built_in(),
            examined,
            graded.denied.len(),
        ),
    ];
    // The collision rule reports what it found and contributes nothing: the requirement
    // is a warning, and a warning never moves the exit code. It is held apart
    // from the three above rather than filtered out of them, so nothing can sum
    // its verdict into the exit code by accident. It also renders through the
    // warning-only printer: the shared one announced `VIOLATION: a check that
    // cannot fail is not a check` for a rule whose verdict is deliberately
    // dropped, so `verify` over an empty tree printed FOUR `VIOLATION` lines
    // above `FAIL: 3 violations` — four accusations, a count matching none of
    // them, and no detail block naming a file.
    let collision = Rule::new(
        "deny.collision",
        config.deny.configured(),
        config.deny.built_in(),
        examined,
        graded.collisions.len(),
    );

    let violations: usize = rules.iter().map(|rule| rule.verdict.violations()).sum();
    let code = if violations == 0 {
        EXIT_OK
    } else {
        EXIT_VIOLATION
    };

    match format {
        Format::Text => text(
            config, &tree, &graded, &rules, &collision, examined, violations,
        ),
        Format::Json => json(config, &graded, &rules, &collision, examined, code),
    }
    Ok(code)
}

/// One rule as both renderers see it.
struct Rule {
    name: &'static str,
    /// Entries the operator wrote.
    configured: usize,
    /// Entries `pubgrd` applied on its own.
    built_in: usize,
    /// Files this rule was offered. The same number for every rule in one run,
    /// carried per rule so a renderer never has to reach outside the value.
    examined: usize,
    verdict: Verdict,
}

impl Rule {
    fn new(
        name: &'static str,
        configured: usize,
        built_in: usize,
        examined: usize,
        found: usize,
    ) -> Self {
        Self {
            name,
            configured,
            built_in,
            examined,
            verdict: Verdict::of(configured + built_in, examined, found),
        }
    }

    /// `7 configured + 2 built-in`, never a bare `9`. A count a reader cannot
    /// reconcile against their own file is worse than no count. The allow rules
    /// have no built-in entries, so they render the short form.
    fn counts(&self) -> String {
        if self.built_in == 0 {
            format!("{} configured", self.configured)
        } else {
            format!(
                "{} configured + {} built-in",
                self.configured, self.built_in
            )
        }
    }
}

/// The English report, unchanged.
fn text(
    config: &Config,
    tree: &walk::Tree,
    graded: &Grading,
    rules: &[Rule; 3],
    collision: &Rule,
    examined: usize,
    violations: usize,
) {
    println!("==> config {}", config.source.display());
    crate::config::warn_fallbacks(config);
    println!("==> {}", tree.summary());

    for rule in rules {
        print_rule(rule);
    }
    print_rule_warning(collision);

    coverage(graded, examined);
    report(
        graded,
        config,
        rules[0].verdict,
        rules[1].verdict,
        rules[2].verdict,
    );

    if violations == 0 {
        println!("\nPASS: {examined} files, all of them named by [allow]");
        return;
    }
    println!(
        "\nFAIL: {violations} {}",
        plural(violations, "violation", "violations")
    );
}

/// One rule's outcome. **Both the exit code and the detail block are derived
/// from this value**, which is the point of it existing.
///
/// Before, the exit code came from these rule functions while `report` read the
/// `Grading` directly, so the two had independent control flow and could
/// disagree inside one report — and did: with an empty allow-set every rule
/// reported `skipped` while `report` printed `FAIL: N files match no [allow]
/// entry` above `PASS: N files, all of them named by [allow]`, at exit 0.
///
/// The empty allow-set is now refused at load, which closes the one
/// reachable case. This type closes the class, so the next rule added cannot
/// reopen it.
///
#[derive(Clone, Copy, Debug)]
enum Verdict {
    /// Nothing configured. The rule did not run and has nothing to say.
    Skipped,
    /// Configured, but examined nothing. A check that cannot fail.
    Vacuous,
    /// Ran over a non-empty tree and found this many.
    Found(usize),
}

impl Verdict {
    /// Decide one rule's outcome.
    ///
    /// A rule with no entries at all is skipped: the four-line `[allow]`-only
    /// config README.md advertises is a real one, and treating an unconfigured
    /// rule as vacuous would make it exit 1 forever. The vacuity rule bites the
    /// other case, entries to apply and nothing to apply them to.
    ///
    /// `configured` here is the TOTAL — what the operator wrote plus the
    /// built-in set — which is why the deny rules are never skipped in practice.
    /// Naming the parameter after only the first half is what made three
    /// documents describe this wrongly (ADR-011).
    fn of(configured: usize, examined: usize, found: usize) -> Self {
        if configured == 0 {
            Verdict::Skipped
        } else if examined == 0 {
            Verdict::Vacuous
        } else {
            Verdict::Found(found)
        }
    }

    /// The word the JSON object carries for this outcome.
    fn outcome(self) -> &'static str {
        match self {
            Verdict::Skipped => "skipped",
            Verdict::Vacuous => "vacuous",
            Verdict::Found(_) => "ran",
        }
    }

    /// What this contributes to the exit code.
    fn violations(self) -> usize {
        match self {
            Verdict::Skipped => 0,
            Verdict::Vacuous => 1,
            Verdict::Found(found) => found,
        }
    }

    /// Whether this rule's detail block prints.
    ///
    /// A rule that did not run has nothing to list, and a rule that lists
    /// something moves the exit code by exactly the count it listed. That
    /// equivalence is what makes `FAIL` at exit 0 unrepresentable.
    fn detailed(self) -> bool {
        matches!(self, Verdict::Found(found) if found > 0)
    }
}

/// Print one rule's line.
fn print_rule(rule: &Rule) {
    let (name, counts, examined) = (rule.name, rule.counts(), rule.examined);
    match rule.verdict {
        Verdict::Skipped => println!("==> {name}: skipped (0 configured)"),
        Verdict::Vacuous => println!(
            "==> {name}: {counts}, 0 examined — VIOLATION: a check that cannot fail is not a check"
        ),
        Verdict::Found(found) => {
            println!("==> {name}: {counts}, {examined} examined, {found} found")
        }
    }
}

/// As `print_rule`, for a rule that never moves the exit code.
///
/// It must not print the word `VIOLATION`. A reader who sees it stops reading
/// the number, which is the same reasoning the exit contract uses to forbid a `FAIL` line
/// at exit 0 — and here the word appeared for a rule whose verdict is thrown
/// away, so the count below could never account for it.
fn print_rule_warning(rule: &Rule) {
    let (name, counts, examined) = (rule.name, rule.counts(), rule.examined);
    match rule.verdict {
        Verdict::Skipped => println!("==> {name}: skipped (0 configured)"),
        Verdict::Vacuous => {
            println!("==> {name}: {counts}, 0 examined — warning: nothing to compare")
        }
        Verdict::Found(found) => {
            println!("==> {name}: {counts}, {examined} examined, {found} found")
        }
    }
}

/// The schema version the JSON object carries.
///
/// ADR-010: a consumer can refuse an object it does not understand instead of
/// misreading one. Bump this when a field is renamed or removed.
const SCHEMA: u32 = 1;

/// One JSON object on stdout and nothing else.
///
/// ADR-010. The `==> config` line, the walk summary and the fallback warnings
/// are all `println!` in the text renderer, and a stray line breaks every
/// parser this flag exists to serve. The warnings are carried in the object
/// rather than dropped.
///
/// `exit_code` is the value the process returns, passed in rather than
/// recomputed. A caller reading the object and a caller reading `$?` must never
/// disagree.
fn json(
    config: &Config,
    graded: &Grading,
    rules: &[Rule; 3],
    collision: &Rule,
    examined: usize,
    exit_code: i32,
) {
    let rule = |rule: &Rule| {
        let found = match rule.verdict {
            Verdict::Found(found) => found.to_string(),
            // A rule that did not run has no count. Reporting `0` would say it
            // looked and found nothing, which is the reading this whole tool
            // exists to refuse.
            _ => "null".to_string(),
        };
        format!(
            "{{\"name\": {}, \"configured\": {}, \"built_in\": {}, \"examined\": {}, \
             \"outcome\": {}, \"found\": {}}}",
            quoted(rule.name),
            rule.configured,
            rule.built_in,
            rule.examined,
            quoted(rule.verdict.outcome()),
            found
        )
    };

    let mut all: Vec<String> = rules.iter().map(&rule).collect();
    all.push(rule(collision));

    let coverage: Vec<String> = graded
        .coverage
        .iter()
        .map(|covered| {
            format!(
                "{{\"entry\": {}, \"matched\": {}}}",
                quoted(covered.entry.raw()),
                covered.matched
            )
        })
        .collect();

    let unlisted: Vec<String> = graded.unlisted.iter().map(|path| quoted(path)).collect();

    let denied: Vec<String> = graded
        .denied
        .iter()
        .map(|path| {
            let origin = match config.deny.matching(path).map(Entry::origin) {
                Some(Origin::Conventional) => "built-in",
                _ => "config",
            };
            format!(
                "{{\"path\": {}, \"origin\": {}}}",
                quoted(path),
                quoted(origin)
            )
        })
        .collect();

    let unmatched: Vec<String> = graded
        .unmatched
        .iter()
        .map(|unmatched| {
            let swallowed = match unmatched.swallowed_by {
                Some(deny) => quoted(deny.raw()),
                None => "null".to_string(),
            };
            format!(
                "{{\"entry\": {}, \"swallowed_by\": {}}}",
                quoted(unmatched.entry.raw()),
                swallowed
            )
        })
        .collect();

    let collisions: Vec<String> = graded
        .collisions
        .iter()
        .map(|collision| {
            format!(
                "{{\"path\": {}, \"allow\": {}, \"deny\": {}}}",
                quoted(collision.path),
                quoted(collision.allow.raw()),
                quoted(collision.deny.raw())
            )
        })
        .collect();

    let warnings: Vec<String> = crate::config::fallback_warnings(config)
        .iter()
        .map(|warning| quoted(warning))
        .collect();

    println!("{{");
    println!("  \"version\": {SCHEMA},");
    println!("  \"exit_code\": {exit_code},");
    println!(
        "  \"config\": {},",
        quoted(&config.source.display().to_string())
    );
    println!("  \"examined\": {examined},");
    println!("  \"rules\": {},", array(&all));
    println!("  \"coverage\": {},", array(&coverage));
    println!("  \"unlisted\": {},", array(&unlisted));
    println!("  \"denied\": {},", array(&denied));
    println!("  \"unmatched\": {},", array(&unmatched));
    println!("  \"collisions\": {},", array(&collisions));
    println!("  \"warnings\": {}", array(&warnings));
    println!("}}");
}

/// A JSON array of already-rendered values, one per line when non-empty.
fn array(values: &[String]) -> String {
    if values.is_empty() {
        return "[]".to_string();
    }
    format!("[\n    {}\n  ]", values.join(",\n    "))
}

/// A JSON string, escaped per RFC 8259 section 7.
///
/// Hand-written because `serde_json` would be a sixth dependency, which
/// `Cargo.toml` says needs an ADR saying why the standard library is
/// insufficient. ADR-010 accepts that trade and names this as the one real
/// hazard in it: a path holding a quote ends its string early, and every count
/// after it shifts. Filenames hold quotes, backslashes and newlines on every
/// platform this runs on.
fn quoted(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 2);
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0c}' => out.push_str("\\f"),
            // Everything below 0x20 is forbidden raw, and the named escapes
            // above cover only five of them.
            control if (control as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// How many paths a detail block lists before it stops and says how many more
/// there are.
///
/// This exists because report volume was previously answered by not grading —
/// the `node_modules` prune in the conventional deny set, which bought a readable report with a
/// hole in the whitelist. Truncating output is the cheap half of that trade.
/// The count in the `FAIL` heading is always the true total, so the exit code
/// never depends on this number.
///
pub const DETAIL_CAP: usize = 20;

/// Print at most [`DETAIL_CAP`] of `paths`, then say how many were withheld.
fn capped<T: std::fmt::Display>(paths: &[T], render: impl Fn(&T) -> String) {
    for path in paths.iter().take(DETAIL_CAP) {
        println!("{}", render(path));
    }
    if let Some(more) = paths.len().checked_sub(DETAIL_CAP).filter(|more| *more > 0) {
        println!("  ... and {more} more (the count above is the whole of it)");
    }
}

/// How many files each `[allow]` entry claimed.
///
/// **Prints on a passing run, which no other detail block does.** That is the
/// whole point of it. `allow.missing: 0 found` means every entry matched at
/// least one file, and a reader takes it to mean the tree is what it should be.
/// A directory entry is satisfied by any non-empty subset of itself, so a tree
/// missing two of thirteen scripts passes here in silence. FEEDBACK-001
/// measured that tree.
///
/// The number is for a person to compare against a build they just ran.
/// `pubgrd` cannot make the comparison itself: the expected count lives in the
/// second tree, and `verify` reads one. Clone the published tree and `diff -rq`
/// it against your build when you want that answer.
///
/// Nothing fails on this number, so it must not be rendered like a verdict. A
/// tick beside `scripts 11` would repeat the defect it exists to report.
fn coverage(graded: &Grading, examined: usize) {
    let mut entries: Vec<(&str, usize)> = graded
        .coverage
        .iter()
        .map(|covered| (covered.entry.raw(), covered.matched))
        .collect();
    if entries.is_empty() {
        return;
    }
    // Fewest first, so a [`DETAIL_CAP`] truncation drops the large counts. An
    // entry matching nothing or almost nothing is the one worth reading. Ties
    // break on the name, so the column does not depend on entry order.
    entries.sort_by(|left, right| left.1.cmp(&right.1).then(left.0.cmp(right.0)));

    println!(
        "\n    coverage (files matched per [allow] entry; entries may overlap, so these need not \
         sum to {examined})\n"
    );
    let shown = entries.len().min(DETAIL_CAP);
    let width = entries[..shown]
        .iter()
        .map(|(raw, _)| raw.len())
        .max()
        .unwrap_or(0);
    for (raw, matched) in &entries[..shown] {
        println!("      {raw:<width$}  {matched}");
    }
    if let Some(more) = entries
        .len()
        .checked_sub(DETAIL_CAP)
        .filter(|more| *more > 0)
    {
        println!("      ... and {more} more, every one matching more files than those above");
    }
}

/// The detail under the rule lines. Every violation carries the `reason` of
/// the block that produced it: a path with no stated reason is a
/// path nobody can audit later.
fn report(
    graded: &Grading,
    config: &Config,
    unlisted: Verdict,
    missing: Verdict,
    present: Verdict,
) {
    if unlisted.detailed() {
        println!(
            "\nFAIL: {} {}",
            graded.unlisted.len(),
            plural(
                graded.unlisted.len(),
                "file matches no [allow] entry",
                "files match no [allow] entry"
            )
        );
        println!();
        capped(&graded.unlisted, |path| format!("  {path}"));
        println!("      reason (allow): {:?}", config.allow.reason);
    }

    if present.detailed() {
        println!(
            "\nFAIL: {} {}",
            graded.denied.len(),
            plural(
                graded.denied.len(),
                "file matches [deny]",
                "files match [deny]"
            )
        );
        println!();
        // Attributed per path. A conventional denial must not borrow the
        // project's `reason` — which is the empty string when no `[deny]` block
        // was written — nor point at a `pubgrd.toml` line that does not hold the
        // rule. The reader greps their config, finds nothing, and cannot appeal
        // a rule the precedence rule gives no override for.
        let mut conventional = false;
        let mut configured = false;
        let mut lines = Vec::with_capacity(graded.denied.len());
        for path in &graded.denied {
            match config.deny.matching(path).map(Entry::origin) {
                Some(Origin::Conventional) => {
                    conventional = true;
                    lines.push(format!("  {path} — built-in"));
                }
                _ => {
                    configured = true;
                    lines.push(format!("  {path}"));
                }
            }
        }
        capped(&lines, Clone::clone);
        if configured && let Some(reason) = &config.deny_reason {
            println!("      reason (deny): {reason:?}");
        }
        if conventional {
            println!("      reason (built-in): {CONVENTIONAL_REASON}");
        }
    }

    // Both halves of this partition belong to `allow.missing`, so both gate on
    // its verdict rather than on the partition being non-empty.
    let (swallowed, absent): (Vec<_>, Vec<_>) = if missing.detailed() {
        graded
            .unmatched
            .iter()
            .partition(|unmatched| unmatched.swallowed_by.is_some())
    } else {
        (Vec::new(), Vec::new())
    };

    if !swallowed.is_empty() {
        println!(
            "\nFAIL: {} allow {} matched something before [deny] and nothing after",
            swallowed.len(),
            plural(swallowed.len(), "entry", "entries")
        );
        println!();
        // Attributed, exactly like the `deny.present` block above. `Origin` was
        // introduced for that block and never reached this one, so the same
        // report was right in one place and wrong four lines later: a built-in
        // denial printed here as `excluded by [deny] `.env`` — the operator's
        // own words — and then attached `deny_reason`, which is None for the
        // allow-only config README.md advertises. The reader greps their file
        // for `.env`, finds no `[deny]` block at all, and holds an unappealable
        // refusal announced by nothing.
        let mut conventional = false;
        let mut configured = false;
        for unmatched in &swallowed {
            let deny = unmatched.swallowed_by.expect("partitioned on Some");
            match deny.origin() {
                Origin::Conventional => {
                    conventional = true;
                    println!(
                        "  {} — on [allow] paths, excluded by the built-in deny set (`{}`)",
                        unmatched.entry.raw(),
                        deny.raw()
                    );
                }
                Origin::Config => {
                    configured = true;
                    println!(
                        "  {} — on [allow] paths, excluded by [deny] `{}`",
                        unmatched.entry.raw(),
                        deny.raw()
                    );
                }
            }
        }
        if configured && let Some(reason) = &config.deny_reason {
            println!("      reason (deny): {reason:?}");
        }
        if conventional {
            println!("      reason (built-in): {CONVENTIONAL_REASON}");
        }
    }

    if !absent.is_empty() {
        println!(
            "\nFAIL: {} allow {}",
            absent.len(),
            plural(
                absent.len(),
                "entry matches no file in the tree",
                "entries match no file in the tree"
            )
        );
        println!();
        let lines: Vec<String> = absent
            .iter()
            .map(|unmatched| format!("  {}", unmatched.entry.raw()))
            .collect();
        capped(&lines, Clone::clone);
        println!("      reason (allow): {:?}", config.allow.reason);
    }

    // Last, and not a failure. the requirement names both source lines so the reader
    // can find the two entries that disagree.
    for collision in &graded.collisions {
        println!(
            "\nwarning: {} is named by [allow] {} and by [deny] {} — excluded, because deny \
             always wins",
            collision.path,
            at(
                &config.source,
                collision.allow.line(),
                collision.allow.origin()
            ),
            at(
                &config.source,
                collision.deny.line(),
                collision.deny.origin()
            ),
        );
    }
}

/// English, so a report reads as a sentence rather than as a form.
fn plural(count: usize, one: &'static str, many: &'static str) -> &'static str {
    if count == 1 { one } else { many }
}

/// `pubgrd.toml:12`, or just the filename when the entry carries no line.
fn at(source: &Path, line: Option<usize>, origin: Origin) -> String {
    match (origin, line) {
        // A built-in entry is in no file. Rendering `(pubgrd.toml)` for it sends
        // the reader to grep a config that does not contain the rule.
        (Origin::Conventional, _) => "(built-in)".to_string(),
        (Origin::Config, Some(line)) => format!("({}:{line})", source.display()),
        (Origin::Config, None) => format!("({})", source.display()),
    }
}
