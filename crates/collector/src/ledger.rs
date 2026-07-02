//! Cumulative ledger of recurring stuck points across daily reports.
//!
//! Each `reports/nippo-YYYY-MM-DD.md` daily report is an iteration of the same
//! person practicing the same craft. This module
//! parses the structured `## Unclear points` section of those reports (the
//! parser accepts both `##` and `###` depth) —
//! `Issue / Cause / General Fix Rule` triples — accumulates them into
//! `reports/ledger.yaml` with normalized keys, and classifies each rule as
//! new vs. re-seen relative to history.
//!
//! On top of the ledger sits a `StreakTracker` that tracks two streaks over
//! the per-report new-rule counts — a run of zero-new-rule reports and a run
//! of non-decreasing new-rule counts — and emits two signals:
//!
//! - **CONVERGED** after 2 consecutive reports with zero new rules — the
//!   pattern is learned, this class of struggle stopped surfacing.
//! - **DIVERGENCE-SIGNAL** after 3 consecutive reports with a non-decreasing
//!   new-rule count — patching individual symptoms is not working; the
//!   underlying habit or environment needs a structural change.
//!
//! The framing mirrors the natural-language-gradient-descent reading of
//! reflection: each unclear-point bullet is a loss sample, the General Fix
//! Rule is the gradient direction, and the ledger is the integration of
//! those samples over time.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// Convergence threshold: this many *consecutive* zero-new-rule reports stops
/// the streak as `Converged`.
pub(crate) const CONVERGE_STREAK: u32 = 2;
/// Divergence threshold: this many *consecutive* non-decreasing new-rule
/// counts (including the first report that establishes the baseline) emits
/// `Diverged`.
pub(crate) const DIVERGE_STREAK: u32 = 3;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct UnclearPoint {
    pub(crate) issue: String,
    pub(crate) cause: String,
    pub(crate) rule: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ReportEntry {
    /// `reports/nippo-2026-05-11.md` filename (no leading directory).
    pub(crate) report: String,
    /// Inferred date from the filename (YYYY-MM-DD), if any.
    pub(crate) date: Option<String>,
    pub(crate) new_rules: Vec<String>,
    pub(crate) reseen_rules: Vec<String>,
    pub(crate) points: Vec<UnclearPoint>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Ledger {
    /// Reports already folded in, in the order they were processed.
    pub(crate) reports: Vec<ReportEntry>,
    /// Normalized rule keys seen across all reports.
    pub(crate) known_rules: BTreeSet<String>,
}

/// Streak detection over the new-rule counts of consecutive reports. Kept
/// in a small struct so the transition logic can be unit-tested without
/// touching the filesystem.
#[derive(Debug, Default)]
pub(crate) struct StreakTracker {
    zero_streak: u32,
    nondecreasing_streak: u32,
    last_count: Option<usize>,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum Signal {
    Continue,
    Converged,
    Diverged,
}

impl StreakTracker {
    pub(crate) fn observe(&mut self, new_rules: usize) -> Signal {
        if new_rules == 0 {
            self.zero_streak += 1;
        } else {
            self.zero_streak = 0;
        }

        match self.last_count {
            None => {
                self.nondecreasing_streak = if new_rules > 0 { 1 } else { 0 };
            }
            Some(prev) => {
                if new_rules >= prev && new_rules > 0 {
                    self.nondecreasing_streak += 1;
                } else {
                    self.nondecreasing_streak = 0;
                }
            }
        }
        self.last_count = Some(new_rules);

        if self.zero_streak >= CONVERGE_STREAK {
            Signal::Converged
        } else if self.nondecreasing_streak >= DIVERGE_STREAK {
            Signal::Diverged
        } else {
            Signal::Continue
        }
    }
}

/// Heuristic normalization: trim, collapse whitespace, lowercase. Two rules
/// that differ only by spacing or case collapse to the same key.
pub(crate) fn normalize_rule(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Pull the `## Unclear points` section out of a report and parse each
/// `- Issue: / Cause: / General Fix Rule:` triple. `extract_section` looks for
/// the heading at `###` depth first and falls back to `##`, so both depths are
/// accepted. Returns the empty vector when no section is present or the section
/// is the special "なし" sentinel.
pub(crate) fn parse_unclear_points(md: &str) -> Vec<UnclearPoint> {
    let Some(section) = extract_section(md, "Unclear points") else {
        return Vec::new();
    };
    let trimmed = section.trim();
    if trimmed.is_empty() || trimmed == "なし" || trimmed.eq_ignore_ascii_case("none") {
        return Vec::new();
    }

    let mut out = Vec::new();
    let mut cur: Option<UnclearPoint> = None;
    let flush = |cur: &mut Option<UnclearPoint>, out: &mut Vec<UnclearPoint>| {
        if let Some(p) = cur.take()
            && !p.issue.is_empty()
        {
            out.push(p);
        }
    };
    for line in section.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("- Issue:") {
            flush(&mut cur, &mut out);
            cur = Some(UnclearPoint {
                issue: rest.trim().to_string(),
                cause: String::new(),
                rule: String::new(),
            });
        } else if let Some(rest) = trimmed.strip_prefix("Cause:")
            && let Some(p) = cur.as_mut()
        {
            p.cause = rest.trim().to_string();
        } else if let Some(rest) = trimmed
            // `Fix Rule:` / `Rule:` are accepted aliases so hand-written
            // reports that shorten the canonical `General Fix Rule:` still parse.
            .strip_prefix("General Fix Rule:")
            .or_else(|| trimmed.strip_prefix("Fix Rule:"))
            .or_else(|| trimmed.strip_prefix("Rule:"))
            && let Some(p) = cur.as_mut()
        {
            p.rule = rest.trim().to_string();
        }
    }
    flush(&mut cur, &mut out);
    // Drop unfilled template placeholders: an issue wrapped in full-width
    // parens (e.g. `（何が詰まりだったか — 一行で具体的に）`) is the template's
    // fill-me-in prompt, not a real stuck point.
    out.retain(|p| !(p.issue.starts_with('（') && p.issue.ends_with('）')));
    out
}

/// Find the body under `### <heading>` (or `## <heading>`) and return it up
/// to the next heading at the same-or-shallower depth, or end of document.
///
/// The heading match is line-anchored (the prefix must sit at the start of a
/// line) and the terminating scan is fence-aware: a `#` line inside a ```` ```
/// ```` fenced code block does not close the section.
fn extract_section<'a>(md: &'a str, heading: &str) -> Option<&'a str> {
    for depth in [3usize, 2] {
        let prefix = "#".repeat(depth) + " " + heading;
        let Some(start) = find_line_anchored(md, &prefix) else {
            continue;
        };
        let after = &md[start + prefix.len()..];
        let after = after.strip_prefix('\n').unwrap_or(after);
        // End at the next heading of depth <= heading depth, skipping any
        // heading-like line that sits inside a fenced code block.
        let mut end = after.len();
        let mut offset = 0usize;
        let mut in_fence = false;
        for line in after.split_inclusive('\n') {
            let trimmed = line.trim();
            if trimmed.starts_with("```") {
                in_fence = !in_fence;
            } else if !in_fence && trimmed.starts_with('#') {
                let h_depth = trimmed.chars().take_while(|c| *c == '#').count();
                if h_depth <= depth {
                    end = offset;
                    break;
                }
            }
            offset += line.len();
        }
        return Some(&after[..end]);
    }
    None
}

/// Find `needle` where it begins at the start of a line (offset 0 or right
/// after a `\n`). Skips mid-line matches so an inline mention cannot be
/// mistaken for a heading.
fn find_line_anchored(haystack: &str, needle: &str) -> Option<usize> {
    let mut from = 0usize;
    while let Some(rel) = haystack[from..].find(needle) {
        let abs = from + rel;
        if abs == 0 || haystack.as_bytes()[abs - 1] == b'\n' {
            return Some(abs);
        }
        from = abs + 1;
    }
    None
}

/// Classify the points of one report against the cumulative `known` set,
/// returning (new_rule_keys, reseen_rule_keys). Deduplicated and sorted.
/// When a point's `rule` is empty the raw `issue` text is used as the dedup key.
pub(crate) fn classify(
    points: &[UnclearPoint],
    known: &BTreeSet<String>,
) -> (Vec<String>, Vec<String>) {
    let mut new = Vec::new();
    let mut reseen = Vec::new();
    let mut seen_this_report: BTreeSet<String> = BTreeSet::new();
    for p in points {
        let raw = if p.rule.is_empty() { &p.issue } else { &p.rule };
        let key = normalize_rule(raw);
        if key.is_empty() || !seen_this_report.insert(key.clone()) {
            continue;
        }
        if known.contains(&key) {
            reseen.push(key);
        } else {
            new.push(key);
        }
    }
    new.sort();
    reseen.sort();
    (new, reseen)
}

/// Atomic save: write to `<name>.yaml.tmp`, then rename. Prevents a kill
/// mid-write from zeroing out the cumulative `known_rules`.
pub(crate) fn save(path: &Path, ledger: &Ledger) -> Result<()> {
    let yaml = serde_yaml::to_string(ledger).context("serializing ledger")?;
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("creating {}", parent.display()))?;
    }
    let tmp = path.with_extension("yaml.tmp");
    std::fs::write(&tmp, yaml).with_context(|| format!("writing tempfile {}", tmp.display()))?;
    std::fs::rename(&tmp, path)
        .with_context(|| format!("renaming {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

/// Discover `reports/nippo-YYYY-MM-DD.md` daily reports, sorted oldest-first
/// by the date embedded in the filename. Files without a parseable date are
/// skipped — we want a strict chronological order so the `StreakTracker` reads
/// day-N before day-(N+1).
pub(crate) fn discover_reports(reports_dir: &Path) -> Result<Vec<PathBuf>> {
    let mut entries: Vec<(String, PathBuf)> = Vec::new();
    let read = std::fs::read_dir(reports_dir)
        .with_context(|| format!("reading {}", reports_dir.display()))?;
    for ent in read.flatten() {
        let path = ent.path();
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        if !stem.starts_with("nippo-") {
            continue;
        }
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        if let Some(date) = extract_date_from_stem(stem) {
            entries.push((date, path));
        }
    }
    entries.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.file_name().cmp(&b.1.file_name()))
    });
    Ok(entries.into_iter().map(|(_, p)| p).collect())
}

