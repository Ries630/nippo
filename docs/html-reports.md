# HTML レポート契約

`trend`、`insight`、`review` が Markdown とともに生成する自己完結 HTML の規範。
各テンプレートと Claude Code / Codex の skill は、このファイルを参照する。

## 生成順序とファイル名

1. skill が従来どおり `reports/{mode}-YYYY-MM-DD[-Nd].md` を保存する。
2. 保存した Markdown を `nippo render-html --mode {mode} --input {path}` へ渡す。
3. レンダラーが同じ stem の `reports/{mode}-YYYY-MM-DD[-Nd].html` を保存する。

リポジトリの checkout 内では、インストール済みバイナリより現在の実装を優先し、
`cargo run -q -p nippo -- render-html ...` を使う。

## 内容の正

Markdown を内容の生成元とする。HTML は Markdown の本文、表、リンクをすべて含み、
Markdown ファイルがなくてもレポートとして完結しなければならない。

HTML の視覚化は Markdown に存在する見出しと値から作る。Markdown にない分析、数値、
判断、評価を追加しない。詳細な理由は
[ADR-0001](adr/0001-render-html-from-markdown.md) を参照する。

## モード別の視覚化

| モード | 視覚化する構造 |
|---|---|
| `trend` | 三期間の指標比較、ツール使用の変化、活動量の推移 |
| `insight` | プロジェクト構成、ツール使用傾向、ALACT の流れ |
| `review` | 定量指標、主要成果、成長・課題・次期目標の流れ |

元の表と本文も HTML 内に残す。グラフだけで意味を伝えず、色だけに依存しないラベルを付ける。

## 自己完結性と安全性

- CSS と SVG は HTML 内へ埋め込む
- JavaScript、外部 CSS、Web フォント、外部画像、CDN を使わない
- Markdown 内の生 HTML は文字列として表示し、実行しない
- 危険な URL スキームをリンクとして出力しない
- デスクトップ、幅 390px 前後のモバイル、印刷で本文を読めるようにする

## 失敗時の扱い

HTML は一時ファイルへ書き、完成後に置き換える。不完全な HTML を出力先へ残さない。
HTML 生成に失敗しても、先に完成した Markdown は削除・巻き戻ししない。skill は Markdown の
保存先と HTML 生成の失敗を明示し、HTML も生成できたとは報告しない。
