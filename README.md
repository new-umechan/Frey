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
- Typescript（レンダリングとUI）
- Three.js（描画）

## External Data

ベンチマーク実行では外部実データを使用する。生データの配置先は `benches/raw/`。

- ETOPO 2022 Ice Surface（`geology.height`、`benches/raw/geology/`）
- WorldClim v2.1（`temperature` / `precipitation`、`benches/raw/climate/`）
- ERA5-Land monthly means（`runoff` / `evapotranspiration`、`benches/raw/climate/`）
- CGIAR Global Aridity Index（`aridity`、`benches/raw/climate/`）
- GloFAS historical / ERA5系（`river_flow` 参照、`benches/raw/hydrology/`）
- HydroLAKES（`is_lake` 参照、`benches/raw/hydrology/`）
- MODIS MOD44B（`tree_cover` / `ground_cover` / `biome` 合成参照、`benches/raw/ecology/`）
- MODIS MCD12Q1（`natural_mask` / `biome` 合成参照、`benches/raw/ecology/`）
- SoilGrids（`soil_fertility` proxy、`benches/raw/ecology/soilgrids/`）
