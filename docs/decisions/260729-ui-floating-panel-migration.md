# UI: fixed right sidebar → floating panels

## Status

Draft

閉じる条件: 右 `.sidebar` を撤去し、その機能がフローティングパネル/メタ表示へ
移設され、`pnpm dev:wasm` で各機能が動作確認できたとき Accepted にする。

## Context

Frey の UI を Figma のダーク v2 デザイン(310-51)へ全面移行している。左アイコン
サイドバー・レイヤーパネル・因果パネルは実装済み。残る大きな構造差は、**現状の
固定右サイドバー(256px)を廃止し、アイコンボタンから開くフローティングパネルへ
機能を移す**こと。

Figma は固定右サイドバーを持たず、代わりに: レイヤーパネル(357-95)、因果パネル
(357-96)、右上メタ(357-94)、下部プレイバック(310-56)を使う。

## 現状の右サイドバー `.sidebar` の中身

すべて id で TS から参照されるため、撤去時は **id を保ったまま移設**する(または
参照側を更新する)。

- Status: `#status-era`(時代)、`#era-scale-tick-label`(1Tick)、`#status-message`、
  隠し `#stat-level/seed/plates/land`
- Setting: `#debug-mode-toggle`、`#seed-form` / `#seed-input`
- View: `#view-cui-context`、`#view-cui` / `#view-cui-options`(動的なビュー/レイヤー選択)、
  隠し view-mode ラジオ
- History: `#event-log-list`
- 隠しデータ: `#era-scale-select`、`#era-weight-*`

## 移行マッピング

| 現状 | 移行先 | 状態 |
| --- | --- | --- |
| 時代 / 1Tick | 右上メタ 357-94 | フレームあり |
| seed 表示 | 右上メタ 357-94 | 表示のみ |
| ビュー/レイヤー選択 `#view-cui` | レイヤーパネル 357-95 | フレームあり(動的化が要る) |
| 気候凡例 | レイヤーパネル 357-95 | ライブ凡例データの配線が要る |
| イベントログ `#event-log-list` | 履歴パネル(履歴ボタンで開く) | フレーム無し → 私が起こす |
| seed 入力(変更) | 右上メタ 357-94(クリックで編集) | 決定 |
| debug 切替 | **撤去**(付随コード経路ごと) | 決定 |
| 隠しデータ(stats / era-weight / era-scale / view-mode) | 隠しコンテナへ退避(id 保持) | - |

## 決定(2026-07-29)

1. **seed**: 右上メタに表示し、**クリックすると入力に切り替わる**(inline 編集)。
2. **debug**: `#debug-mode-toggle` とその付随コード経路をすべて撤去する。
3. **履歴パネル**: 専用 Figma フレームは無い。layer/causal と同じ器(v2-panel)で私が起こし、
   履歴ボタンで開く。

## 実装順(各段でアプリを壊さない)

1. **右上メタ(357-94)** を作り、時代 / 1Tick / seed を移設。seed はクリックで inline 編集。
2. **下部プレイバック(310-56)** を Figma 化(既存 `playback-overlay` を流用)。
3. **レイヤーパネルの動的化**。`#view-cui` のレイヤー選択と気候凡例をレイヤーパネルへ
   移設(id 保持)。
4. **履歴パネル**(v2-panel の器)を作り、`#event-log-list` を移設。履歴ボタンで開く。
5. **debug 撤去**。`#debug-mode-toggle`、`toggle-row`、関連する TS のデバッグ経路を削除。
6. **右 `.sidebar` を撤去**。残った隠しデータは隠しコンテナへ退避(id 保持)。
   `app-shell` の grid を単一カラムへ(`--sidebar-width` 列を除去)。掃除。
7. **サイドバー背景(310-127)** の差分(枠線・色)を確認して当てる。

各段の後に `pnpm dev:wasm` でスクショ確認。id を保つため、移設は要素の DOM 位置を
移すだけにし、参照側 TS は極力触らない。

## Consequences

- `app-shell` の grid レイアウト変更(右列削除)。`viewport-panel` が全幅になる。
- 現状 sidebar 前提の CSS(`layout.css` の `.sidebar` 系、`panels.css` の各パネル)は
  撤去または移設先向けに整理。
- 移設先が id を保てば、既存のコントローラ(view-cui, event-log, seed-form, status)は
  そのまま動く見込み。

## Non-goals

- パネルの中身の機能(explain_cell グラフ描画、ライブ凡例)の作り込みは、それぞれの
  段で必要最小限にし、深掘りは別途。
