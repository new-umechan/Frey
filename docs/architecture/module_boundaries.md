# モジュール境界

## 目的

この文書は、各モジュールが `World State` の何を読み、何を書き、何を書かないかを定義する。

各モジュールは他モジュールへ直接依存しない。
モジュール間の共有面は `World State` である。
進行管理入力は `Exec State` である。

## `Exec`

### 読むもの

- 現在の `World State`
- 現在の `Exec State`

### 書くもの

- 次tick
- 時代
- `SubsystemBudgets`
- `FeedbackQueue` の適用結果
- 履歴とスナップショット

### 書かないもの

- 地形、気候、生態、文明の各属性値そのもの

### 補足

`Exec` は進行管理だけを担当する。
`FeedbackQueue` を tick開始時に `World State` へ適用する責務も持つ。
個別の自然法則や社会法則は持たない。

## `Geology`

### 読むもの

- 標高
- プレートID
- 降水
- 流量
- 流域植生
- 文明による取水、ダム、汚染などの次tick向けフィードバック

### 書くもの

- 標高
- 侵食量
- 堆積量
- 流路
- 地形由来の境界条件

### 書かないもの

- 降水
- 気温
- 可住性
- 人口

### 補足

地形を書き換える責任は `Geology` に一本化する。
地殻形成期に `Climate` や `Ecology` が未有効な間は、降水に簡易な初期降水分布、流量に 0、流域植生に なし を既定値として使う。

## `Climate`

### 読むもの

- 標高
- 海陸分布
- 地形由来の境界条件
- 前tickまでの気候状態

### 書くもの

- 降水
- 気温
- 流量
- 水循環由来の環境条件

### 書かないもの

- 標高
- 侵食量
- 堆積量
- 人口

### 補足

`Climate` は地形を読むが、地形そのものは書き換えない。
地形変化は常に `Geology` が引き受ける。

## `Ecology`

### 読むもの

- 標高
- 降水
- 気温
- 流量
- 前tickまでの生態状態

### 書くもの

- 植生
- 可住性
- 生産性
- 流域植生との交換量

### 書かないもの

- 標高
- 流路
- 人口
- 国家

### 補足

`Ecology` は環境応答を `World State` に書く。
社会変化は直接扱わない。

## `Civilization`

### 読むもの

- 標高
- 降水
- 流量
- 可住性
- 生産性
- 前tickまでの文明状態

### 書くもの

- 人口
- 国家ID
- 農業状態
- 取水
- ダム
- 汚染
- `FeedbackQueue` への環境フィードバック要求

### 書かないもの

- 標高の直接更新
- 降水の直接更新
- 流路の直接更新

### 補足

`Civilization` は環境へ影響を与えうるが、その影響は次tickへ遅延させる。
tick N では `FeedbackQueue` に書き込むだけで、その場では適用しない。
同一tick内で `Geology` や `Climate` を逆流更新しない。

## tick内依存

同一tick内の依存は次で固定する。

```python
UPDATE_DAG = {
    Geology:      [],
    Climate:      [Geology],
    Ecology:      [Geology, Climate],
    Civilization: [Geology, Climate, Ecology],
}
```

## フィードバック

逆方向の影響は、同一tickではなく次tickへ遅延させる。

```python
FEEDBACK_EDGES = {
    Civilization: [Geology, Climate, Ecology],
}
```

処理は2段階に分ける。

- tick N で `Civilization` が `FeedbackQueue` に書く
- tick N+1 の開始時に `Exec` が `FeedbackQueue` を `World State` に適用する

これにより、依存グラフはDAGのまま保たれる。

## 河川の責務分担

河川は単独モジュールにしない。
`World State` 上の属性群として分担して扱う。

| モジュール | 河川に関する担当 |
| --- | --- |
| `Geology` | 流路の決定、侵食、堆積による地形書き換え |
| `Climate` | 降水量、流量 |
| `Ecology` | 流域植生との交換 |
| `Civilization` | 取水、ダム、汚染 |

流路計算は、標高を読んで流路グラフを返す純粋関数として切り出す。
