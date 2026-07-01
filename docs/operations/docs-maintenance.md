# Documentation Maintenance

本書は Frey の文書を棚卸しするときの手順である。
現在有効な分類ルールは `docs/README.md`、用語の正本は `docs/reference/terminology.md` を参照する。

## 目的

- `docs/decisions/` が未整理の設計メモ置き場になることを防ぐ
- 同じ概念に複数の名前が付くことを防ぐ
- 人間と AI が同じ分類基準で文書を更新できるようにする

## decision の棚卸し

`docs/decisions/` の文書は、次のどれかの状態を必ず持つ。

- `Draft`: 未採用、検討中
- `Accepted`: 採用済み
- `Rejected`: 不採用
- `Superseded`: 別 decision または reference に置換済み

`Draft` は未解決の判断キューであり、AI や開発者の作業メモ置き場ではない。
実装済み仕様の正本は `docs/reference/`、現在手順は `docs/operations/`、
実験ログと比較履歴は `docs/operations/bench/` に置く。

棚卸し時は次の順で見る。

1. `## Status` がない文書を見つける
2. 実装済みで正本化されている内容は `Accepted` または `Superseded` にする
3. 実装済み仕様は `docs/reference/` に移す
4. 現在の手順は `docs/operations/` に移す
5. 実験ログ、比較履歴、棄却仮説は `docs/operations/bench/` に移す
6. decision には判断、理由、結果だけを残す

削除ではなく、まず状態更新と正本への昇格を優先する。
`Accepted` は削除せず、短い判断ログへ圧縮する。
圧縮後の decision には、長い仕様、手順、実験ログ、作業タスクを残さない。
重複した Draft が複数ある場合は、採用候補を 1 つに寄せ、残りを `Superseded` にする。

## decision の寿命

### Draft

`Draft` は実装前に合意したい未解決判断だけに使う。
各 `Draft` には `## Close when` を置き、どうなったら閉じるかを明示する。

`Draft` に置いてよいもの:

- 複数案から選ぶための判断軸
- 科学モデルの近似や trade-off
- 既存仕様を大きく変える前提
- 複数モジュールにまたがる実装前の合意事項

`Draft` に置かないもの:

- 実装しながら考えるための作業メモ
- AI へのタスクリスト
- 調査ログ
- benchmark の試行錯誤
- 実装済み仕様
- 現在有効な手順

### Accepted

`Accepted` は現在仕様の正本ではない。
採用後は判断ログへ圧縮し、詳細は `reference`、`operations`、`operations/bench` へ移す。
本文はおおむね 350 words 以下に保つ。

残す情報:

- 採用した判断
- 採用理由
- 重要な却下案や制約
- 正本文書への導線

移す情報:

- 実装済み仕様
- 実行手順
- 詳細な実験値
- 比較履歴
- 作業タスク一覧

### Superseded

`Superseded` は旧本文を読む必要がない状態にする。
`Superseded by` または `## Superseded By` で置換先を明示する。

## decision を増やす基準

次の場合だけ decision を追加する。

- 採用しない選択肢を後で説明する必要がある
- 既存仕様を大きく変える
- 科学モデルの近似や trade-off を明示する必要がある
- 複数案のうち、なぜその案を選ぶかを残す必要がある

次の場合は decision を追加しない。

- 小さなリファクタ
- test だけの追加や修正
- 命名修正
- 既存 reference に沿った実装
- 現在手順の更新だけで説明できる変更

## 用語の棚卸し

同じ意味の用語が増えた場合は、まず `docs/reference/terminology.md` に寄せる。

優先順位:

1. 文書本文では正規用語を使う
2. コマンド、パス、ファイル名では既存名を維持する
3. 古い用語を残す場合は、初出で正規用語との関係を書く
4. 新しい略称は、既存の略称と衝突しない場合だけ使う

`test`, `gate`, `benchmark`, `bench`, `validation` は混同しやすいため、
新しい文書を書く前に `docs/reference/terminology.md` の定義を確認する。

## AI への依頼時の注意

AI に文書整理を依頼するときは、次を明示する。

- 正本を更新するのか、棚卸し案だけを作るのか
- decision を新規追加してよいか
- 古い文書を `Superseded` にしてよいか
- 用語の機械置換を許すか、文脈ごとに確認するか

大量の文書移動や状態変更は、先に対象一覧を作ってから実施する。