/// `nippo-2026-05-11`, `reflection-2026-05-11-7d` → `Some("2026-05-11")`.
fn extract_date_from_stem(stem: &str) -> Option<String> {
    // Find first ten ASCII chars matching YYYY-MM-DD. Only ASCII positions
    // are considered, so byte-level slicing is safe.
    let bytes = stem.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    for start in 0..=bytes.len() - 10 {
        let slice = &bytes[start..start + 10];
        let looks_like_date = slice.iter().enumerate().all(|(i, b)| match i {
            4 | 7 => *b == b'-',
            _ => b.is_ascii_digit(),
        });
        if looks_like_date {
            return std::str::from_utf8(slice).ok().map(str::to_string);
        }
    }
    None
}

/// End-to-end: scan the reports dir, fold *every* discovered report into the
/// ledger (the streak signal never short-circuits the fold), atomically
/// persist, and return the `Signal` from the final report's observation plus a
/// short human summary line per report.
pub(crate) fn rebuild_from_scratch(
    reports_dir: &Path,
    ledger_path: &Path,
) -> Result<RebuildOutcome> {
    let mut ledger = Ledger::default();
    let mut tracker = StreakTracker::default();
    let mut log = Vec::new();
    let mut last_signal = Signal::Continue;

    for path in discover_reports(reports_dir)? {
        let report_name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("(unnamed)")
            .to_string();
        let text = std::fs::read_to_string(&path)
            .with_context(|| format!("reading {}", path.display()))?;
        let points = parse_unclear_points(&text);
        let (new_rules, reseen_rules) = classify(&points, &ledger.known_rules);
        for k in &new_rules {
            ledger.known_rules.insert(k.clone());
        }
        let date = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(extract_date_from_stem);
        let signal = tracker.observe(new_rules.len());
        last_signal = signal;
        let entry = ReportEntry {
            report: report_name,
            date,
            new_rules,
            reseen_rules,
            points,
        };
        let line = format_log_line(&entry, signal);
        ledger.reports.push(entry);
        log.push(line);
    }

    save(ledger_path, &ledger)?;
    Ok(RebuildOutcome {
        ledger,
        signal: last_signal,
        log,
    })
}

