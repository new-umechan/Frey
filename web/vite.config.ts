import path from "node:path";
import { fileURLToPath } from "node:url";
import { defineConfig } from "vite";
import viteCompression from "vite-plugin-compression";

const configDir = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
    root: configDir,
    plugins: [
        viteCompression({
            algorithm: "brotliCompress",
            ext: ".br",
            threshold: 1024,
        }),
        viteCompression({
            algorithm: "gzip",
            ext: ".gz",
            threshold: 1024,
        }),
    ],
    server: {
        fs: {
            allow: [path.resolve(configDir, "../..")],
        },
        proxy: {
            "/api": {
                target:
                    process.env.FREY_PRECOMPUTE_PROXY_TARGET ??
                    "http://127.0.0.1:8787",
                changeOrigin: true,
            },
        },
    },
    build: {
        outDir: path.join(configDir, "../dist"),
        emptyOutDir: true,
    },
});
