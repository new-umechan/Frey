# モジュール境界

## 目的

この文書は、各モジュールが何を読み、何を書き、何を書かないかを定義する。
ここで扱う共有面は `CellStore` と `hecs::World`、進行管理入力は `Clock` と `FeedbackQueue` である。
擬似コードをpythonで記述しているが、これはrustで書くと長くなってしまい、要件定義書として不適だったためだ。

この文書が定義するのは `Module` 境界であり、`System` 境界ではない。
`Module` は同一、または非常に近い内容を読み書きする `System` の束ね単位で、ECS実装都合とは独立した設計上の区分である。

各モジュールは他モジュールへ直接依存しない。
モジュール間の共有面は `CellStore` および `hecs::World` である。
進行管理入力は `Clock` と `FeedbackQueue` である。

## 現状
Tier1までのモジュールについて、詳細を決定している。

---

## モジュール一覧

### Tier 1（必須）

| モジュール | 概要 |
| --- | --- |
| `Geology` | 地形変化（標高・プレート更新） |
| `Climate` | 降水・気温・水循環 |
| `Hydrology` | 流路・流量・集積、侵食・堆積率計算 |
| `Ecology` | 植生 |
| `Domesticates` | 作物・家畜の分布 |
| `Subsistence` | 居住適性・地域ごとの生業構成 |
| `Population` | 人口変動 |
| `Settlement` | 集落・都市形成 |
| `Polity` | 国家・領域変化 |
| `Conflict` | 戦争・境界変化 |

### Tier 2（粗いモデルで可）

| モジュール | 概要 |
| --- | --- |
| `Disease` | 感染拡大・人口への影響 |
| `Resources` | 資源埋蔵・採掘・枯渇 |
| `Trade` | 地域間交換・交易流量 |
| `Technology` | 技術水準更新 |
| `Infrastructure` | 地形書き換え能力 |

### Tier 3（スコープ外）

| モジュール | 概要 |
| --- | --- |
| `Institutions` | 制度（属性として保持のみ） |

---

## tick内依存（UPDATE_DAG）

```python
UPDATE_DAG = {
    Geology:      [],
    Climate:      [Geology],
    Hydrology:    [Geology, Climate],
    Ecology:      [Geology, Climate, Hydrology],
    Domesticates: [Geology, Climate, Hydrology, Ecology],
    Subsistence:  [Geology, Hydrology, Ecology, Domesticates],
    Population:   [Subsistence, Ecology],
    Settlement:   [Population, Subsistence, Hydrology, Geology],
    Polity:       [Settlement, Population],
    Conflict:     [Polity, Population],
}
```

## フィードバック（FEEDBACK_EDGES）

逆方向の影響は次tickへ遅延させる。

```python
FEEDBACK_EDGES = {
    # 文明→環境・文明内逆向き
    Conflict:    [Geology, Hydrology, Ecology, Population, Settlement, Polity],
    Population:  [Ecology],
    Subsistence: [Ecology, Hydrology],
    Polity:      [Settlement, Domesticates],
    Settlement:  [Domesticates],  # 隣接地域への作物・家畜拡散を含む
}
```

---

以下の内容は境界定義の要約である。
データ構造の正本は `docs/architecture/data_model.md`、更新順序と適用タイミングの正本は `docs/architecture/phase_control.md` を参照する。
詳細なドメイン仕様は `docs/modules/` 配下を参照する。

### 記載ルール（詳細要件が確定した項目）

- 詳細要件が確定したモジュールでは、概念語に加えて具体的な変数名を併記する。
- 変数名は `docs/modules/` 配下の定義を正本として採用する。
- `rust/` 配下の実装は、境界定義の変数名決定の根拠にしない。
- 詳細要件が未確定のモジュールは、従来どおり概念語のみでもよい。

## `Geology`

### 読むもの

- 標高
- プレートID
- 流出量 ← `Climate` が書く
- 侵食量・堆積量（`erosion_rate`、`deposition_rate`）← `Hydrology` が書く
- FeedbackQueue（`Conflict` による焦土・地形破壊）

### 書くもの

- 標高
- プレートID

### 書かないもの

- 流路・流量（`Hydrology` に移管）
- 降水・気温
- 植生

### 補足

地形を書き換える責任は `Geology` に一本化する。
`Hydrology` 切り出し以前は流路・流量も担当していたが、v2では `Hydrology` に移管する。

---

## `Climate`

### 読むもの

- 標高（`geology.height`）
- 固定地理量（`geo.latitude_deg`、`geo.distance_from_ocean_km`、`geo.coast_side`、`geo.is_coastal`）
- 植生密度（`ecology.tree_cover`、`ecology.ground_cover` から算出）
- `Clock`

### 書くもの

- 降水（`climate.precipitation`）
- 気温（`climate.temperature`）
- 実蒸発散量（`climate.evapotranspiration`）
- 流出量（`climate.runoff`）
- 乾燥指数（`climate.aridity`）
- 海水温（`climate.ocean_temperature`）

