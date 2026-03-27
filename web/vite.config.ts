import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";

const configDir = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
    root: configDir,
    server: {
        fs: {
            allow: [path.resolve(configDir, "../..")],
        },
    },
    build: {
        outDir: path.join(configDir, "../dist"),
        emptyOutDir: true,
    },
});
