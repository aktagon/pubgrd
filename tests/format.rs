//! ADR-010: `verify --format json`.
//!
//! FEEDBACK-001 filed this third and asked that it wait for the per-entry
//! counts. Five of an adopting project's assertions broke at once on `match`
//! against `matches` in the verdict sentences, which is what prompted it.
//!
//! These tests parse the object with a small reader rather than a JSON crate.
//! `serde_json` is not a dependency, and a test that pulls one in would be
//! asserting against a parser this crate does not use to produce the output.

use std::fs;
use std::path::Path;
use std::process::{Command, Output};

use tempfile::TempDir;

fn tree(files: &[(&str, &str)]) -> TempDir {
    let dir = tempfile::tempdir().expect("create temp tree");
    for (path, contents) in files {
        let full = dir.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parent");
        }
        fs::write(&full, contents).expect("write fixture file");
    }
    dir
}

fn config(contents: &str) -> TempDir {
    let dir = tempfile::tempdir().expect("create temp config dir");
    fs::write(dir.path().join("pubgrd.toml"), contents).expect("write config");
    dir
}

fn verify(public: &Path, config_dir: &Path, format: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_pubgrd"))
        .arg("verify")
        .arg("--public")
        .arg(public)
        .arg("--config")
        .arg(config_dir.join("pubgrd.toml"))
        .args(format)
        .output()
        .expect("run pubgrd")
}

/// The value of `"key":` as it was written, with no unescaping. Enough to read
/// a number, a bare string or `null` out of a flat position in the object.
fn field<'a>(json: &'a str, key: &str) -> Option<&'a str> {
    let at = json.find(&format!("\"{key}\":"))? + key.len() + 3;
    let rest = json[at..].trim_start();
    let end = if let Some(quoted) = rest.strip_prefix('"') {
        return Some(&quoted[..quoted.find('"')?]);
    } else {
        rest.find([',', '}', ']']).unwrap_or(rest.len())
    };
    Some(rest[..end].trim())
}

/// A tree that passes, so the object is the only thing under test.
fn passing() -> (TempDir, TempDir) {
    let public = tree(&[
        ("scripts/build_index.py", "print('build')\n"),
        ("scripts/render_site.py", "print('render')\n"),
        ("README.md", "# public\n"),
    ]);
    let cfg = config(
        r#"
[allow]
paths  = ["scripts", "README.md"]
reason = "the published subset"
"#,
    );
    (public, cfg)
}

