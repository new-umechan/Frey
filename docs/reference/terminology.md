# Terminology

本書は Frey の文書と実装で使う用語の正本である。
新しい独自用語を追加する前に、既存用語で説明できないかを確認する。

## 品質確認

### test

日常開発で回す自動確認を指す。

使う場面:

- Rust unit test
- WASM API test
- seed 回帰 gate
- PR 前の最低限確認

書き方:

- コマンド名が `test:*` のものは test と呼ぶ
- 「検証」一般の意味で test を使わない
- 重い科学比較や長期 artifact 比較は test と呼ばない

### gate

pass / fail で変更の受け入れ可否を決める自動判定を指す。

使う場面:

- unit test の失敗
- seed 回帰の閾値超過
- perf gate の閾値超過
- CI で落とす判定

書き方:

- 判定基準が明確なものだけ gate と呼ぶ
- 人間が結果を読んで判断するものは gate と呼ばない

### benchmark

現実データ、参照 artifact、過去 artifact と比較してモデルの性質を読む重い評価を指す。

使う場面:

- Climate / Hydrology / Geology / Ecology / Domesticates / Glaciology の単体評価
- Full pipeline や era transition の統合評価
- モデル変更やパラメータ調整の判断材料

書き方:

- 文書本文では benchmark を基本形にする
- コマンド、ディレクトリ、ファイル名では既存の `bench` を使ってよい
- benchmark は原則として quality gate ではない

### bench

コマンド名、ディレクトリ名、短い識別子で使う略称である。

使う場面:

- `pnpm bench:*`
- `docs/operations/bench/`
- `benches/results/`
- Rust の `cargo bench`

書き方:

- 説明文では benchmark と書く
- 固有名詞やパスでは bench のままにする

### validation

モデル契約や科学的仮説が、参照データや診断指標と矛盾していないかを確認する行為を指す。
validation は test や benchmark の上位語ではなく、目的を表す語である。

使う場面:

- 科学モデルの妥当性確認
- 地形・水文・気候などの model contract 確認
- 実データとの整合性を読む診断文脈

書き方:

- 汎用の「動作確認」を validation と呼ばない
- pass / fail の自動判定は gate と呼ぶ
- 実行単位や artifact 比較は benchmark と呼び、目的が妥当性確認である場合だけ validation と説明する

## 文書分類

### concept

背景、設計思想、読み方を説明する文書を指す。
現在の仕様の正本ではない。

### reference

実装済み仕様の正本を指す。
履歴、検討中の案、実験ログを置かない。

### operation

現在有効な手順を指す。
過去の手順や試行錯誤は置かない。

### decision

採用、却下、保留、置換した重要判断と理由を指す。
実装済み仕様の正本ではない。

### research

外部文献、調査メモ、採用前の材料を指す。
実装済み仕様の正本ではない。

## 命名ルール

- 新しい用語は、まず本書に追加する
- 同じ概念に別名を作らない
- 略称はコマンド、パス、表の列名など短さが必要な場所に限る
- 文書本文では、略称より正規用語を優先する
- AI に作業を依頼するときは、本書の用語を使う