### 書かないもの

- 標高
- 侵食量・堆積量
- 流路・流量

### 補足

局所水収支までを担当する。流量の集積は `Hydrology` が引き受ける。

---

## `Hydrology`

Systemは2つに分かれる。

| System | 実行条件 |
| --- | --- |
| `HydrologyMFDSystem` | 地殻形成期・環境形成期は毎tick実行。先史期以降はExecSystemが地形変化フラグを検知したtickのみ実行 |
| `HydrologyFlowSystem` | 先史期以降、毎tick実行 |

地形変化フラグの判定はExecSystemが担う。GeologyはCellStoreに標高を書くだけであり、フラグ管理はしない。

### 読むもの

- 標高（`geology.height`）← `Geology` が書く
- 流出量（`climate.runoff`）← `Climate` が書く
- FeedbackQueue（`Subsistence`・`Settlement` による取水・ダム）

### 書くもの

- 流路・分配率（`river_downstream`）
- 流量（`river_flow`）
- 侵食量（`erosion_rate`）
- 堆積量（`deposition_rate`）
- 河川輸送コスト（`river_transport_cost`）
- 湖フラグ（`is_lake`）

### 書かないもの

- 降水・流出量
- 植生
- 標高（侵食・堆積率を書くのみ。標高への反映は `Geology` が行う）

### 補足

MFD（Multiple Flow Direction）を採用する。
流下先と分配率はペアで保持する（`SmallVec<[(CellId, f32); 3]>`）。
`river_upstream` は保持しない。流域の塗り分けが必要になった時点で再検討する。

窪地は湖（`is_lake=true`）として扱い、流量をそこで吸収する。
湖セルは隣接セルの中で最も低い鞍部を唯一の流下先として設定し、溢れた水を流下させる。

侵食・堆積率は `Hydrology` が計算して `CellStore` に書き、標高への最終反映は `Geology` が行う。
河川輸送コストは `Settlement` と `Trade` が読む。

---

## `Ecology`

### 読むもの

- 標高（`geology.height` / `GeoState.height`）
- 降水（`climate.precipitation` / `ClimateState.precipitation`）
- 気温（`climate.temperature` / `ClimateState.temperature`）
- 流量（`hydrology.river_flow` / `HydrologyState.river_flow`）
- 前tickまでの生態状態（`biome`、`tree_cover`、`ground_cover`、`disturbance`、`soil_fertility`）
- FeedbackQueue（`Population`・`Subsistence` による土地利用変化）

### 書くもの

- バイオーム（`biome`）
- 樹木被覆（`tree_cover`）
- 地被（`ground_cover`）
- 撹乱（`disturbance`）
- 土壌肥沃度（`soil_fertility`）

### 書かないもの

- 標高
- 流路

### 補足

環境応答を `CellStore` に書く。社会変化は直接扱わない。

---

## `Domesticates`

### 読むもの

- 標高
- 気温
- 降水
- 植生 ← `Ecology` が書く
- FeedbackQueue（`Settlement` 隣接地域からの拡散）

### 書くもの

- 作物栽培可能種（`crop_available`）— Domesticates内部専用。Subsistenceは読まない
- 作物普及度（`crop_adoption`）— 0.0〜1.0。Subsistenceが読む
- 家畜利用可能種（`livestock_available`）— Domesticates内部専用。Subsistenceは読まない
- 家畜普及度（`livestock_adoption`）— 0.0〜1.0。Subsistenceが読む

### 書かないもの

- 標高
- 気候属性
- 人口
- 国家

### 補足

伝播（隣接 `Settlement` からの拡散）はFeedbackQueue経由で次tickに適用する。
環境条件から栽培・利用可能かどうかを判定し、分布を更新する。

---

## `Subsistence`

### 読むもの

- 標高
- 流量
- 植生 ← `Ecology` が書く
- 作物・家畜普及度（`crop_adoption`、`livestock_adoption`）← `Domesticates` が書く
- 前tickまでの生業構成

### 書くもの

- 生業構成（`subsistence_mix`）
- 食料生産量（`food_production`）
- 淡水アクセス（`freshwater_access`）

### 書かないもの

- 人口（`Population` が読む値として提供するが、直接書かない）
- 標高
- 気候属性
- 国家
- 生産性（`food_production` で代替。独立列としては持たない）
- 土地利用（`SubsistenceMix` から導出可。独立列としては持たない）

### 補足

生産量と生業様式は別物として扱う。
生業構成（`SubsistenceMix`）の変化は環境条件と前tickの状態から決まり、転換には慣性がある。
`crop_adoption` が高い → 農耕転換の圧力が上がる → `farming` 比率が遅延して上昇 → `food_production` が跳ね上がる、という遅延と非線形性が「なぜここで文明が生まれたか」の表現に直結する。
`freshwater_access` は `river_flow`・`is_lake` から導出し、Population・Settlementが読む。
計算式の粒度（距離減衰の有無など）はドメイン仕様に委ねる。

