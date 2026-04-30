# Geology単体ベンチ（Earth 実データ入力, planned）

## 概要

`geology_solo` は将来実装する Earth 実データ入力 bench の予約名である。
現時点では未実装であり、既存の tectonics 診断 bench は `geology_validation_solo` として別管理する。

- 想定入力: Earth 地形・気候・水文などの外部入力
- 想定目的: Geology 系の Earth 応答を I/O として比較する
- 非目的: tectonics validation bench の兼用

設計方針は次のとおり。

- Earth 固有 preset への過剰適合ではなく、Earth 実データ入力に対する出口比較を行う
- fluvial `erosion_rate` / `deposition_rate` は Hydrology の責務として扱う
- 現行 `hydrology_solo` と責務が衝突しない指標だけを持ち込む

## 現在の状態

まだ実装していない。
現行 bench は [validation_solo.md](/Users/umehararyu/prog/100days/Frey/docs/operations/bench/geology/validation_solo.md) を参照する。

## 想定範囲

候補は今後の proposal / decision で確定するが、少なくとも次を前提にする。

- Earth 地形入力は preset ではなく外部データを読む
- `erosion_rate` / `deposition_rate` の主比較は Hydrology 側へ置く
- Geology bench には Geology 固有の応答指標だけを載せる

## 関連

- `docs/proposal/geology-benchmark-split-and-hydrology-sediment-ownership.md`
- `docs/operations/bench/geology/validation_solo.md`
