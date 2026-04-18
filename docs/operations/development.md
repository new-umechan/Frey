# Development

本書は運用文書である。日常開発で必要なセットアップ、実行、更新手順だけを扱う。

設計の正本は `docs/concepts/overview.md`、API と型の正本は `docs/reference/` 配下を参照する。

## 前提ツール

- Node.js
- `pnpm`
- Rust toolchain
- `wasm-pack`

## 初回セットアップ

```sh
pnpm install
```

必要に応じて Git hooks を入れる:

```sh
pnpm hooks:install
```

## 日常コマンド

| 目的             | コマンド            |
| ---------------- | ------------------- |
| 開発サーバー起動 | `pnpm dev`          |
| 本番ビルド       | `pnpm build`        |
| プレビュー       | `pnpm preview`      |
| Web テスト実行   | `pnpm test:run`     |
| Rust 整形        | `pnpm format:rust`  |
| Rust lint        | `pnpm lint:rust`    |
| 事前チェック     | `pnpm lint:prepush` |

## 変更後の確認

軽い確認:

```sh
pnpm test:run
cargo test --manifest-path rust/Cargo.toml
```

ゲート確認:

```sh
pnpm test:gate
pnpm bench:perf:gate
```

用途ごとの詳しいルールは `docs/operations/test.md` と `docs/operations/benchmark.md` を参照する。

## 文書更新

Module 依存関係の自動生成ブロックを更新する場合:

```sh
pnpm module:docs
```

このコマンドは `docs/reference/architecture/module_boundaries.md` の自動生成領域を更新する。

## 補足

- `TODO.md` は個人メモであり、手順や仕様の正本ではない
- 調査途中の知見は `docs/research/` に置き、採用済み仕様は `docs/reference/` に反映する
