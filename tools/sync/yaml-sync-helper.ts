import fs from "node:fs";
import path from "node:path";

export type TypeHint = "f32" | "u32";

export interface ParamEntry {
    yamlPath: string;
    fieldName: string;
    type: TypeHint;
    value: number;
    raw: string;
}

export type ParsedYaml = Map<string, ParamEntry>;

/**
 * Parse a self-describing YAML config file.
 * Expected format (flat):
 *   lapse_rate_c_per_km:
 *     type: f32
 *     value: 6.5
 *   precip_min_mm:
 *     type: f32
 *     value: 25.0
 */
export function parseSelfDescribingYaml(text: string): ParsedYaml {
    const result = new Map<string, ParamEntry>();
    const lines = text.split(/\r?\n/);

    // For flat format, we expect top-level keys with type/value children
    let currentParamName: string | null = null;
    let currentType: TypeHint | null = null;
    let currentValue: number | null = null;
    let currentRaw: string | null = null;

    function flushParam() {
        if (currentParamName !== null && currentType !== null && currentValue !== null) {
            result.set(currentParamName, {
                yamlPath: currentParamName,
                fieldName: currentParamName,
                type: currentType,
                value: currentValue,
                raw: currentRaw!,
            });
        }
    }

    for (let lineIdx = 0; lineIdx < lines.length; lineIdx++) {
        const line = lines[lineIdx];
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith("#")) {
            continue;
        }
        if (/\t/.test(line)) {
            throw new Error(`Invalid YAML line ${lineIdx + 1}: tab indentation is not allowed`);
        }

        const colonIndex = line.indexOf(":");
        if (colonIndex < 0) {
            throw new Error(`Invalid YAML line ${lineIdx + 1}: missing ":"`);
        }

        const indent = line.length - line.trimStart().length;
        if (indent % 2 !== 0) {
            throw new Error(`Invalid YAML line ${lineIdx + 1}: indentation must use 2-space units`);
        }

        const localKey = line.slice(0, colonIndex).trim();
        const rawValue = stripInlineComment(line.slice(colonIndex + 1));

        if (!localKey) {
            throw new Error(`Invalid YAML line ${lineIdx + 1}: empty key`);
        }

        if (indent === 0) {
            // Top-level key = parameter name
            flushParam();
            currentParamName = localKey;
            currentType = null;
            currentValue = null;
            currentRaw = null;
            continue;
        }

        if (indent === 2) {
            // Child key = type or value
            if (localKey === "type") {
                if (rawValue !== "f32" && rawValue !== "u32") {
                    throw new Error(`Invalid type "${rawValue}" at line ${lineIdx + 1}: expected "f32" or "u32"`);
                }
                currentType = rawValue as TypeHint;
            } else if (localKey === "value") {
                const isNumberLiteral = /^-?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/.test(rawValue);
                if (!isNumberLiteral) {
                    throw new Error(`Invalid value "${rawValue}" at line ${lineIdx + 1}: expected numeric scalar`);
                }
                const num = Number(rawValue);
                if (!Number.isFinite(num)) {
                    throw new Error(`Invalid numeric value at line ${lineIdx + 1}`);
                }
                currentValue = num;
                currentRaw = rawValue;
            }
        }
    }

    flushParam();
    return result;
}

export function stripInlineComment(value: string): string {
    let inSingle = false;
    let inDouble = false;

    for (let i = 0; i < value.length; i += 1) {
        const ch = value[i];
        if (ch === "'" && !inDouble) {
            inSingle = !inSingle;
            continue;
        }
        if (ch === "\"" && !inSingle) {
            inDouble = !inDouble;
            continue;
        }
        if (ch === "#" && !inSingle && !inDouble) {
            return value.slice(0, i).trim();
        }
    }

    return value.trim();
}

export function validateParams(parsed: ParsedYaml): void {
    for (const [path, entry] of parsed) {
        if (entry.type === "u32" && !Number.isInteger(entry.value)) {
            throw new Error(`Key "${path}" must be an integer for type u32`);
        }
        if (entry.type === "u32" && entry.value < 0) {
            throw new Error(`Key "${path}" must be non-negative for type u32`);
        }
    }
}

export function ensureParentDir(filePath: string): void {
    fs.mkdirSync(path.dirname(filePath), { recursive: true });
}

export function writeFileIfChanged(filePath: string, nextContent: string): boolean {
    const currentContent = fs.existsSync(filePath) ? fs.readFileSync(filePath, "utf8") : null;
    if (currentContent === nextContent) {
        return false;
    }
    ensureParentDir(filePath);
    fs.writeFileSync(filePath, nextContent);
    return true;
}

export function rustLiteral(raw: string, typeHint: TypeHint): string {
    if (typeHint === "f32") {
        return `${raw}f32`;
    }
    if (typeHint === "u32") {
        const parsed = Number(raw);
        if (!Number.isInteger(parsed) || parsed < 0) {
            throw new Error(`Expected unsigned integer but got "${raw}"`);
        }
        return `${parsed}`;
    }
    return raw;
}
