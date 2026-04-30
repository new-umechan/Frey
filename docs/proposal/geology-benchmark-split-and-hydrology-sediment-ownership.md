# Geology ベンチ分離と Hydrology 所有 sediment state への移行

## Status

Accepted

## 背景

- 現行の `geology_solo` は Earth 実データ比較ベンチとして文書化されているが、実装は tectonics の runtime / 構造診断ベンチに近い。
- そのため、`docs/operations/bench/geology/solo.md` の説明と実装責務が一致していない。
- 一方で `erosion_rate` と `deposition_rate` は、計算主体としては Hydrology が算出している。
- しかし公開 state 上は `GeologyState` に保存されており、ownership と責務の境界が曖昧になっている。
- `docs/reference/modules/hydrology.md` は既に `hydrology.erosion_rate` / `hydrology.deposition_rate` を Hydrology の出力として扱っており、仕様と実装にもずれがある。

## 目的

- 現行の tectonics 診断ベンチを `geology_solo` から切り離し、別名の validation bench として再定義する。
- `geology_solo` という名前は、将来の Earth 実データ入力ベンチのために空ける。
- `erosion_rate` / `deposition_rate` の state ownership を `HydrologyState` へ移す。
- 侵食・堆積の Earth 比較は Geology 単体ではなく、Hydrology 単体ベンチで扱う方針を明確化する。

## 提案概要

### 1. ベンチの役割分離

- 現行 `geology_solo` を `geology_validation_solo` へ改名する。
- これは tectonics の runtime、構造変化、内部診断を記録する validation bench とする。
- `geology_solo` は将来の Earth 実データ入力ベンチ名として予約し、現時点では未実装扱いにする。

### 2. Earth 実データベンチの位置づけ

- 新しい `geology_solo` は、Earth 実データを入力として与え、Geology 系の出口だけを比較する bench とする。
- ここでいう「本物」は、preset 再現ではなく、Earth 地形・気候・水文などの外部入力を読ませる I/O bench を意味する。
- ただし fluvial erosion / deposition は Hydrology の計算責務であるため、主比較は Hydrology 側で扱う。

### 3. sediment state ownership

- `erosion_rate` と `deposition_rate` は `GeologyState` から削除し、`HydrologyState` に移す。
- Hydrology はこれらを正本として更新する。
- Geology は `hydrology.erosion_rate` / `hydrology.deposition_rate` を読んで、標高・地殻厚・export accounting へ反映する。
- Ecology や query API などの参照側も、公開パスを `hydrology.*` へ寄せる。

### 4. ベンチ運用

- `hydrology_solo` は `river_flow` / `is_lake` に加えて、`erosion_rate` / `deposition_rate` の参考値を保持する唯一の単体 bench とする。
- `geology_validation_solo` には、侵食・堆積 Earth 比較指標を持ち込まない。
- 将来の `geology_solo` を実装する場合も、侵食・堆積の主指標を入れるのではなく、Geology 固有の Earth 応答指標に絞る。

## スコープ

この proposal で決めること:

- 現行 `geology_solo` の改名
- `geology_solo` の将来用途の予約
- `erosion_rate` / `deposition_rate` の Hydrology 所有への移行
- Hydrology ベンチを侵食・堆積評価の単体責務とすること

この proposal でまだ決めないこと:

- 将来の `geology_solo` の具体的な入出力仕様
- Earth 実データ入力 bench で比較する Geology 固有指標の最終セット
- `geology_validation_solo` の quality gate 閾値

## 成功条件

- 現行の tectonics 診断 bench が `geology_validation_solo` という名前で実行できる。
- `geology_solo` の名称が、現実装の validation bench に使われなくなる。
- `erosion_rate` / `deposition_rate` の公開 state path が `hydrology.*` に統一される。
- `hydrology_solo` が、侵食・堆積の参考評価を継続して出力できる。

## リスクとトレードオフ

- ベンチ名変更により、既存のスクリプト・artifact 名・手順書の追従が必要になる。
- `erosion_rate` / `deposition_rate` の所有移動は参照箇所が多く、追従漏れがあると query / visualization / tests が壊れやすい。
- 一方で ownership を実態に合わせることで、今後の Earth bench 設計は明確になる。

## 実施計画

1. proposal を追加し、Geology benchmark 文書の役割を再整理する。
2. 現行 `geology_solo` を `geology_validation_solo` へ改名し、関連スクリプトと docs を更新する。
3. `erosion_rate` / `deposition_rate` を `HydrologyState` へ移し、参照箇所を更新する。
4. `hydrology_solo` が引き続き erosion / deposition 参考値を出すことを確認する。
5. 将来の `geology_solo` 実装は別 proposal / decision で進める。

## 未解決事項

- `geology_validation_solo` の artifact 名を完全に分離するか、既存 `geology_*` を引き継ぐか
- 将来の `geology_solo` で Earth 入力として何を必須にするか
- Geology 固有の Earth 出力ベンチを単体で成立させるか、coupling bench として切るか
