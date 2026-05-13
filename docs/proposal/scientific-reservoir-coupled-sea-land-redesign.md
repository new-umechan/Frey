# 学術整合な貯留層結合による海陸安定化再設計

## Status

Accepted

## 背景

- 現行モデルでは `t=805` 付近で海優勢に見える状態と、`t=850` 付近で急激な全陸化ジャンプが共存している。
- 直近の描画修正で `sea_level_offset` 判定不一致は解消したが、内部ダイナミクスの不安定性は残った。
- 既存対応は era 境界の段差抑制や交換上限などの局所対策が中心で、状態変数と保存則の設計が不足している。

## 目的

- 海陸判定を物理状態に対して一貫化し、era 遷移でも連続性を保つ。
- 水・氷・地形の相互作用を保存則ベースで拘束し、単一係数のパッチ調整依存を脱却する。
- ベンチマークを warning 駆動から violation 駆動へ移し、崩壊を自動検知できるようにする。

## 提案概要

- 状態分離を明示する。
    - `bedrock_elevation`（固体地形）
    - `ice_thickness`（氷床）
    - `sea_level_offset`（海面）
    - 描画・海陸判定は `surface_elevation = bedrock_elevation + ice_thickness - sea_level_offset`
- 水収支の保存則を runtime invariant として導入する。
    - `ocean + coupling*ice` を既存の mass proxy から拡張し、tick ごとの drift を厳密に監視する。
    - 閾値超過は `alpha_transition_guard` で warning ではなく violation にする。
- stiff 系対策として、氷床・海面・アイソスタシー更新を緩和形で統一する。
    - `x_{t+1} = x_t + (x_eq - x_t) * (1 - exp(-dt/tau))`
    - `tau_ice`, `tau_sea`, `tau_isostasy` を分離し、era 切替時は ramp 補間する。
- 評価指標を land ratio 単独から拡張する。
    - hypsometry（標高ヒストグラム）
    - 連結成分数
    - 最大大陸比率
    - `d(sea_level_offset)/dt`
    - `render_land_ratio_diff`

## スコープ

- `rust/src/sim/world/metrics.rs`
- `rust/src/sim/glaciology/surface.rs`
- `rust/src/sim/climate/surface.rs`
- `rust/src/sim/geology/surface.rs`
- `rust/src/bin/alpha_transition_guard.rs`
- `benches/results/alpha_transition_guard/*.jsonl` の schema 拡張
- `docs/operations/benchmark.md`
- `docs/reference/modules/glaciology.md`
- `docs/reference/modules/geology.md`

## 成功条件

- `alpha_transition_guard` で `t=780..900` の run が、海陸ジャンプ violation なしで通る。
- `t=850` 近傍の `land_ratio` 急変（例: 0.30 -> 0.98）が再発しない。
- `render_land_ratio_diff` が閾値以内で安定する。
- 係数変更なしで再実行しても再現性がある（run-to-run drift が小さい）。

## リスクとトレードオフ

- 状態分離により field 数と同期コストが増える。
- 緩和時定数の導入で短期応答は鈍くなる可能性がある。
- 既存 artifact schema と bench 解析スクリプトの後方互換が壊れるため、移行作業が必要。

## 実施計画

1. `alpha_transition_guard` の violation 条件を強化し、診断項目を追加する。
2. `surface_elevation` ベースの海陸判定へ内部実装を統一する。
3. 氷床・海面・アイソスタシーを緩和形へ変更し、era ramp を導入する。
4. hypsometry / 連結性指標を bench へ追加する。
5. docs/reference を実装に合わせて更新する。

## 未解決事項

- `tau_*` の初期値をどの文献レンジに合わせるか（年換算と tick 換算の対応）。
- `surface_elevation` を public field として transport するか、内部導出のままにするか。
- 連結性指標を gate 化する際の閾値設定（seed 固有最適化を避ける方法）。

## 実装補足

- faithful な `crust_exec_pipeline_hypsometry_series` と `alpha_transition_guard` を通じて、
  Crust 末期の hypsometry 圧縮と `Crust -> Environment` 遷移崩壊を切り分ける
  診断系は実装済みとする。
- `reference_isostatic_column`、shoreline 保護、`land_freeboard_p90` gate などの
  現行 legacy Geology 安定化策は、本 proposal の補助戦術として維持する。
- stable continental / `PassiveMargin` の詳細な時系列診断は、
  設計案ではなく旧系の棚卸し対象として bench 側へ退避する。

## 検証履歴の退避

旧 Geology の `vxx` artifact 比較、棄却仮説、handover 用サマリは
[legacy_hypsometry_handover.md](/Users/umehararyu/prog/100days/Frey/docs/operations/bench/geology/legacy_hypsometry_handover.md)
を正本とする。

本 proposal には再設計案と採用済み方針だけを残し、
内部検証の時系列ログは `docs/operations/bench/geology/` に分離する。
