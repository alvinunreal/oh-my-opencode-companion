# Companion development

Standalone Rust desktop companion for oh-my-opencode-slim. Read `codemap.md`
before changing native behavior. The crate, Cargo.lock, `src/`, and embedded
`animations/` live at the repository root. `VIDEOS/` contains source media only.

- Keep Rust code formatted with `cargo fmt`; run `cargo fmt --check` and
  `cargo test --locked`. Linux system dependencies are listed in `README.md`.
- Release helpers use Node.js 22 built-ins only. Run
  `node --test scripts/*.test.mjs`; tests use injected API requests, not GitHub.
- Keep the executable/archive prefix `oh-my-opencode-slim-companion` and tags
  `companion-vX.Y.Z`. Never move published tags or overwrite release assets.
- Native release automation belongs here. Plugin manifests, download checksums,
  lifecycle integration, and configuration persistence belong in
  `alvinunreal/oh-my-opencode-slim`; do not recreate that tooling here.
- Preserve shared-state field names, environment variables, paths, and locking
  semantics unless explicitly coordinating a protocol change with the plugin.
- Update README/codemap when build, release, or protocol behavior changes.
