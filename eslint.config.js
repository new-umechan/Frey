import importPlugin from "eslint-plugin-import";
import globals from "globals";

export default [
    {
        files: ["web/src/**/*.js"],
        languageOptions: {
            ecmaVersion: "latest",
            sourceType: "module",
            globals: globals.browser,
        },
        plugins: {
            import: importPlugin,
        },
        rules: {
            "no-unused-vars": [
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
        files: ["web/src/app/**/*.js", "web/src/gfx/**/*.js", "web/src/ui/**/*.js"],
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
