import { type FieldKind } from "./constants";

interface FieldResponse {
    f32_data?: Float32Array;
    i32_data?: Int32Array;
    u32_data?: Uint32Array;
}

interface FieldIoController {
    get_field: (worldId: string, fieldKind: FieldKind, mode: number) => FieldResponse;
    set_field_f32: (worldId: string, fieldKind: FieldKind, buffer: Float32Array) => void;
    set_field_i32: (worldId: string, fieldKind: FieldKind, buffer: Int32Array) => void;
    set_field_u32: (worldId: string, fieldKind: FieldKind, buffer: Uint32Array) => void;
}

type NumericTypedArray = Float32Array | Int32Array | Uint32Array;

export function readFieldToBuffer(
    controller: FieldIoController,
    worldId: string,
    fieldKind: FieldKind,
    target: NumericTypedArray,
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
        target.set(source.subarray(0, copyLength), 0);
    }
    return copyLength > 0;
}

export function writeBufferToField(
    controller: FieldIoController,
    worldId: string,
    fieldKind: FieldKind,
    buffer: NumericTypedArray
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
