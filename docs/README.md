# ドキュメント役割分担

本書は、`docs/` 配下の文書をどこへ置くかを決めるための運用ルールである。

基本方針:

- `docs/` は浅く保つ
- `README.md` は入口、詳細は `docs/` に置く
- 判断、検討中の設計、実装仕様、外部調査、内部検証ログを混在させない

## 目的

- 判断、検討中の設計、実装仕様、外部調査、内部検証ログを混在させない
- 実装の現状と、そこに至る試行錯誤を別々に追えるようにする
- 大規模な作り替え時に、旧設計の履歴と新設計の意図が混ざらないようにする

## 役割

### `docs/research/`

- 外部文献、既存手法、概念調査
- まだ採用判断していない材料
- 実験 run の `vxx` ログは置かない

### `docs/decisions/`

- 採用・却下・保留中の重要判断と理由
- これから変える設計案のうち、追跡したい検討状態
- `Status` で `Draft`, `Accepted`, `Rejected`, `Superseded` のいずれかを明示する
- 実験値は必要最小限の根拠だけを残し、詳細ログは bench 文書へ委譲する
- `Draft` は未解決の判断キューとして扱い、実装前の作業メモや仕様書代わりにしない
- `Accepted` は短い判断ログへ圧縮し、現在仕様の正本にはしない
- `Superseded` は置換先を明示し、旧本文を読む必要がない状態にする

### `docs/reference/`

- 現在の実装仕様
- as-is の挙動、モジュール責務、公開前提
- 変更履歴や試行錯誤ログは置かない
- 用語の正本は `docs/reference/terminology.md` に置く

### `docs/operations/`

- 現在有効な開発、テスト、ベンチマーク、運用手順
- 文書棚卸し手順は `docs/operations/docs-maintenance.md` に置く
- 過去の手順や試行錯誤ログは置かない

### `docs/operations/bench/`

- ベンチマークの実行方法
- artifact の読み方
- 内部検証ログ、比較履歴、棄却した仮説
- `vxx` 系の検証はここに置く

## Geology の運用

- `docs/research/procedural_tctonic_planets.md`
    - `Procedural Tectonic Planets` 論文の調査ノートとして扱う
- `docs/decisions/*`
    - Geology の新旧設計判断と Draft 検討を分けて置く
- `docs/operations/bench/geology/*`
    - Crust / Environment の内部診断、artifact の比較結果、旧 Geology の棚卸しを置く

## 作業ルール

1. 外部調査は `research` に書く
2. 追跡したい変更案は `decisions` に `Draft` としてまとめる
3. 採用・却下・置換した判断は同じ文書の `Status` を更新する
4. 実装後の仕様を `reference` に反映する
5. 実験ログと比較履歴は `operations/bench` に残す
6. `Accepted` にした decision は、判断、理由、正本への導線だけへ圧縮する

## 用語ルール

- 正規用語は `docs/reference/terminology.md` に集約する
- 同じ概念に新しい別名を作らない
- 文書本文では `benchmark` を基本形にし、`bench` はコマンド、パス、短い識別子に限る
- `test` は日常開発で回す自動確認、`gate` は pass / fail 判定、`validation` は妥当性確認の目的を指す
- 新しい文書を書く前に、既存用語で説明できるかを確認する

## 棚卸しルール

- `docs/decisions/` は判断と理由だけを残し、実装済み仕様は `docs/reference/` へ移す
- 現在有効な手順は `docs/operations/` へ移す
- 実験ログ、比較履歴、棄却した仮説は `docs/operations/bench/` へ移す
- 棚卸し手順は `docs/operations/docs-maintenance.md` に従う
- `Draft` には閉じる条件を置き、実装後に放置しない
- `Accepted` は長文化させず、仕様、手順、実験ログを含めない

## 退避ルール

- 大きな作り替え前は、旧系の dirty state を `WIP` で固定する
- 旧系の検証ログは `bench` 文書へ退避し、新設計文書へ直接持ち込まない
- 新設計は既存 decision を履歴ごと上書きせず、新しい `Draft` decision として起票する