/// stdout is the object and nothing else. A single `==> config` line breaks
/// every parser this flag exists to serve.
#[test]
fn json_mode_writes_nothing_to_stdout_but_the_object() {
    let (public, cfg) = passing();

    let out = verify(public.path(), cfg.path(), &["--format", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.trim_start().starts_with('{') && stdout.trim_end().ends_with('}'),
        "stdout must hold the object alone, got:\n{stdout}"
    );
    assert!(
        !stdout.contains("==>"),
        "the decorative report lines must be suppressed in json mode:\n{stdout}"
    );
    assert!(
        !stdout.contains("PASS"),
        "the English verdict must not be in the json stream:\n{stdout}"
    );
}

/// The exit code the object reports is the exit code the process returns.
/// A caller reading the object and a caller reading `$?` must never disagree.
#[test]
fn the_reported_exit_code_is_the_process_exit_code() {
    let public = tree(&[("README.md", "# public\n"), ("TODO.md", "private\n")]);
    let cfg = config("[allow]\npaths  = [\"README.md\"]\nreason = \"the upload set\"\n");

    let out = verify(public.path(), cfg.path(), &["--format", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert_eq!(
        out.status.code(),
        Some(1),
        "an unlisted file is a violation:\n{stdout}"
    );
    assert_eq!(
        field(&stdout, "exit_code"),
        Some("1"),
        "the object must carry the code the process returned:\n{stdout}"
    );
    assert!(
        stdout.contains("TODO.md"),
        "the object must name the unlisted file, or a consumer parses the prose anyway:\n{stdout}"
    );
}

/// The counts this request waited for. An object without them would report
/// `found: 0` over a tree missing a third of a directory.
#[test]
fn the_object_carries_the_per_entry_counts() {
    let (public, cfg) = passing();

    let out = verify(public.path(), cfg.path(), &["--format", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains("\"coverage\""),
        "coverage is the field this request waited for:\n{stdout}"
    );
    let coverage = &stdout[stdout.find("\"coverage\"").expect("coverage present")..];
    assert!(
        coverage.contains("\"scripts\""),
        "every allow entry must appear:\n{stdout}"
    );
    assert!(
        field(coverage, "matched").is_some(),
        "each entry must carry its count:\n{stdout}"
    );
}

/// Every rule the text report names appears in the object.
///
/// Two renderers exist now, and shared verdicts keep them consistent about
/// outcomes without stopping anyone adding a rule to one and not the other.
#[test]
fn every_rule_in_the_text_report_is_in_the_object() {
    let (public, cfg) = passing();

    let text = verify(public.path(), cfg.path(), &[]);
    let json = verify(public.path(), cfg.path(), &["--format", "json"]);
    let text = String::from_utf8_lossy(&text.stdout);
    let json = String::from_utf8_lossy(&json.stdout);

    let named: Vec<&str> = text
        .lines()
        .filter_map(|line| line.strip_prefix("==> "))
        .filter_map(|line| line.split(':').next())
        // A dotted token with no space in it. The `==> config <path>` line also
        // holds a dot, in `pubgrd.toml`.
        .filter(|name| name.contains('.') && !name.contains(' '))
        .collect();

    // Asserted non-empty first. A change to the report prefix would otherwise
    // leave this loop with nothing to check and report ok.
    assert_eq!(
        named.len(),
        4,
        "the text report must still name four rules, got {named:?}\n{text}"
    );
    for rule in named {
        assert!(
            json.contains(&format!("\"{rule}\"")),
            "rule {rule:?} is in the text report and missing from the object:\n{json}"
        );
    }
}

/// `text` is the default and is byte-identical to passing it explicitly.
#[test]
fn text_is_the_default_format() {
    let (public, cfg) = passing();

    let implied = verify(public.path(), cfg.path(), &[]);
    let explicit = verify(public.path(), cfg.path(), &["--format", "text"]);

    assert_eq!(
        String::from_utf8_lossy(&implied.stdout),
        String::from_utf8_lossy(&explicit.stdout),
        "the default must be text, unchanged"
    );
    assert_eq!(implied.status.code(), explicit.status.code());
}

/// Every field the object carries is named in `README.md`.
///
/// ADR-010 makes the shape a contract, and ADR-011 requires prose about a
/// contract to be tested rather than proofread. `README.md` was the gap ADR-011
/// left open, and it closed in the wrong direction first: the sample documented
/// nine of the ten fields, silently omitting `config`.
///
/// Asserted in the direction that drifts. A field added to the object without a
/// line in the README is the failure; extra prose in the README is not.
#[test]
fn the_readme_names_every_field_the_object_carries() {
    let (public, cfg) = passing();
    let out = verify(public.path(), cfg.path(), &["--format", "json"]);
    let json = String::from_utf8_lossy(&out.stdout);

    let readme = std::fs::read_to_string("README.md").expect("README.md sits at the crate root");

    // Top-level keys only: two spaces of indent, then a quoted name. The nested
    // rule and coverage objects render inline on one line.
    let fields: Vec<&str> = json
        .lines()
        .filter_map(|line| line.strip_prefix("  \""))
        .filter_map(|line| line.split('"').next())
        .collect();

    // Asserted before the loop. A change to the object's indentation would
    // otherwise leave this iterating an empty set and reporting ok.
    assert!(
        fields.len() >= 10,
        "expected the object's top-level fields, found {fields:?}\n{json}"
    );
    for field in fields {
        assert!(
            readme.contains(&format!("\"{field}\"")),
            "the object carries {field:?} and README.md never names it; the JSON shape is a \
             documented contract"
        );
    }
}

/// A path holding a quote, a backslash and a control character.
///
/// Escaping is the one real hazard in writing JSON by hand, and ADR-010 accepts
/// that hazard in exchange for not taking a sixth dependency. This is the test
/// that makes the trade honest.
#[test]
fn a_path_holding_json_syntax_is_escaped() {
    let public = tree(&[("od\"d\\name.md", "# odd\n"), ("README.md", "# public\n")]);
    let cfg = config("[allow]\npaths  = [\"README.md\"]\nreason = \"the upload set\"\n");

    let out = verify(public.path(), cfg.path(), &["--format", "json"]);
    let stdout = String::from_utf8_lossy(&out.stdout);

    assert!(
        stdout.contains(r#"od\"d\\name.md"#),
        "the quote and the backslash must both be escaped:\n{stdout}"
    );
    // Balanced braces are a weak parse and a real one: an unescaped quote ends
    // its string early and the counts stop matching.
    let (open, close) = (stdout.matches('{').count(), stdout.matches('}').count());
    assert_eq!(open, close, "braces must balance:\n{stdout}");
    let quotes = stdout.matches('"').count() - stdout.matches(r#"\""#).count();
    assert_eq!(quotes % 2, 0, "quotes must pair:\n{stdout}");
}
