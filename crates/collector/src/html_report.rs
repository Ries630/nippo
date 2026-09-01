//! Markdown レポートを自己完結 HTML へ変換する。

use anyhow::{Context, Result, bail};
use clap::ValueEnum;
use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Eq, PartialEq)]
struct ReportSubsection {
    heading: String,
    fields: Vec<(String, String)>,
}

/// HTML 表示へ変換できるレポートモード。
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum HtmlReportMode {
    /// ALACT に基づく期間の振り返り。
    Insight,
    /// 三期間の変化分析。
    Trend,
    /// 評価面談・自己評価向けレポート。
    Review,
}

/// Markdown ファイルを同じ stem の自己完結 HTML として保存する。
pub(crate) fn render_file(mode: HtmlReportMode, input: &Path) -> Result<PathBuf> {
    let markdown = fs::read_to_string(input)
        .with_context(|| format!("failed to read Markdown report: {}", input.display()))?;
    let output = input.with_extension("html");
    let document = render_document(mode, &markdown);
    write_atomically(&output, document.as_bytes())?;
    Ok(output)
}

fn render_document(mode: HtmlReportMode, markdown: &str) -> String {
    let title = document_title(markdown).unwrap_or("nippo report");
    let body = render_markdown(markdown);
    let visualizations = render_visualizations(mode, markdown);

    format!(
        r#"<!doctype html>
<html lang="ja">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; style-src 'unsafe-inline'; img-src data:; base-uri 'none'; form-action 'none'">
<title>{title}</title>
<style>{STYLE}</style>
</head>
<body>
<main class="report-shell">
<header class="report-header"><p class="eyebrow">nippo visual report</p><h1>{title}</h1></header>
{visualizations}
<article class="report-content">{body}</article>
</main>
</body>
</html>
"#,
        title = escape_html(title),
    )
}

fn render_markdown(markdown: &str) -> String {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let events = Parser::new_ext(markdown, options).filter_map(sanitize_event);
    let mut rendered = String::new();
    html::push_html(&mut rendered, events);
    rendered
}