#[derive(Debug)]
pub(crate) struct RebuildOutcome {
    pub(crate) ledger: Ledger,
    pub(crate) signal: Signal,
    pub(crate) log: Vec<String>,
}

fn format_log_line(entry: &ReportEntry, signal: Signal) -> String {
    let sig = match signal {
        Signal::Continue => "..",
        Signal::Converged => "CONVERGED",
        Signal::Diverged => "DIVERGE",
    };
    format!(
        "{:<28} new={:<2} reseen={:<2} [{sig}]",
        entry.report,
        entry.new_rules.len(),
        entry.reseen_rules.len(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_REPORT: &str = r#"# 日報 2026年05月11日

## 今日の作業

- やったこと
- etc

## Unclear points

- Issue: rayon の lifetime エラーに3時間
  Cause: 借用チェッカーの理解が浅い
  General Fix Rule: rayon 並列化前に scope を `move` で閉じる癖を付ける
- Issue: テストが SQLite ロックで間欠失敗
  Cause: 並列実行時に DB ファイル共有
  General Fix Rule: integration tests は `serial_test` で直列化する

## 統計
"#;

    #[test]
    fn parses_structured_unclear_points() {
        let pts = parse_unclear_points(SAMPLE_REPORT);
        assert_eq!(pts.len(), 2);
        assert!(pts[0].issue.contains("rayon"));
        assert!(pts[0].rule.contains("scope"));
        assert!(pts[1].rule.contains("serial_test"));
    }

    #[test]
    fn nashi_sentinel_yields_empty() {
        let md = "## Unclear points\n\nなし\n";
        assert!(parse_unclear_points(md).is_empty());
    }

    #[test]
    fn missing_section_yields_empty() {
        assert!(parse_unclear_points("# 日報\n\nなにもなし\n").is_empty());
    }

    #[test]
    fn classifies_new_and_reseen() {
        let pts = parse_unclear_points(SAMPLE_REPORT);
        let mut known = BTreeSet::new();
        known.insert(normalize_rule(&pts[0].rule));
        let (new, reseen) = classify(&pts, &known);
        assert_eq!(new.len(), 1);
        assert_eq!(reseen.len(), 1);
        assert!(reseen[0].contains("scope"));
    }

    #[test]
    fn dedup_within_one_report() {
        let mut p = parse_unclear_points(SAMPLE_REPORT);
        // Duplicate the first point — should still only count once.
        let dup = p[0].clone();
        p.push(dup);
        let (new, _) = classify(&p, &BTreeSet::new());
        assert_eq!(new.len(), 2);
    }

    #[test]
    fn streak_converges_after_two_zero_iterations() {
        let mut t = StreakTracker::default();
        assert_eq!(t.observe(3), Signal::Continue);
        assert_eq!(t.observe(1), Signal::Continue);
        assert_eq!(t.observe(0), Signal::Continue);
        assert_eq!(t.observe(0), Signal::Converged);
    }

    #[test]
    fn streak_diverges_after_three_nondecreasing() {
        let mut t = StreakTracker::default();
        assert_eq!(t.observe(2), Signal::Continue); // baseline streak=1
        assert_eq!(t.observe(3), Signal::Continue); // streak=2
        assert_eq!(t.observe(3), Signal::Diverged); // streak=3
    }

    #[test]
    fn streak_resets_on_decrease() {
        let mut t = StreakTracker::default();
        assert_eq!(t.observe(5), Signal::Continue);
        assert_eq!(t.observe(6), Signal::Continue);
        assert_eq!(t.observe(2), Signal::Continue);
        assert_eq!(t.observe(3), Signal::Continue);
    }

    #[test]
    fn extract_date_handles_suffix_variants() {
        assert_eq!(
            extract_date_from_stem("nippo-2026-05-11"),
            Some("2026-05-11".into())
        );
        assert_eq!(
            extract_date_from_stem("reflection-2026-05-11-7d"),
            Some("2026-05-11".into())
        );
        assert_eq!(extract_date_from_stem("ledger"), None);
    }

    #[test]
    fn fenced_code_block_does_not_truncate_section() {
        // A code fence containing a `#!/usr/bin/env bash` shebang sits between
        // the two points. If the section scan were not fence-aware it would
        // treat that `#` line as an h1 and cut the second point off.
        let md = "## Unclear points\n\
\n\
- Issue: first issue\n\
  Cause: c1\n\
  General Fix Rule: rule one\n\
\n\
```bash\n\
#!/usr/bin/env bash\n\
echo hi\n\
```\n\
\n\
- Issue: second issue\n\
  Cause: c2\n\
  General Fix Rule: rule two\n\
\n\
## 統計\n";
        let pts = parse_unclear_points(md);
        assert_eq!(pts.len(), 2);
        assert!(pts[1].rule.contains("rule two"));
    }

    #[test]
    fn placeholder_shaped_points_are_dropped() {
        let md = "## Unclear points\n\
\n\
- Issue: （何が詰まりだったか — 一行で具体的に）\n\
  Cause: （なぜ詰まったか — 根本原因を一行で）\n\
  General Fix Rule: （次に同じ状況で使える一般ルール）\n\
\n\
## 統計\n";
        assert!(parse_unclear_points(md).is_empty());
    }

    #[test]
    fn reports_after_convergence_are_still_folded() {
        let dir = tempfile::tempdir().expect("tempdir");
        let reports = dir.path();
        // Day 1 introduces rule A; days 2-3 have no new rules (converges after
        // day 3); day 4 introduces a brand-new rule B *after* convergence.
        let write = |name: &str, body: &str| {
            std::fs::write(reports.join(name), body).expect("write fixture report");
        };
        write(
            "nippo-2026-05-01.md",
            "## Unclear points\n\n- Issue: i-a\n  Cause: c-a\n  General Fix Rule: rule alpha\n\n## 統計\n",
        );
        write("nippo-2026-05-02.md", "## Unclear points\n\nなし\n");
        write("nippo-2026-05-03.md", "## Unclear points\n\nなし\n");
        write(
            "nippo-2026-05-04.md",
            "## Unclear points\n\n- Issue: i-b\n  Cause: c-b\n  General Fix Rule: rule beta\n\n## 統計\n",
        );

        let ledger_path = reports.join("ledger.yaml");
        let outcome = rebuild_from_scratch(reports, &ledger_path).expect("rebuild");

        // All four reports were folded, not stopped at the day-3 convergence.
        assert_eq!(outcome.ledger.reports.len(), 4);
        // The post-convergence rule landed in the cumulative known set.
        assert!(
            outcome
                .ledger
                .known_rules
                .contains(&normalize_rule("rule alpha"))
        );
        assert!(
            outcome
                .ledger
                .known_rules
                .contains(&normalize_rule("rule beta"))
        );
        // Day 4's new rule is recorded on the last report entry.
        assert_eq!(outcome.ledger.reports[3].new_rules.len(), 1);
        assert!(ledger_path.exists());
    }
}
