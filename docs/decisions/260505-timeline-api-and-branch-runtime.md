# Timeline API / Branch Runtime の正本化

## Status

Superseded by `260505-single-timeline-runtime-without-branch.md`

## Context

逆再生前提の設計に寄せるには、
時間操作を単なる補助 API ではなく timeline runtime の正本として扱う必要がある。

現行は rewind / seek / fork を持つが、
branch metadata、cursor、retention policy、tick boundary model が公開面に揃っていない。

## Decision

- timeline 操作の正本 API は `advance / rewind / seek / fork` とする
- `TimelineRuntime` は `TimelineBranch`、`TimelineCursor`、`TimelineRetentionPolicy` を持つ
- branch id は world id と別に発番し、fork lineage を保持する
- `tick 完了境界` を時間操作の公開整合点とする
- `river_downstream` は専用 compact undo patch で巻き戻し可能にする
- UI / worker 向けに `get_timeline_state` を追加する

## Consequences

利点:

- 逆再生と branch 分岐の設計責務が runtime に寄る
- UI / worker が「現在値の world」ではなく「timeline cursor」を扱える
- hydrology の複合構造でも短距離 rewind の full clone 発生率を下げられる

コスト:

- DTO と WASM API の更新範囲が広い
- retention / branch metadata の維持が追加コストになる

## Notes

今回の decision は timeline 公開面と runtime 責務の正本化であり、
長期的なメモリ上限制御を完全に解決するものではない。
byte-based retention は将来拡張として残すが、今回は policy と観測面の導入までを含む。