fn sanitize_event(event: Event<'_>) -> Option<Event<'_>> {
    match event {
        Event::Html(raw) | Event::InlineHtml(raw) => Some(Event::Text(raw)),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Some(Event::Start(Tag::Link {
            link_type,
            dest_url: sanitize_link(dest_url),
            title,
            id,
        })),
        Event::Start(Tag::Image { .. }) => Some(Event::Text(CowStr::Borrowed("画像: "))),
        Event::End(TagEnd::Image) => None,
        other => Some(other),
    }
}

fn sanitize_link(destination: CowStr<'_>) -> CowStr<'_> {
    let value = destination.trim();
    let lowercase = value.to_ascii_lowercase();
    let is_allowed = value.starts_with('#')
        || value.starts_with('/')
        || value.starts_with("./")
        || value.starts_with("../")
        || lowercase.starts_with("https://")
        || lowercase.starts_with("http://")
        || lowercase.starts_with("mailto:")
        || !value.contains(':');

    if is_allowed {
        destination
    } else {
        CowStr::Borrowed("#unsafe-link")
    }
}

fn render_visualizations(mode: HtmlReportMode, markdown: &str) -> String {
    match mode {
        HtmlReportMode::Insight => render_insight_visualizations(markdown),
        HtmlReportMode::Review => render_review_visualizations(markdown),
        HtmlReportMode::Trend => render_trend_visualizations(markdown),
    }
}

fn render_trend_visualizations(markdown: &str) -> String {
    [
        ("期間の概要", "期間比較"),
        ("ツール使用の変化", "ツール使用の変化"),
        ("活動量の推移", "活動量の推移"),
    ]
    .into_iter()
    .filter_map(|(heading, title)| {
        let aria_label = format!("{heading}の視覚化");
        section_table(markdown, heading)
            .and_then(|table| render_comparison_chart(&table, &aria_label))
            .map(|chart| {
                format!(
                    r#"<section class="visual-panel"><h2>{}</h2>{chart}</section>"#,
                    escape_html(title)
                )
            })
    })
    .collect()
}

fn render_review_visualizations(markdown: &str) -> String {
    let mut panels = Vec::new();
    if let Some(cards) = section_table(markdown, "定量データ").and_then(render_metric_cards) {
        panels.push(format!(
            r#"<section class="visual-panel"><h2>主要指標</h2>{cards}</section>"#
        ));
    }

    let achievements = subsections_in_section(markdown, "主要な成果");
    if !achievements.is_empty() {
        let cards = achievements
            .iter()
            .map(|achievement| {
                let fields = achievement
                    .fields
                    .iter()
                    .map(|(label, value)| {
                        format!(
                            "<dt>{}</dt><dd>{}</dd>",
                            escape_html(label),
                            escape_html(value)
                        )
                    })
                    .collect::<String>();
                format!(
                    "<article class=\"achievement-card\"><h3>{}</h3><dl>{fields}</dl></article>",
                    escape_html(strip_order_prefix(&achievement.heading))
                )
            })
            .collect::<String>();
        panels.push(format!(
            r#"<section class="visual-panel"><h2>主要な成果</h2><div class="achievement-grid">{cards}</div></section>"#
        ));
    }

    let stages = ["主要な成果", "技術的な成長", "課題と学び", "次期の目標"]
        .into_iter()
        .filter(|heading| has_section(markdown, heading))
        .map(|heading| format!("<li>{}</li>", escape_html(heading)))
        .collect::<String>();
    if !stages.is_empty() {
        panels.push(format!(
            r#"<section class="visual-panel"><h2>レビューの流れ</h2><ol class="review-flow">{stages}</ol></section>"#
        ));
    }
    panels.concat()
}

fn render_metric_cards(table: Vec<Vec<String>>) -> Option<String> {
    let metrics = table
        .iter()
        .skip(1)
        .filter_map(|row| Some((row.first()?, row.get(1)?)))
        .map(|(label, value)| (label.clone(), value.clone()))
        .collect::<Vec<_>>();
    render_labeled_cards(&metrics, "定量データの視覚化")
}

fn render_labeled_cards(metrics: &[(String, String)], aria_label: &str) -> Option<String> {
    let cards = metrics
        .iter()
        .map(|(label, value)| {
            format!(
                "<div class=\"metric-card\"><dt>{}</dt><dd>{}</dd></div>",
                escape_html(label),
                escape_html(value)
            )
        })
        .collect::<String>();
    (!cards.is_empty()).then(|| {
        format!(
            r#"<dl class="metrics-grid" aria-label="{}">{cards}</dl>"#,
            escape_html(aria_label)
        )
    })
}

fn render_insight_visualizations(markdown: &str) -> String {
    let mut panels = Vec::new();
    let metrics = labeled_list_in_section(markdown, "期間の概要");
    if let Some(cards) = render_labeled_cards(&metrics, "期間の主要指標") {
        panels.push(format!(
            r#"<section class="visual-panel"><h2>期間の概要</h2>{cards}</section>"#
        ));
    }
    if let Some(chart) = table_after_heading(markdown, "### プロジェクト別サマリー")
        .and_then(|table| render_horizontal_chart(&table, 2, "プロジェクト別サマリーの視覚化"))
    {
        panels.push(format!(
            r#"<section class="visual-panel"><h2>プロジェクト構成</h2>{chart}</section>"#
        ));
    }
    if let Some(chart) = table_after_heading(markdown, "### ツール使用傾向")
        .and_then(|table| render_horizontal_chart(&table, 1, "ツール使用傾向の視覚化"))
    {
        panels.push(format!(
            r#"<section class="visual-panel"><h2>ツール使用傾向</h2>{chart}</section>"#
        ));
    }

    let phases = subheadings_in_section(markdown, "振り返り（ALACT モデル）");
    if !phases.is_empty() {
        let items = phases
            .iter()
            .take(4)
            .map(|phase| format!("<li>{}</li>", escape_html(strip_order_prefix(phase))))
            .collect::<String>();
        panels.push(format!(
            r#"<section class="visual-panel"><h2>ALACTの流れ</h2><ol class="alact-flow">{items}</ol></section>"#
        ));
    }
    panels.concat()
}

fn render_comparison_chart(table: &[Vec<String>], aria_label: &str) -> Option<String> {
    let header = table.first()?;
    if header.len() < 4 {
        return None;
    }

    let rows = table
        .iter()
        .skip(1)
        .filter_map(|row| {
            let displays = row.iter().skip(1).take(3).cloned().collect::<Vec<_>>();
            let values = displays
                .iter()
                .map(|cell| parse_number(cell))
                .collect::<Option<Vec<_>>>()?;
            (values.iter().any(|value| *value > 0.0)).then(|| (row[0].clone(), values, displays))
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return None;
    }

    let row_height = 96.0;
    let height = 72.0 + row_height * rows.len() as f64;
    let mut content = String::new();
    for (row_index, (label, values, displays)) in rows.iter().enumerate() {
        let y = 56.0 + row_index as f64 * row_height;
        let maximum = values.iter().copied().fold(0.0_f64, f64::max);
        content.push_str(&format!(
            r#"<text class="chart-label" x="8" y="{y}">{label}</text>"#,
            label = escape_html(label),
        ));
        for (value_index, value) in values.iter().enumerate() {
            let bar_y = y + 12.0 + value_index as f64 * 20.0;
            let width = if maximum > 0.0 {
                500.0 * value.max(0.0) / maximum
            } else {
                0.0
            };
            content.push_str(&format!(
                r#"<text class="chart-period" x="8" y="{text_y}">{period}</text><rect class="series-{series}" x="74" y="{bar_y}" width="{width:.2}" height="12" rx="6"><title>{period}: {display}</title></rect><text class="chart-value" x="{value_x:.2}" y="{text_y}">{display}</text>"#,
                text_y = bar_y + 11.0,
                period = escape_html(&header[value_index + 1]),
                series = value_index + 1,
                value_x = 84.0 + width,
                display = escape_html(&displays[value_index]),
            ));
        }
    }

    Some(format!(
        r#"<svg viewBox="0 0 720 {height:.0}" role="img" aria-label="{aria_label}"><title>{aria_label}</title>{content}</svg>"#,
        aria_label = escape_html(aria_label),
    ))
}

fn render_horizontal_chart(
    table: &[Vec<String>],
    value_column: usize,
    aria_label: &str,
) -> Option<String> {
    let rows = table
        .iter()
        .skip(1)
        .filter_map(|row| {
            let display = row.get(value_column)?.clone();
            let value = parse_number(&display)?;
            Some((row.first()?.clone(), value, display))
        })
        .collect::<Vec<_>>();
    if rows.is_empty() {
        return None;
    }

    let maximum = rows
        .iter()
        .map(|(_, value, _)| *value)
        .fold(0.0_f64, f64::max);
    let height = 52.0 + rows.len() as f64 * 42.0;
    let mut content = String::new();
    for (index, (label, value, display)) in rows.iter().enumerate() {
        let y = 30.0 + index as f64 * 42.0;
        let width = if maximum > 0.0 {
            480.0 * value.max(0.0) / maximum
        } else {
            0.0
        };
        content.push_str(&format!(
            r#"<text class="chart-label" x="8" y="{y}">{label}</text><rect class="series-{series}" x="190" y="{bar_y}" width="{width:.2}" height="14" rx="7"><title>{label}: {display}</title></rect><text class="chart-value" x="{value_x:.2}" y="{y}">{display}</text>"#,
            label = escape_html(label),
            series = index % 3 + 1,
            bar_y = y - 12.0,
            value_x = 200.0 + width,
            display = escape_html(display),
        ));
    }

    Some(format!(
        r#"<svg viewBox="0 0 760 {height:.0}" role="img" aria-label="{aria_label}"><title>{aria_label}</title>{content}</svg>"#,
        aria_label = escape_html(aria_label),
    ))
}

fn section_table(markdown: &str, heading: &str) -> Option<Vec<Vec<String>>> {
    table_after_heading(markdown, &format!("## {heading}"))
}

fn table_after_heading(markdown: &str, heading: &str) -> Option<Vec<Vec<String>>> {
    let mut lines = markdown.lines().skip_while(|line| line.trim() != heading);
    lines.next()?;
    let section_lines = lines
        .take_while(|line| !line.trim_start().starts_with('#'))
        .collect::<Vec<_>>();
    let table_lines = section_lines
        .into_iter()
        .skip_while(|line| !line.trim_start().starts_with('|'))
        .take_while(|line| line.trim_start().starts_with('|'))
        .collect::<Vec<_>>();
    parse_table(&table_lines)
}

fn subheadings_in_section(markdown: &str, heading: &str) -> Vec<String> {
    let section_heading = format!("## {heading}");
    markdown
        .lines()
        .skip_while(|line| line.trim() != section_heading)
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
        .filter_map(|line| line.strip_prefix("### ").map(str::to_string))
        .collect()
}

fn subsections_in_section(markdown: &str, heading: &str) -> Vec<ReportSubsection> {
    let section_heading = format!("## {heading}");
    let mut subsections = Vec::new();
    let mut current: Option<ReportSubsection> = None;

    for line in markdown
        .lines()
        .skip_while(|line| line.trim() != section_heading)
        .skip(1)
        .take_while(|line| !line.starts_with("## "))
    {
        if let Some(subheading) = line.strip_prefix("### ") {
            if let Some(subsection) = current.take() {
                subsections.push(subsection);
            }
            current = Some(ReportSubsection {
                heading: subheading.to_string(),
                fields: Vec::new(),
            });
        } else if let (Some(subsection), Some(field)) =
            (current.as_mut(), parse_labeled_bullet(line))
        {
            subsection.fields.push(field);
        }
    }
    if let Some(subsection) = current {
        subsections.push(subsection);
    }
    subsections
}

fn parse_labeled_bullet(line: &str) -> Option<(String, String)> {
    let item = line.trim().strip_prefix("- ")?;
    let (label, value) = item.split_once(':')?;
    Some((
        label.trim().trim_matches('*').to_string(),
        value.trim().replace("**", "").replace('`', ""),
    ))
}

fn has_section(markdown: &str, heading: &str) -> bool {
    let expected = format!("## {heading}");
    markdown.lines().any(|line| line.trim() == expected)
}

fn labeled_list_in_section(markdown: &str, heading: &str) -> Vec<(String, String)> {
    let section_heading = format!("## {heading}");
    markdown
        .lines()
        .skip_while(|line| line.trim() != section_heading)
        .skip(1)
        .take_while(|line| !line.starts_with('#'))
        .filter_map(|line| line.trim().strip_prefix("- "))
        .filter_map(|item| item.split_once(':'))
        .map(|(label, value)| {
            (
                label.trim().to_string(),
                value.trim().replace("**", "").replace('`', ""),
            )
        })
        .collect()
}

fn strip_order_prefix(value: &str) -> &str {
    let Some((prefix, remainder)) = value.split_once(". ") else {
        return value;
    };
    let is_order = prefix.chars().all(|character| character.is_ascii_digit())
        || (prefix.len() == 1
            && prefix
                .chars()
                .all(|character| character.is_ascii_uppercase()));
    if is_order { remainder } else { value }
}

fn parse_table(lines: &[&str]) -> Option<Vec<Vec<String>>> {
    if lines.len() < 2 {
        return None;
    }
    let rows = lines
        .iter()
        .map(|line| {
            line.trim()
                .trim_matches('|')
                .split('|')
                .map(|cell| cell.trim().trim_matches('`').to_string())
                .collect::<Vec<_>>()
        })
        .filter(|row| {
            !row.iter().all(|cell| {
                let alignment = cell.trim_matches(':');
                !alignment.is_empty() && alignment.chars().all(|character| character == '-')
            })
        })
        .collect::<Vec<_>>();
    (rows.len() >= 2).then_some(rows)
}

fn parse_number(value: &str) -> Option<f64> {
    let value = value.trim().replace(',', "");
    if value.contains('/') || value.contains('〜') {
        return None;
    }
    let numeric = value
        .chars()
        .skip_while(|character| !character.is_ascii_digit() && *character != '-')
        .take_while(|character| {
            character.is_ascii_digit() || *character == '.' || *character == '-'
        })
        .collect::<String>();
    numeric.parse().ok()
}

fn document_title(markdown: &str) -> Option<&str> {
    markdown
        .lines()
        .find_map(|line| line.strip_prefix("# ").map(str::trim))
}

fn write_atomically(path: &Path, contents: &[u8]) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("HTML output filename is not valid UTF-8")?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before UNIX epoch")?
        .as_nanos();
    let temporary = parent.join(format!(".{file_name}.{}.{}.tmp", std::process::id(), nonce));

    if let Err(error) = fs::write(&temporary, contents) {
        let _ = fs::remove_file(&temporary);
        return Err(error)
            .with_context(|| format!("failed to write temporary HTML: {}", temporary.display()));
    }
    #[cfg(windows)]
    if path.is_file() {
        if let Err(error) = fs::remove_file(path) {
            let _ = fs::remove_file(&temporary);
            return Err(error)
                .with_context(|| format!("failed to remove previous HTML: {}", path.display()));
        }
    }
    if let Err(error) = fs::rename(&temporary, path) {
        let _ = fs::remove_file(&temporary);
        bail!("failed to replace HTML report {}: {error}", path.display());
    }
    Ok(())
}

fn escape_html(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

const STYLE: &str = r#"
:root { color-scheme: light dark; --bg: #f5f7fb; --surface: #ffffff; --text: #172033; --muted: #5c667a; --line: #dbe1ea; --accent: #3157d5; --series-1: #3157d5; --series-2: #7a52c7; --series-3: #13a38b; }
* { box-sizing: border-box; }
body { margin: 0; background: var(--bg); color: var(--text); font-family: system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; line-height: 1.7; }
.report-shell { width: min(1120px, calc(100% - 32px)); margin: 32px auto 64px; }
.report-header, .visual-panel, .report-content { background: var(--surface); border: 1px solid var(--line); border-radius: 18px; box-shadow: 0 14px 40px rgba(28, 42, 72, .08); }
.report-header { padding: 32px; background: linear-gradient(135deg, #203c9f, #526fe0); color: #fff; }
.report-header h1 { margin: 0; font-size: clamp(1.8rem, 4vw, 3.2rem); line-height: 1.2; }
.eyebrow { margin: 0 0 8px; text-transform: uppercase; letter-spacing: .12em; font-size: .75rem; opacity: .8; }
.visual-panel, .report-content { margin-top: 20px; padding: clamp(20px, 4vw, 40px); }
.visual-panel { overflow-x: auto; }
.visual-panel h2 { margin-top: 0; }
.report-content > h1:first-child { display: none; }
h2 { margin-top: 2.2em; border-bottom: 1px solid var(--line); padding-bottom: .35em; }
h3 { margin-top: 1.7em; }
table { width: 100%; border-collapse: collapse; display: block; overflow-x: auto; }
th, td { padding: 10px 12px; border-bottom: 1px solid var(--line); text-align: left; vertical-align: top; }
th { color: var(--muted); font-size: .86rem; }
code { background: #edf0f7; color: #293653; border-radius: 5px; padding: .12em .35em; }
pre { overflow-x: auto; padding: 16px; background: #172033; color: #edf2ff; border-radius: 12px; }
blockquote { margin-left: 0; padding-left: 16px; border-left: 4px solid var(--accent); color: var(--muted); }
a { color: var(--accent); overflow-wrap: anywhere; }
svg { width: 100%; min-width: 680px; height: auto; min-height: 220px; }
.chart-label { fill: var(--text); font-size: 14px; font-weight: 700; }
.chart-period { fill: var(--muted); font-size: 11px; }
.chart-value { fill: var(--text); font-size: 11px; }
.series-1 { fill: var(--series-1); }
.series-2 { fill: var(--series-2); }
.series-3 { fill: var(--series-3); }
.alact-flow { display: grid; grid-template-columns: repeat(4, minmax(0, 1fr)); gap: 12px; padding: 0; list-style: none; counter-reset: alact; }
.alact-flow li { min-height: 100px; padding: 18px; border: 1px solid var(--line); border-radius: 14px; background: color-mix(in srgb, var(--accent) 8%, var(--surface)); counter-increment: alact; }
.alact-flow li::before { content: counter(alact, upper-alpha); display: block; margin-bottom: 8px; color: var(--accent); font-size: 1.25rem; font-weight: 800; }
.metrics-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(180px, 1fr)); gap: 14px; margin: 0; }
.metric-card { padding: 18px; border: 1px solid var(--line); border-radius: 14px; background: color-mix(in srgb, var(--accent) 7%, var(--surface)); }
.metric-card dt { color: var(--muted); font-size: .82rem; }
.metric-card dd { margin: 8px 0 0; font-size: clamp(1.25rem, 3vw, 2rem); font-weight: 800; overflow-wrap: anywhere; }
.achievement-grid { display: grid; grid-template-columns: repeat(auto-fit, minmax(260px, 1fr)); gap: 14px; counter-reset: achievement; }
.achievement-card { padding: 20px; border: 1px solid var(--line); border-radius: 14px; counter-increment: achievement; }
.achievement-card h3 { margin-top: 0; }
.achievement-card h3::before { content: counter(achievement, decimal-leading-zero); display: block; color: var(--accent); font-size: .8rem; font-weight: 800; }
.achievement-card dl { display: grid; grid-template-columns: minmax(80px, auto) 1fr; gap: 8px 12px; margin: 0; }
.achievement-card dt { color: var(--muted); font-size: .82rem; font-weight: 700; }
.achievement-card dd { margin: 0; }
.review-flow { display: flex; flex-wrap: wrap; gap: 10px; padding: 0; list-style: none; }
.review-flow li { flex: 1 1 180px; padding: 14px 16px; border-left: 4px solid var(--accent); background: color-mix(in srgb, var(--accent) 7%, var(--surface)); }
@media (prefers-color-scheme: dark) { :root { --bg: #101522; --surface: #181f2d; --text: #edf2ff; --muted: #aeb9cd; --line: #303a4d; --accent: #93a9ff; } code { background: #252f42; color: #edf2ff; } }
@media (max-width: 600px) { .report-shell { width: min(100% - 16px, 1120px); margin-top: 8px; } .report-header, .visual-panel, .report-content { border-radius: 12px; } .report-header { padding: 24px 20px; } .alact-flow { grid-template-columns: 1fr; } }
@media print { :root { --bg: #fff; --surface: #fff; --text: #000; --muted: #444; --line: #bbb; } body { background: #fff; } .report-shell { width: 100%; margin: 0; } .report-header, .visual-panel, .report-content { box-shadow: none; break-inside: avoid; } a { color: #000; text-decoration: underline; } }
"#;
