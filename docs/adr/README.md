# Architecture Decision Records

コードから理由を読み取れない長期的な設計判断を、判断した時点の記録として残す。

## 運用

1. 最初の ADR が必要になった時点でこのディレクトリを作る。
2. Issue で判断し、ADR を実装する PR に含める。
3. ファイル名は `NNNN-kebab-case-summary.md` とし、連番にする。
4. 判断が変わったら新しい ADR で置換する。古い判断の理由を上書きしない。
5. 用語の定義は `CONTEXT.md`、ADR の作成基準と書式は `adr` skill を正とする。

## 一覧

| # | 決定 | ステータス |
|---|---|---|
| [0001](0001-render-html-from-markdown.md) | Markdownから自己完結HTMLを生成する | 承認済み |

## テンプレート

[`template.md`](template.md) をコピーして使う。
