use anyhow::{Context, Result, ensure};
use std::fs;
use std::process::Command;

#[test]
fn trend_markdown_renders_to_self_contained_html() -> Result<()> {
    let directory = tempfile::tempdir().context("create temp directory")?;
    let markdown_path = directory.path().join("trend-2026-09-01-90d.md");
    fs::write(
        &markdown_path,
        r#"# トレンド分析 2026年9月1日（過去90日間）

## 期間の概要

| 指標 | 前期 | 中期 | 後期 |
|---|---:|---:|---:|
| セッション数 | 10 | 20 | 30 |
| ツール使用回数 | 100 | 150 | 120 |

## ツール使用の変化

| ツール | 前期 | 中期 | 後期 | 変化の傾向 |
|---|---:|---:|---:|---|
| Read | 20 (20%) | 30 (30%) | 40 (40%) | 増加 |
| Edit | 30 (30%) | 20 (20%) | 10 (10%) | 減少 |

## 活動量の推移

| 指標 | 前期 | 中期 | 後期 |
|---|---:|---:|---:|
| 1日あたりセッション数 | 1.0 | 2.0 | 3.0 |
| 1日あたりメッセージ数 | 10 | 20 | 25 |

## 全体を通して見える変化

本文に <script>alert("危険")</script> を書いても実行されない。
"#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_nippo"))
        .args([
            "render-html",
            "--mode",
            "trend",
            "--input",
            markdown_path
                .to_str()
                .context("markdown path is not UTF-8")?,
        ])
        .output()
        .context("run nippo render-html")?;

    ensure!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let html_path = markdown_path.with_extension("html");
    let html = fs::read_to_string(&html_path).context("read generated HTML")?;
    ensure!(html.contains("<!doctype html>"));
    ensure!(html.contains("<html lang=\"ja\">"));
    ensure!(html.contains("name=\"viewport\""));
    ensure!(html.contains("@media print"));
    ensure!(html.contains("トレンド分析 2026年9月1日"));
    ensure!(html.contains("<svg"));
    ensure!(html.contains("aria-label=\"期間の概要の視覚化\""));
    ensure!(html.contains("aria-label=\"ツール使用の変化の視覚化\""));
    ensure!(html.contains("aria-label=\"活動量の推移の視覚化\""));
    ensure!(html.contains("&lt;script&gt;alert(\"危険\")&lt;/script&gt;"));
    ensure!(!html.contains("<script"));
    ensure!(!html.contains("<link"));
    ensure!(!html.contains("src=\"http"));
    ensure!(
        String::from_utf8_lossy(&output.stdout).contains(&format!("html: {}", html_path.display()))
    );

    Ok(())
}

#[test]
fn insight_markdown_visualizes_projects_and_alact_without_changing_prose() -> Result<()> {
    let directory = tempfile::tempdir().context("create temp directory")?;
    let markdown_path = directory.path().join("insight-2026-09-01-7d.md");
    fs::write(
        &markdown_path,
        r#"# Insight 2026年9月1日（過去7日間）

## 期間の概要

- 対象期間: 2026/08/26 〜 2026/09/01
- セッション数: 12
- プロジェクト数: 2

### プロジェクト別サマリー

| プロジェクト | セッション数 | メッセージ数 | 主な作業 |
|---|---:|---:|---|
| nippo | 8 | 120 | HTMLレポート |
| sample | 4 | 40 | 調査 |

### ツール使用傾向

| ツール | 使用回数 | 割合 |
|---|---:|---:|
| Read | 30 | 60% |
| Edit | 20 | 40% |

## 振り返り（ALACT モデル）

### A. 行動を振り返る（Looking back on the Action）

確認できた事実を書く。

### B. 本質への気づき（Awareness of essential aspects）

推測は推測として書く。

### C. 概念化（Creating alternative methods of action）

別の方法を検討する。

### D. 来週の実験（Trial）

小さな実験を提案する。

## あなたの番

- 自分の実感は？
  >
"#,
    )?;

    let output = render("insight", &markdown_path)?;
    ensure!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let html = fs::read_to_string(markdown_path.with_extension("html"))?;
    ensure!(html.contains("aria-label=\"プロジェクト別サマリーの視覚化\""));
    ensure!(html.contains("aria-label=\"ツール使用傾向の視覚化\""));
    ensure!(html.contains("aria-label=\"期間の主要指標\""));
    ensure!(html.contains("class=\"alact-flow\""));
    ensure!(html.contains("<li>行動を振り返る（Looking back on the Action）</li>"));
    ensure!(html.contains("確認できた事実を書く。"));
    ensure!(html.contains("推測は推測として書く。"));
    ensure!(html.contains("あなたの番"));

    Ok(())
}

