# 二層 playback archive と優先付き時系列ストリーム

## Status

Draft

## Close when

exact playback chunk のbinary/zstd wire format、client-side ring buffer、最新jumpを優先するcancel protocolを実装し、
低帯域の連続再生でHTTP tick requestとの競合を除去したら `Accepted` に更新する。空間LOD preview assetは同じprotocol上の
次段階として設計を確定し、renderer対応まで実装できない場合は別Draftへ分離する。

## Context

現在のstoreは64 tickごとのfull keyframeと毎tickのzstd deltaを保存している。しかしserverはdeltaをmaterializeして
JSON APIへ展開し、通常表示の4 fieldだけでも約1.11 MB/tickを送る。tick 58の保存済みzstd deltaは全fieldで約425 KBであり、
wire JSONが大きな増幅要因になっている。

440 KbpsではJSON delta一つに約20秒かかる。通常再生、近傍cache、遠方jump keyframeを同じ優先度で流すと、
background transferが再生を妨害する。bufferは短期的な帯域変動を隠せるが、平均帯域が必要bitrateを下回る場合には、
低詳細表示または再生速度制御が必要である。

## Decision

- public wire formatをinternal bincodeから分離した `PlaybackChunk v1` とする。WebSocket binary frameは小さなheaderと
  zstd圧縮payloadで構成し、browser Workerが `DecompressionStream("zstd")` をfeature detectして展開する。
- exact playback chunkはfield groupごとに最大4 tickを含む。first tickはclientが既に表示しているfull stateの次とし、
  通常再生でfull anchorを再送しない。
- serverはJSON cursorを毎tick更新する代わりにread-only archive chunkを送る。clientがdecoded deltaを順に適用して
  rendererを更新し、seek/field/metricsの整合はjump時・停止時・定期同期時にHTTPで確定する。
- clientは最低bufferと目標bufferをtick数で管理する。低水位では新規jump previewを優先し、bufferを消費し尽くした場合は
  tickを進めず再bufferする。
- commandはepochを持つ。jumpはepochを進め、serverとclientは古いepochのqueue、frame、decode結果を破棄する。
- navigation previewはexact laneとは別laneとする。v2では遠方tickの表示主要fieldをicosphereの4 level低い親meshへ
  投影して `PlaybackChunk` で送る。clientは最近傍親cellでfull meshへ復元し、exact stateへの到達を待つ間だけ表示する。
  この補間はdisplay-only近似であり、科学model stateやexact metricsには使わない。
- browserがzstd decompressionを提供しない場合、既存HTTP JSON APIへfallbackする。

## Trade-off

- zstdはbrowser supportをfeature detectする。未対応browserはarchive streamを使わないため、互換性を保てる。
- chunkを小さくするとjumpの割込みは速くなるが、headerとcompression frameのoverheadは増える。v1は4 tickを上限とし、
  送信queueにはcurrent playbackと最新jumpのみを置く。
- client-side delta適用はsimulation再実行ではない。main threadを止めないためにWorkerでdecodeし、typed array更新は既存の
  delta適用規則と一致させる。
- exact binary化だけでは440 Kbpsで高frequency再生を保証しない。空間LOD previewはjumpの初期表示を軽くするが、平均帯域が
  exact laneの必要量を下回る場合はadaptive playbackが必要である。

## Validation plan

- Rustでchunk encode/decode、field filter、epoch queueの優先順を検証する。
- Webでbinary decode、range/bitmap適用、古いepoch破棄、buffer水位の挙動を検証する。
- level 6 alphaでJSON delta、binary raw、zstd chunkのサイズとdecode時間を記録する。
