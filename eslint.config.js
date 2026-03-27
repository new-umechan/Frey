import typescriptEslint from "@typescript-eslint/eslint-plugin";
import typescriptParser from "@typescript-eslint/parser";
import importPlugin from "eslint-plugin-import";
import globals from "globals";

export default [
    {
        files: ["web/src/**/*.ts"],
        languageOptions: {
            parser: typescriptParser,
            ecmaVersion: "latest",
            sourceType: "module",
            globals: globals.browser,
        },
        plugins: {
            "@typescript-eslint": typescriptEslint,
            import: importPlugin,
        },
        rules: {
            "no-unused-vars": "off",
            "@typescript-eslint/no-unused-vars": [
                "error",
                {
                    argsIgnorePattern: "^_",
                    varsIgnorePattern: "^_",
                    caughtErrorsIgnorePattern: "^_",
                },
            ],
        },
    },
    {
        files: ["web/src/app/**/*.ts", "web/src/gfx/**/*.ts", "web/src/ui/**/*.ts"],
        plugins: {
            import: importPlugin,
        },
        rules: {
            "import/no-unused-modules": [
                "warn",
                {
                    unusedExports: true,
                    missingExports: false,
                },
            ],
        },
    },
];
