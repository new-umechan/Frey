# 学術整合な海陸安定化のため貯留層結合モデルへ再設計する

## Status

Accepted

## Context

- 描画側の `sea_level_offset` 不一致は解消したが、内部では `t=850` 近傍に急激な全陸化ジャンプが残っている。
- 既存の境界再基準化や交換上限は局所的に有効だが、状態変数の責務分離と保存則拘束が弱く、再発リスクが高い。
- 学術的妥当性を担保するためには、海面・氷床・地形を一つの整合モデルとして扱う必要がある。

## Decision

- 海陸安定化の主戦略を「係数パッチ」から「貯留層結合の再設計」へ切り替える。
- 海陸判定の基準状態を `surface_elevation = bedrock + ice - sea_level` に統一する。
- `alpha_transition_guard` で以下を violation として gate 化する。
    - 大域 land ratio ジャンプ
    - sea level ジャンプ
    - mass proxy drift
    - 連結性崩壊（追加予定）
- 氷床・海面・アイソスタシー更新は緩和時定数ベース（指数緩和）で実装する。
- era 切替は係数の段差切替を禁止し、有限 tick の ramp 補間を必須とする。

## Rationale

- 海陸崩壊は単一モジュール起因ではなく、複数 reservoir の結合不安定として現れている。
- 保存則と時定数分離を軸にすると、モデルの頑健性をパラメータ微調整に依存せず説明可能になる。
- violation gate を強化することで、視覚評価に依存せず回帰を早期検知できる。

## Consequences

- 短期的には実装範囲が拡大し、bench schema も更新が必要になる。
- 中長期的には era 遷移時の崩壊再発が減り、学術整合な説明可能性が向上する。
- 既存の海面連続性対策（260509）は本決定の下位戦術として維持しつつ、順次統合する。

## Update 2026-05-12

- faithful な `crust_exec_pipeline_hypsometry_series` と `alpha_transition_guard` を
  legacy Geology の主診断系として採用する。
- `equilibrium_thickness` の `height` 依存は禁止し、
  `reference_isostatic_column` と整合する regime ベース target へ寄せる。
- `reference_freeboard + compensated_thickness_anomaly` の split diagnostics を持ち、
  raw/applied、oceanic/continental、orogenic/stable、passive/transform を順に分解して読む。
- `Crust -> Environment` 入口の大域崩壊は、surface diffusion ではなく
  Crust stress memory carry-over が主因と判断し、stress memory quench を採用する。
- Crust 末期の shoreline crowding には、`preserve_crust_freeboard` 補助、
  shoreline remap、`land_freeboard_p90` gate を legacy 安定化策として採用する。
- 詳細な artifact 比較や棄却仮説の時系列は、decision ではなく bench 側へ退避する。

## 検証履歴の退避

legacy Geology の `vxx` 診断履歴、棄却仮説、handover 用サマリは
[legacy_hypsometry_handover.md](/Users/umehararyu/prog/100days/Frey/docs/operations/bench/geology/legacy_hypsometry_handover.md)
を正本とする。

本 decision には採用・棄却の判断だけを残し、
artifact ごとの数値比較や実験ログは `docs/operations/bench/geology/` へ分離する。
