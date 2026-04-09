import fs from "node:fs";
import path from "node:path";

export type SchemaEntry = [pathKey: string, outKey: string, typeHint?: string];

export type ParsedYaml = Map<string, { raw: string; value: number }>;

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

export function parseYamlNumericScalars(text: string): ParsedYaml {
    type StackEntry = { key: string; indent: number };
    const result = new Map<string, { raw: string; value: number }>();
    const lines = text.split(/\r?\n/);
    const stack: StackEntry[] = [];

    lines.forEach((line: string, index: number) => {
        const trimmed = line.trim();
        if (!trimmed || trimmed.startsWith("#")) {
            return;
        }
        if (/\t/.test(line)) {
            throw new Error(`Invalid YAML line ${index + 1}: tab indentation is not supported`);
        }

        const colonIndex = line.indexOf(":");
        if (colonIndex < 0) {
            throw new Error(`Invalid YAML line ${index + 1}: missing ":"`);
        }

        const indent = line.length - line.trimStart().length;
        if (indent % 2 !== 0) {
            throw new Error(`Invalid YAML line ${index + 1}: indentation must use 2-space units`);
        }
        while (stack.length > 0 && indent <= stack[stack.length - 1].indent) {
            stack.pop();
        }

        const localKey = line.slice(0, colonIndex).trim();
        const rawValue = stripInlineComment(line.slice(colonIndex + 1));
        if (!localKey) {
            throw new Error(`Invalid YAML line ${index + 1}: empty key`);
        }

        if (!rawValue) {
            stack.push({ key: localKey, indent });
            return;
        }

        const keyPath = [...stack.map((entry) => entry.key), localKey].join(".");
        if (result.has(keyPath)) {
            throw new Error(`Duplicate key "${keyPath}" at line ${index + 1}`);
        }

        const isNumberLiteral = /^-?(?:\d+\.?\d*|\.\d+)(?:[eE][+-]?\d+)?$/.test(rawValue);
        if (!isNumberLiteral) {
            throw new Error(
                `Unsupported value for "${keyPath}" at line ${index + 1}: expected numeric scalar`,
            );
        }

        const value = Number(rawValue);
        if (!Number.isFinite(value)) {
            throw new Error(`Invalid numeric value for "${keyPath}" at line ${index + 1}`);
        }

        result.set(keyPath, { raw: rawValue, value });
    });

    return result;
}

export function loadSchemaFromJson(schemaPath: string): SchemaEntry[] {
    const raw = fs.readFileSync(schemaPath, "utf8");
    const parsed = JSON.parse(raw) as unknown;
    if (!Array.isArray(parsed)) {
        throw new Error(`Invalid schema file: ${schemaPath} (expected array)`);
    }
    return parsed.map((entry, index) => {
        if (!Array.isArray(entry) || entry.length < 2 || entry.length > 3) {
            throw new Error(
                `Invalid schema entry at index ${index} in ${schemaPath}: expected tuple [pathKey, outKey, typeHint?]`,
            );
        }
        const [pathKey, outKey, typeHint] = entry;
        if (typeof pathKey !== "string" || pathKey.length === 0) {
            throw new Error(`Invalid pathKey at index ${index} in ${schemaPath}`);
        }
        if (typeof outKey !== "string" || outKey.length === 0) {
            throw new Error(`Invalid outKey at index ${index} in ${schemaPath}`);
        }
        if (
            typeHint !== undefined
            && typeHint !== "u32"
            && typeHint !== "f32"
        ) {
            throw new Error(`Invalid typeHint at index ${index} in ${schemaPath}: ${String(typeHint)}`);
        }
        return [pathKey, outKey, typeHint] as SchemaEntry;
    });
}

export function validateAgainstSchema(parsed: ParsedYaml, schema: SchemaEntry[]): void {
    const schemaKeys = new Set(schema.map(([pathKey]) => pathKey));
    const parsedKeys = new Set(parsed.keys());

    for (const key of schemaKeys) {
        if (!parsedKeys.has(key)) {
            throw new Error(`Missing key in YAML: "${key}"`);
        }
    }

    for (const key of parsedKeys) {
        if (!schemaKeys.has(key as string)) {
            throw new Error(`Unknown key in YAML: "${key}"`);
        }
    }

    // Rustのu32 型向けに整数チェック
    for (const [pathKey, _outKey, typeHint] of schema) {
        const entry = parsed.get(pathKey);
        if (entry !== undefined && typeHint === "u32" && !Number.isInteger(entry.value)) {
            throw new Error(`Key "${pathKey}" must be an integer for Rust type u32`);
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

export function rustLiteral(raw: string, typeHint: string | undefined): string {
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

export function jsLiteral(raw: string): string {
    return raw;
}
