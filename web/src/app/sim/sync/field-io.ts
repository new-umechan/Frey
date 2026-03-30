import { type FieldKind } from "./constants";

interface FieldResponse {
    f32_data?: Float32Array;
    i32_data?: Int32Array;
    u32_data?: Uint32Array;
}

export function readFieldToBuffer(
    controller: any,
    worldId: string,
    fieldKind: FieldKind,
    target: Float32Array | Int32Array | Uint32Array,
    options: { mode?: number } = {}
): boolean {
    const mode = options.mode ?? 1;
    let response: FieldResponse | null = null;
    try {
        response = controller.get_field(worldId, fieldKind, mode);
    } catch {
        return false;
    }

    if (!response) {
        return false;
    }

    const source = response.f32_data ?? response.i32_data ?? response.u32_data;
    if (!source) {
        return false;
    }

    const copyLength = Math.min(target.length, source.length);
    if (copyLength > 0) {
        (target as any).set((source as any).subarray(0, copyLength));
    }
    return copyLength > 0;
}

export function writeBufferToField(
    controller: any,
    worldId: string,
    fieldKind: FieldKind,
    buffer: Float32Array | Int32Array | Uint32Array
): boolean {
    try {
        if (buffer instanceof Float32Array) {
            controller.set_field_f32(worldId, fieldKind, buffer);
        } else if (buffer instanceof Int32Array) {
            controller.set_field_i32(worldId, fieldKind, buffer);
        } else if (buffer instanceof Uint32Array) {
            controller.set_field_u32(worldId, fieldKind, buffer);
        } else {
            return false;
        }
        return true;
    } catch {
        return false;
    }
}
