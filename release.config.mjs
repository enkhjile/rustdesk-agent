export default {
    branches: ["main", { name: "alpha", prerelease: true }],
    tagFormat: "v${version}",
    plugins: [
        "@semantic-release/commit-analyzer",
        "@semantic-release/release-notes-generator",
        ["@semantic-release/github", {
            successComment: false,
            failComment: false,
            releasedLabels: false,
        }],
    ],
};
