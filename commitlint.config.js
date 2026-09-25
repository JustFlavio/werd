/** Conventional Commits, see CONTRIBUTING.md. */
export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    "scope-enum": [
      2,
      "always",
      ["core", "cli", "desktop", "ui", "runtimes", "services", "proxy", "i18n", "ci", "docs", "deps", "release"],
    ],
    "body-max-line-length": [0],
  },
};
