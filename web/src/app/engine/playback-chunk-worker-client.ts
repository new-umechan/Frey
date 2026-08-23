import type { DecodedPlaybackChunk } from "./playback-chunk-codec";

interface PendingDecode {
    resolve: (chunk: DecodedPlaybackChunk) => void;
    reject: (error: Error) => void;
}

export class PlaybackChunkWorkerClient {
    private readonly worker: Worker;
    private readonly pending = new Map<number, PendingDecode>();
    private nextId = 1;

    constructor() {
        this.worker = new Worker(new URL("./playback-chunk-worker.ts", import.meta.url), {
            type: "module",
        });
        this.worker.addEventListener("message", this.handleMessage);
        this.worker.addEventListener("error", this.handleError);
    }

    decode(payload: ArrayBuffer): Promise<DecodedPlaybackChunk> {
        const id = this.nextId;
        this.nextId += 1;
        return new Promise((resolve, reject) => {
            this.pending.set(id, { resolve, reject });
            this.worker.postMessage({ id, kind: "decode", payload }, [payload]);
        });
    }

    close() {
        this.worker.terminate();
        for (const pending of this.pending.values()) {
            pending.reject(new Error("playback decoder worker closed"));
        }
        this.pending.clear();
    }

    private handleMessage = (event: MessageEvent<{
        id: number;
        ok: boolean;
        payload?: DecodedPlaybackChunk;
        error?: string;
    }>) => {
        const response = event.data;
        const pending = this.pending.get(response.id);
        if (!pending) {
            return;
        }
        this.pending.delete(response.id);
        if (response.ok && response.payload) {
            pending.resolve(response.payload);
            return;
        }
        pending.reject(new Error(response.error ?? "failed to decode playback chunk"));
    };

    private handleError = (event: ErrorEvent) => {
        const error = new Error(event.message || "playback decoder worker failed");
        for (const pending of this.pending.values()) {
            pending.reject(error);
        }
        this.pending.clear();
    };
}
