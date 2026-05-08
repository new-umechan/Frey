# Hydrology の理想 MFD 再現を採用する

## Status

Accepted

## Context

これまでの Hydrology 実装は `river_next`（単一流下先）を主系として扱い、
`river_downstream` は `rebuild_mfd_from_primary` により 1.0 重みの単一 edge へ再構築していた。

この構成では、仕様上 MFD を掲げていても実挙動は実質 SFD に近くなり、
デルタ・扇状地・緩斜面での分散流を十分に表現できない。

## Decision

Hydrology の流路正本を `river_downstream` に置く。

1. `river_flow` は `river_downstream`（重み付き分配）で累積する。
2. 分配率は Holmgren 系の `fraction_i ∝ slope_i^x` を採用し、`x` は勾配依存の可変指数で近似する。
3. MFD 分岐は候補上位4本を保持し、刈り込み後に重みを再正規化する。
4. fill-spill overflow は単一 spill edge（`weight=1.0`）を維持する。
5. `river_next` は互換用の派生ビューとして保持し、最大重み edge を代表流下先として扱う。

## Consequences

- 公開 `river_downstream` が実際の分配流ネットワークを反映する。
- `river_next` 前提ロジックは互換レイヤへ限定される。
- 1セルあたりの分岐保持を4本に増やすため、局所的にメモリアクセスと計算量が増える。
- 可変指数は研究知見に沿うが、係数は計算コストと安定性を優先した近似値であり、今後の較正余地を残す。
