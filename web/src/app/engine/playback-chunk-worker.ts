import { decodePlaybackChunk } from "./playback-chunk-codec";

interface DecodeRequest {
    id: number;
    kind: "decode";
    payload: ArrayBuffer;
}

interface PlaybackWorkerScope {
    onmessage: ((event: MessageEvent<DecodeRequest>) => void) | null;
    postMessage: (message: unknown, transfer?: Transferable[]) => void;
}

const workerScope = self as unknown as PlaybackWorkerScope;

workerScope.onmessage = async (event: MessageEvent<DecodeRequest>) => {
    const request = event.data;
    try {
        const payload = await decodePlaybackChunk(request.payload);
        const transferables: Transferable[] = [];
        for (const field of payload.delta.deltas) {
            const data = field.f32_data ?? field.u32_data ?? field.i32_data;
            if (data instanceof Float32Array || data instanceof Uint32Array || data instanceof Int32Array) {
                transferables.push(data.buffer);
            }
            if (field.dirty_bitmap instanceof Uint32Array) {
                transferables.push(field.dirty_bitmap.buffer);
            }
        }
        workerScope.postMessage({ id: request.id, ok: true, payload }, transferables);
    } catch (error) {
        workerScope.postMessage({
            id: request.id,
            ok: false,
            error: error instanceof Error ? error.message : String(error),
        });
    }
};