#[test]
fn review_markdown_presents_metrics_achievements_and_review_flow() -> Result<()> {
    let directory = tempfile::tempdir().context("create temp directory")?;
    let markdown_path = directory.path().join("review-2026-09-01-90d.md");
    fs::write(
        &markdown_path,
        r#"# 自己評価レビュー 2026年9月1日（過去90日間）

## 期間サマリー

- 対象期間: 2026/06/04 〜 2026/09/01
- 稼働セッション数: 120
- 関わったプロジェクト数: 3

## 主要な成果

### 1. nippo

- **成果**: HTMLレポートを追加した
- **インパクト**: 期間の変化を把握しやすくした

### 2. sample

- **成果**: 安全な変換処理を実装した
- **インパクト**: 生HTMLの実行を防いだ

## 定量データ

| 指標 | 値 |
|---|---:|
| 総セッション数 | 120 |
| 総メッセージ数 | 3,400 |
| 意思決定ポイント数 | 24 |

## 技術的な成長

安全なHTML変換を理解した。

## 課題と学び

テンプレート構造との結合をテストする。

## 次期の目標

1. 視覚化の回帰を防ぐ
"#,
    )?;

    let output = render("review", &markdown_path)?;
    ensure!(
        output.status.success(),
        "command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let html = fs::read_to_string(markdown_path.with_extension("html"))?;
    ensure!(html.contains("aria-label=\"定量データの視覚化\""));
    ensure!(html.contains("class=\"achievement-grid\""));
    ensure!(html.contains("1. nippo"));
    ensure!(html.contains("<h3>nippo</h3>"));
    ensure!(html.contains("<dt>成果</dt><dd>HTMLレポートを追加した</dd>"));
    ensure!(html.contains("<dt>インパクト</dt><dd>期間の変化を把握しやすくした</dd>"));
    ensure!(html.contains("class=\"review-flow\""));
    ensure!(html.contains("技術的な成長"));
    ensure!(html.contains("次期の目標"));
    ensure!(html.contains("安全なHTML変換を理解した。"));

    Ok(())
}

#[test]
fn renderer_escapes_raw_html_and_disables_active_or_external_embeds() -> Result<()> {
    let directory = tempfile::tempdir().context("create temp directory")?;
    let markdown_path = directory.path().join("insight-2026-09-01.md");
    fs::write(
        &markdown_path,
        r#"# 安全性レポート

<img src="https://example.com/tracker.png" onerror="alert(1)">

[危険なリンク](javascript:alert(1))

![外部画像](https://example.com/chart.png)
"#,
    )?;

    let output = render("insight", &markdown_path)?;
    ensure!(output.status.success());
    let html = fs::read_to_string(markdown_path.with_extension("html"))?;

    ensure!(html.contains("&lt;img src=\"https://example.com/tracker.png\""));
    ensure!(html.contains("href=\"#unsafe-link\""));
    ensure!(html.contains("画像: 外部画像"));
    ensure!(!html.contains("<img"));
    ensure!(!html.contains("javascript:"));

    Ok(())
}

#[test]
fn failed_replace_keeps_markdown_and_removes_temporary_html() -> Result<()> {
    let directory = tempfile::tempdir().context("create temp directory")?;
    let markdown_path = directory.path().join("trend-2026-09-01.md");
    let html_path = markdown_path.with_extension("html");
    fs::write(&markdown_path, "# トレンド分析\n")?;
    fs::create_dir(&html_path)?;

    let output = render("trend", &markdown_path)?;
    ensure!(!output.status.success());
    ensure!(fs::read_to_string(&markdown_path)? == "# トレンド分析\n");
    ensure!(html_path.is_dir());
    let temporary_prefix = format!(
        ".{}.",
        html_path
            .file_name()
            .and_then(|name| name.to_str())
            .context("HTML filename is not UTF-8")?
    );
    ensure!(
        fs::read_dir(directory.path())?
            .filter_map(Result::ok)
            .all(|entry| !entry
                .file_name()
                .to_string_lossy()
                .starts_with(&temporary_prefix))
    );

    Ok(())
}

#[test]
fn rendering_again_replaces_the_previous_complete_html() -> Result<()> {
    let directory = tempfile::tempdir().context("create temp directory")?;
    let markdown_path = directory.path().join("review-2026-09-01.md");
    fs::write(&markdown_path, "# 最初のレビュー\n")?;
    ensure!(render("review", &markdown_path)?.status.success());

    fs::write(&markdown_path, "# 更新後のレビュー\n")?;
    ensure!(render("review", &markdown_path)?.status.success());

    let html = fs::read_to_string(markdown_path.with_extension("html"))?;
    ensure!(html.contains("更新後のレビュー"));
    ensure!(!html.contains("最初のレビュー"));

    Ok(())
}

#[test]
fn missing_markdown_fails_without_creating_html() -> Result<()> {
    let directory = tempfile::tempdir().context("create temp directory")?;
    let markdown_path = directory.path().join("missing-trend.md");

    let output = render("trend", &markdown_path)?;

    ensure!(!output.status.success());
    ensure!(String::from_utf8_lossy(&output.stderr).contains("failed to read Markdown report"));
    ensure!(!markdown_path.with_extension("html").exists());

    Ok(())
}

#[test]
fn unsupported_mode_is_rejected_by_the_cli() -> Result<()> {
    let directory = tempfile::tempdir().context("create temp directory")?;
    let markdown_path = directory.path().join("report.md");
    fs::write(&markdown_path, "# Report\n")?;

    let output = render("daily", &markdown_path)?;

    ensure!(!output.status.success());
    ensure!(String::from_utf8_lossy(&output.stderr).contains("invalid value 'daily'"));
    ensure!(!markdown_path.with_extension("html").exists());

    Ok(())
}

fn render(mode: &str, markdown_path: &std::path::Path) -> Result<std::process::Output> {
    Command::new(env!("CARGO_BIN_EXE_nippo"))
        .args([
            "render-html",
            "--mode",
            mode,
            "--input",
            markdown_path
                .to_str()
                .context("markdown path is not UTF-8")?,
        ])
        .output()
        .context("run nippo render-html")
}
