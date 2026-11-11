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
            assets: [
                { path: "dist/rustdesk-agent-windows-x64-unsigned.zip", label: "Windows x64 (unsigned)" },
                { path: "dist/SHA256SUMS", label: "SHA-256 checksum" },
            ],
        }],
    ],
};
