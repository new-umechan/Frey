# Frey

100days企画: day020-

地形や気候などの地理的な制約から、国家・戦争・言語圏の興亡までを因果的に生成する歴史シミュレータ

## Purpose
本プロジェクトの目的は、地理的な制約が、人類史にどのように影響するかの因果関係を、動的なシステムとして生成・観察できるようにすることだ。
環境決定論を完全に支持するわけではない。地形や気候はあくまで制約であり、その中で人間がどう動くかはseedを元にした乱数として表現する。Freyの目的は、地理と文明の相互作用を単一のシステムの中で因果的に追えるようにすることだ。「なぜここに大国が生まれたのか」「なぜこの民族は分断されたのか」という問いに対して、地形・気候・資源の視点から仮説を立て、検証できる場を作りたい。

## Design Philosophy

- 入力は文字列seed
- 神（ユーザー）の手を途中で加えられるように。
  介入logは保存しておく。
- 同じseedとパラメータなら、だいたい同じ世界を再現。
  ある程度の揺らぎは許容するが、マクロな構造（大陸配置・河川系・文明分布）は再現したい

## Docs

docs/README.mdに仕様の全体像をメモ

## Teck Stack

- Web + WASM
- Rust: 計算コア
- Vite: 開発サーバー
- JavaScript（レンダリングとUI）
- Three.js（現状の描画）

## Development

`npm run dev`で開発サバーを起動できる。
開発中に`rust/`を編集するとWASMを自動で再ビルドしてVite画面に反映される。
`config/geology.yaml`編集時は地形パラメータを同期し、必要な再ビルドが走る。
`config/runtime.yaml`編集時はランタイム制御パラメータを同期し、Vite画面へ反映される。
Perf BenchはデフォルトOFF。`?perf=1`（または`?bench=1`）付きURLで有効化できる。

## Benchmark Data

Climate単体ベンチ（Phase 2）では、外部実データを使用する。

- DEM（`geology.height` 用。ETOPO 2022 **Ice Surface** 推奨）
- WorldClim v2.1（`temperature` / `precipitation`）
- ERA5-Land monthly means（`runoff` / `evapotranspiration`）
- CGIAR Aridity Index（`aridity`）

生データはGit管理しない方針。配置先と生成物は以下。

- 生データ: `data/raw/`
- 地形生データ: `data/raw/geology/`
- ベンチ用キャッシュ: `bench/data/`
- 地形キャッシュ: `bench/data/terrain_ref.bin`
- ベンチ実行ログ: `bench/results/`
- Climate Phase 2生スコア: `bench/results/climate_phase2_scores.jsonl`（実行ごとに自動追記）

ERA5-Land（`runoff` / `evapotranspiration`）は次で取得・整形できる。

1. `npm run bench:fetch:era5`
2. `npm run bench:prepare:era5`
