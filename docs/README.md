# ドキュメント役割分担

本書は、`docs/` 配下の文書をどこへ置くかを決めるための運用ルールである。

基本方針:

- `docs/` は浅く保つ
- `README.md` は入口、詳細は `docs/` に置く
- 提案、判断、実装仕様、外部調査、内部検証ログを混在させない

## 目的

- 提案、判断、実装仕様、外部調査、内部検証ログを混在させない
- 実装の現状と、そこに至る試行錯誤を別々に追えるようにする
- 大規模な作り替え時に、旧設計の履歴と新設計の意図が混ざらないようにする

## 役割

### `docs/research/`

- 外部文献、既存手法、概念調査
- まだ採用判断していない材料
- 実験 run の `vxx` ログは置かない

### `docs/proposal/`

- これから変える設計案
- 未来向きの意図、スコープ、成功条件
- 実験の時系列ログは置かない

### `docs/decisions/`

- 採用・却下の判断と理由
- proposal のうち採択された内容
- 実験値は必要最小限の根拠だけを残し、詳細ログは bench 文書へ委譲する

### `docs/reference/`

- 現在の実装仕様
- as-is の挙動、モジュール責務、公開前提
- 変更履歴や試行錯誤ログは置かない

### `docs/operations/bench/`

- ベンチマークの実行方法
- artifact の読み方
- 内部検証ログ、比較履歴、棄却した仮説
- `vxx` 系の検証はここに置く

## Geology の運用

- `docs/research/procedural_tctonic_planets.md`
    - `Procedural Tectonic Planets` 論文の調査ノートとして扱う
- `docs/proposal/*`
    - Geology の新旧設計案を分けて置く
- `docs/operations/bench/geology/*`
    - Crust / Environment の内部診断、artifact の比較結果、旧 Geology の棚卸しを置く

## 作業ルール

1. 外部調査は `research` に書く
2. 変更案は `proposal` にまとめる
3. 採用判断は `decisions` に移す
4. 実装後の仕様を `reference` に反映する
5. 実験ログと比較履歴は `operations/bench` に残す

## 退避ルール

- 大きな作り替え前は、旧系の dirty state を `WIP` で固定する
- 旧系の検証ログは `bench` 文書へ退避し、新設計文書へ直接持ち込まない
- 新設計は既存 proposal を上書きせず、新しい proposal として起票する
