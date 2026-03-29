import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vitest/config";

const configDir = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
    test: {
        environment: "jsdom",
        setupFiles: [path.resolve(configDir, "tests/setup.ts")],
        include: [path.resolve(configDir, "**/*.test.ts")],
        exclude: ["node_modules", "dist"],
        globals: true,
        server: {
            deps: {
                inline: [/frey_wasm/],
            },
        },
    },
    resolve: {
        alias: {
            "@wasm": path.resolve(configDir, "../generated/wasm/web"),
        },
    },
});
