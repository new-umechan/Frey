# Alpha Era遷移の統合ベンチを常設する

## Status

Accepted

## Context

- `alpha` の `Environment` 期立ち上がりで、陸海比や海面が急変する不具合が手動確認で見つかった。
- module 単体の bench だけでは、phase 境界をまたぐ統合挙動の破綻を早期検知しにくい。

## Decision

- `alpha` 固定の統合 benchmark `alpha_transition_guard` を追加する。
- benchmark は `Crust -> Environment` を連続実行し、遷移近傍の時系列を JSONL へ記録する。
- benchmark 判定は二段階にする（hard fail: 質量保存 proxy、warning: 形状急変）。
- `Glaciology` は `Environment` 初期にスピンアップ窓を持ち、`sea_level_offset` は時定数緩和で更新する。
- 運用は `test` ではなく `bench` 系コマンドに統一する。

## Consequences

- 手動目視に依存していた症状が artifact と数値判定で再現可能になる。
- しきい値設計は初期値から運用し、false positive/false negative を見て段階調整する。
