# Architecture

## 設計思想

歴史シミュレータの設計軸は、「何を扱うか」ではなく「状態の変化速度」とする。

遅いものは速いものの条件を決める。
速いものは遅いものを少しずつ侵食する。
この非対称な双方向関係を、個別の特例ロジックではなく、コード構造そのものに埋め込む。

この方針により、文明が環境を破壊して自滅する、気候変動が国家を崩す、といった現象を自然に表現する。

## 全体構成

ECSアーキテクチャを採用する。

```text
Simulation
├── CellStore         （全セルのComponent群、SoA配列）
├── hecs::World       （Polity・Settlement・Region等の疎なEntity）
├── polity_relations  （国家間の二者間関係）
├── polity_groups     （経済圏・軍事同盟・文化宗教圏などのグループ）
├── clock             （tick・epoch・予算）
├── feedback          （FeedbackQueue）
└── archive           （履歴・スナップショット）
```

`hecs::World` はクレート名で修飾することで `Simulation` との名前の衝突を避ける。

### Tier 1 System一覧（更新順）

```text
ExecSystem
├── GeologySystem
├── ClimateSystem
├── HydrologySystem
├── EcologySystem
├── DomesticatesSystem
├── SubsistenceSystem
├── PopulationSystem
├── SettlementSystem
├── PolitySystem
└── ConflictSystem
```

`ExecSystem` はtick進行、予算配分、時代遷移、履歴、FeedbackQueueの一括適用を担当する。

---

## ECSアーキテクチャの採用

### 採用理由

このシミュレータの処理の本質は「全セル（約4万）に対して、同じ計算を一斉に適用する」ことである。
ECSのComponent-per-array構造（SoA）はこのパターンに適合し、CPUキャッシュ効率を最大化する。

また、Tier2モジュール追加時にSystemとComponentを登録するだけで拡張できるため、
複雑性の増加に対してアーキテクチャが崩れにくい。

### セルと非セルEntityの分離

セルと非セルEntityでは性質が異なるため、管理方法を分ける。

- `CellStore`（自前SoA）
  - 全セルの現在値Componentを保持する
- `hecs::World`（疎なEntity）
  - Polity・Settlement・Regionなど、動的に生滅するEntityを保持する
- `polity_relations`（国家間関係）
  - 国家間の重み付き関係を保持する

データ配置と型定義の詳細は `docs/architecture/data_model.md` を参照。

---

## Systemの原則

Systemはステートレスな関数として実装する。
CellStore・hecs::World・Clock・FeedbackQueue（必要に応じてArchive）を引数として受け取り、次の状態を書き戻す。

同一tick内の依存はDAGで順序を保証し、逆方向の影響はFeedbackQueueで次tickへ遅延させる。
更新順序と時代制御の詳細は `docs/architecture/phase_control.md` を参照。

各Systemの読み書き境界（Read/Write/Do-not-write）は `docs/architecture/module_boundaries.md` を参照。

---

## 関連文書

- 時代・tick・予算・遷移
  - `docs/architecture/phase_control.md`
- CellStore・hecs::World・Clock・FeedbackQueue・Archiveの構造と型定義
  - `docs/architecture/data_model.md`
- 各Systemが何を読み、何を書くか
  - `docs/architecture/module_boundaries.md`
