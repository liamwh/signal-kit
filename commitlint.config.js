module.exports = {
	extends: ["@commitlint/config-conventional"],
	rules: {
		"type-enum": [
			2,
			"always",
			[
				"build",
				"chore",
				"ci",
				"docs",
				"feat",
				"fix",
				"perf",
				"refactor",
				"revert",
				"style",
				"test",
			],
		],
		"subject-case": [2, "never", ["upper-case", "pascal-case"]],
		"subject-empty": [2, "never"],
		"type-empty": [2, "never"],
		"type-case": [2, "always", "lower-case"],
		"header-max-length": [0, "always", Infinity], // Disable the header length check
	},
};
