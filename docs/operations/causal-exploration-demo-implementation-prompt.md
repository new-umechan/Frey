# 因果探索 Demo Slice 実装プロンプト

以下を、そのままコーディングエージェントへの実装依頼として使う。

## Prompt

```text
Frey の因果探索 Demo Slice を実装してください。

必ず日本語で応答してください。
このタスクは新機能の一般化ではなく、体験検証用 experiment の実装です。

最初に次の文書を読んでください。

- docs/proposal/causal-exploration-mode.md
- docs/decisions/260428-causal-exploration-demo-slice.md
- docs/reference/interface/wasm_api.md
- docs/reference/interface/ui_spec.md

最重要前提:

- decision は恒久仕様ではなく、今回の experiment をぶらさず実装するための暫定制約です
- proposal の目的は、因果探索体験が成立するかを確かめることです
- UI の演出で DTO にない因果を補完してはいけません

実装制約:

- 初回は `border_mountain_plate_demo` という静的 Demo Slice を実装する
- `world_id` は存在確認にだけ使い、データ本体は固定値で返す
- feature は `border_segment`、`ridge_or_mountain_band`、`tectonic_compression_or_plate_boundary` の 3 つに固定する
- trace は `ridge_alignment`、`passability_break`、`tectonic_driver` の 3 本に固定する
- relation type は `constraint_alignment`、`geomorphic_structure`、`tectonic_driver` の 3 種だけを使う
- 国境と山脈の関係は直接因果として扱わず、`constraint_alignment` として扱う
- 色、太さ、流速、揺らぎは `display_mapping` からだけ導く
- evidence には `assumptions`、`approximations`、`uncertainty_reason` を必ず含める
- 詳細な根拠パネルは実装しない
- 初回は evidence 種別と不確実性理由の短い表示だけを許可する

実装対象:

1. Rust / WASM API
- `WorldSimController.get_causal_exploration_demo(world_id: string) -> CausalExplorationDemoResponse` を追加する
- DTO には `demo_id`、`features`、`trace_segments`、`metrics`、`display_mapping`、`evidence` を含める
- unknown world ではエラーにする

2. Web / Engine Client
- engine worker / client に API を通す
- response 正規化層を追加する

3. Three.js / UI
- 既存 scene に因果探索レイヤを追加する
- 発光点、3 本の流れ、短い数値ラベルを出す
- `hover/focus = 接近反応`
- `click/tap = trace 固定`
- `trace click/tap = 次の対象へフォーカス`

4. Docs-first
- 実装に合わせて reference 文書を更新する
- proposal / decision の前提を壊さない

テスト要件:

- Rust:
  - DTO の serde / wasm serialization を確認する
  - known world で 3 trace を返すこと
  - unknown world でエラーになること
  - relation type と evidence 必須項目が欠落しないこと

- Web:
  - engine client の型と response normalization を確認する
  - 3 trace だけが生成されること
  - DTO 外の trace が生成されないこと
  - click / tap で active trace が切り替わること

実装の進め方:

- まず既存コードを読み、変更箇所を特定する
- その後に docs / Rust / Web / テストの順で進める
- 不要な一般化はしない
- `mod.rs` は薄く保つ
- 4 spaces indentation を守る

完了時には:

- 変更した主要ファイル
- 実装したこと
- 実行したテスト
- 通らなかった既存失敗があればその切り分け

を簡潔に報告してください。
```

## 使い分け

- experiment の実装を 1 回で進めたいときは、この prompt をそのまま使う
- 実装前に議論したい場合は、上の Prompt から `実装対象` と `テスト要件` だけを抜いて相談用に短縮する
