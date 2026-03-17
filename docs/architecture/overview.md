# Architecture

## 設計思想

歴史シミュレータの設計軸は、「何を扱うか」ではなく「状態の変化速度」とする。

遅いものは速いものの条件を決める。
速いものは遅いものを少しずつ侵食する。
この非対称な双方向関係を、個別の特例ロジックではなく、コード構造そのものに埋め込む。

この方針により、文明が環境を破壊して自滅する、気候変動が国家を崩す、といった現象を自然に表現する。

## 全体構成

時代ごとの予算配分で活動量は変わるが、Tier 1の更新器は次の通りである。

```text
Exec
├── Geology
├── Climate
├── Hydrology
├── Ecology
├── Domesticates
├── Subsistence
├── Population
├── Settlement
├── Polity
└── Conflict
```

詳細は `docs/architecture/module_boundaries.md` を参照。

- `Exec`
  - tick進行、予算配分、時代遷移、履歴、`FeedbackQueue` の適用タイミング管理を担当する


## モジュール間通信の原則

モジュール間の直接依存は持たない。
すべてのモジュールは共有状態として `World State` と `Graph State` を読み書きする。
すべての更新器は進行管理入力として `Exec State` を参照する。
更新器はステートレスに保つ。

```text
┌─────────────────────────────┐  ┌─────────────────────────────┐
│         World State          │  │         Graph State          │
│  各セルが持つ属性の現在値    │  │  セルに還元できないグラフ    │
│ （標高・降水・植生・人口…）  │  │ （国家関係・交易網・伝播…）  │
└─────────────────────────────┘  └─────────────────────────────┘
         ↑読み書き↑                        ↑読み書き↑
┌──────┬──────┬──────┬──────┬──────┬──────┬──────┬──────┬──────┬──────┐
│Geo   │Cli   │Hyd   │Eco   │Dom   │Sub   │Pop   │Set   │Polity│Conf  │
│更新器│更新器│更新器│更新器│更新器│更新器│更新器│更新器│更新器│更新器│
└──────┴──────┴──────┴──────┴──────┴──────┴──────┴──────┴──────┴──────┘
                     ↑全更新器が参照↑
              ┌─────────────────────────┐
              │         Exec State       │
              │  tick・予算・FeedbackQueue│
              └─────────────────────────┘
```

ここでいう `World State` は、各セルが持つ現在値の集合である。
`Graph State` は、セル1件に還元しにくい関係構造（国家関係、交易網、伝播ネットワークなど）を保持する。
各モジュールは他モジュールの内部実装を知らず、`World State` と `Graph State` を共有面として使う。
`Exec State` は、`Tick.real_years`、`SubsystemBudgets`、現在の時代、`FeedbackQueue` などの進行管理情報を与える。

## 更新順序

同一tick内の依存はDAGで表し、順序を保証する。
同一tick内で循環依存は作らない。
フィードバックは次tick以降に遅延させる。
tick N+1 の開始時に `Exec` が `FeedbackQueue` を `World State` と `Graph State` に適用する

`docs/architecture/module_boundaries.md` を参照。

## 関連文書

- 時代、tick、予算、遷移
  - `docs/architecture/phase_control.md`
- `World State` / `Exec State` / `Graph State` の状態配置
  - `docs/architecture/data_model.md`
- 各モジュールが何を読み、何を書くか
  - `docs/architecture/module_boundaries.md`