---

## `Population`

### 読むもの

- 食料生産量（`food_production`）← `Subsistence` が書く
- 淡水アクセス（`freshwater_access`）← `Subsistence` が書く
- 前tickまでの人口（`population`）
- FeedbackQueue（`Conflict` による死亡率上昇・直接人口減）

### 書くもの

- 人口（`population`）
- 出生率（`birth_rate`）
- 死亡率（`death_rate`）

### 書かないもの

- `population_density`（`population` から導出可能なため列として持たない）
- `migration_pressure`（`Settlement` が内部計算で使用。CellStoreの列として持たない）
- 国家・領域

### 補足

`Conflict` からの干渉は2種類を使い分ける。
通常の戦闘による死者増は死亡率を上げる（`DeltaF32 { field: DeathRate, delta }`）。
大虐殺など単発の大量死は人口を直接削る（`DeltaF32 { field: Population, delta }`）。

`Disease`（Tier 2）が有効化された場合、死亡率への影響をFeedbackQueue経由で受け取る。

---

## `Settlement`

### 読むもの

- 人口（`population`）← `Population` が書く
- 出生率・死亡率（`birth_rate`、`death_rate`）← `Population` が書く
- 食料生産量・生業構成（`food_production`、`subsistence_mix`）← `Subsistence` が書く
- 淡水アクセス（`freshwater_access`）← `Subsistence` が書く
- 河川輸送コスト（`river_transport_cost`）← `Hydrology` が書く
- 標高・固定地理量（`geology.height`、`geo.is_coastal`）
- FeedbackQueue（`Polity` による遷都・強制移住、`Conflict` による都市破壊）

### 書くもの

- 人口（`population`）— 移動による社会増減を直接反映
- 都市化度（`urbanization`）

### 書かないもの

- `settlement_size`（列として持たない。Tier2の `Infrastructure` 有効化時に拡張ポイントとする）
- `migration_pressure`（Settlement内部で計算し、外部には公開しない）
- `centrality`（列として持たない。首都・拠点都市の選定は `Polity` が `urbanization` を読んで行う）
- `population_density`（`population` から導出）
- 国家・領域（`Polity` が書く）

### 補足

移動量の計算はSettlement内部で完結させる。
送り出し側（`food_production` が低い、`population` が高い等）と
受け入れ側（山地・砂漠は `food_production`・`freshwater_access` が低い）の両方を
既存の変数から判断するため、`migration_pressure` を中間値として保持する必要はない。

港市・河港・峠都市などの立地は、地形と河川輸送コストから `urbanization` の計算を通じて自然に決まる。
首都選定（`PolityComponent.capital_cell` の更新）は `Polity` の責務とする。

---

## `Polity`

### 読むもの

- 集落・都市分布 ← `Settlement` が書く
- 人口 ← `Population` が書く
- 前tickまでの国家状態
- polity_relations（同盟・宗主関係）
- polity_groups（所属グループ）
- FeedbackQueue（`Conflict` による領土変化）

### 書くもの

- 国家ID（`polity_id`）
- 領域（`CellStore.polity_id` が正本。`PolityComponent.cells_cache` は差分更新キャッシュ）
- 国家安定度（`legitimacy`・`centralization`・`military_tech`）
- 首都（`capital_cell`）
- polity_groups への加入・脱退・解散（FeedbackQueue経由で次tickに適用）

### 書かないもの

- 人口の直接更新
- 集落の直接更新
- 言語・文化圏（`language_group` はTier2へ移管）

### 補足

`legitimacy` と `centralization` の組み合わせで国家の拡大・崩壊・分裂のダイナミクスを表現する。
多民族構成（言語圏と国家境界の不一致）はTier2の `Language` モジュールが担う。
`cells_cache` の更新は `ExecSystem` が `changed_polity_cells` バッファ経由で差分管理する。

---

## `Conflict`

### 読むもの

- 国家ID・領域・安定度 ← `Polity` が書く
- 人口 ← `Population` が書く
- polity_relations（同盟・敵対・宗主関係）
- polity_groups（軍事同盟グループ）
- 前tickまでの戦争状態

### 書くもの

- 戦争状態
- 戦線位置（`frontline_pressure`）

### 書かないもの（FeedbackQueueに回すもの）

- 領土変化（→ `Polity` へ次tick）
- 人口減（→ `Population` へ次tick）
- 集落破壊（→ `Settlement` へ次tick）
- 地形破壊（→ `Geology`・`Hydrology`・`Ecology` へ次tick）

### 補足

`Conflict` の結果はすべてFeedbackQueue経由で次tickに適用する。
同一tick内で他モジュールを逆流更新しない。
