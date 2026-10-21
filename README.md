# oh-my-opencode-companion

Native Rust desktop companion for
[oh-my-opencode-slim](https://github.com/alvinunreal/oh-my-opencode-slim).
Animated floating windows show active agents, waiting-input attention, and a
project/preset menu. This repository preserves the history extracted from the
plugin's former `companion/` subtree; the Cargo crate now lives at the root.

The executable remains **`oh-my-opencode-slim-companion`**. Releases are public
at [alvinunreal/oh-my-opencode-companion/releases](https://github.com/alvinunreal/oh-my-opencode-companion/releases).
Normal users should let the plugin install and launch the companion.

## Build and test

Install stable Rust and a native desktop toolchain. On Debian/Ubuntu:

```sh
sudo apt-get install -y build-essential pkg-config libgtk-3-dev \
  libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev
cargo build --release --locked
cargo fmt --check
cargo test --locked
```

macOS requires the Xcode command-line tools; Windows requires the MSVC toolchain
and Windows SDK. The output is `target/release/oh-my-opencode-slim-companion`
(`.exe` on Windows). Animations are embedded at compile time; see
[`VIDEOS/README.md`](VIDEOS/README.md) to regenerate sprite sheets.

Release helper tests require Node.js 22, with no npm install or dependencies:

```sh
node --test scripts/*.test.mjs
```

## Native releases

`companion-ci.yml` runs root-crate formatting/Linux tests, Windows tests, and
native release helper tests. `companion-release.yml` is both reusable and
manually dispatchable. Its default targets publish all five supported archives:

| Platform | Rust target | Archive extension |
| --- | --- | --- |
| macOS arm64 | `aarch64-apple-darwin` | `.tar.gz` |
| macOS x64 | `x86_64-apple-darwin` | `.tar.gz` |
| Linux x64 | `x86_64-unknown-linux-gnu` | `.tar.gz` |
| Linux arm64 | `aarch64-unknown-linux-gnu` | `.tar.gz` |
| Windows x64 | `x86_64-pc-windows-msvc` | `.zip` |

Tags remain `companion-vX.Y.Z`. Archive names remain
`oh-my-opencode-slim-companion-vX.Y.Z-<rust-target>.<extension>` and contain only
the executable. A partial manual target selection produces workflow artifacts
only, never a public release. For example, maintainers can dispatch:

```sh
gh workflow run companion-release.yml --repo alvinunreal/oh-my-opencode-companion \
  -f version=0.1.10 \
  -f targets=macos-arm64,macos-x64,linux-x64,linux-arm64,windows-x64
```

The workflow stamps **both root Cargo files** before building, then tags a
release-only commit with the identical stamp. Main does not need a version-bump
commit or a plugin manifest PR. A published tag always records the native
source and crate version used to build its binaries.

`companion-auto-release.yml` runs on relevant pushes to `master`, manually,
and every six hours. Its caller requests automatic planning without choosing a
version or holding an outer lock. The reusable release workflow first acquires
the same publication lock used by manual releases, then resolves the **current
default-branch commit**, fetches tags, and checks release state. A queued run
cannot plan against another release's unfinished uploads or build its stale
triggering revision. Every build and the publication job use the exact resolved
source SHA and version from preparation. Explicit-version manual releases use
their triggering source revision instead.

Under that lock, planning selects the highest stable companion version and
compares `src/`, `assets/`, `animations/`, `build.rs`, and both
Cargo files with that tag. Only the crate's own version is excluded from the
comparison; dependency changes are significant. Documentation, source MP4s,
and automation-only changes do not require a new native binary.

An unchanged source tree and complete stable release (all five nonempty,
uploaded archives) skips all builds and publication. Source drift, a missing
release, or an incomplete latest release allocates the **next patch version**.
Tags/assets are never
rewritten or repaired in place. Fetch, authorization, transport, rate-limit,
and malformed API errors fail closed instead of allocating a release. Only
HTTP 404 is treated as missing after retries. Automation requires an existing
stable companion tag; initial history/tags and mirrored releases are seeded by
the extraction maintainer before enabling it.

Useful read-only checks (after fetching tags):

```sh
node scripts/verify-companion-source.mjs --tag=companion-v0.1.9
GH_TOKEN="$(gh auth token)" GITHUB_REPOSITORY=alvinunreal/oh-my-opencode-companion \
  node scripts/check-companion-release.mjs --tag=companion-v0.1.9
```

The source checker exits 0 for a match, 2 for native drift, and 1 for errors.
`prepare-companion-release.mjs --version=X.Y.Z` writes only crate versions;
it does not generate manifests or checksums.

## Plugin integration and shared-state protocol

The plugin owns downloads, archive checksums, launch/stop lifecycle, session
publication, and user configuration. Its scheduled/manual release sync consumes
this repository's public releases independently. This repository never edits
plugin manifests or opens plugin synchronization PRs. Native semver and the
plugin's npm version are independent.

There is no network service. Native and TypeScript coordinate through
`$XDG_DATA_HOME/opencode/storage/oh-my-opencode-slim/companion-state.json`,
falling back to `~/.local/share` when XDG_DATA_HOME is unset/empty, on all
platforms. The existing path and protocol are unchanged by extraction.

- The launcher sets `OH_MY_OPENCODE_SLIM_COMPANION_SESSION_ID` to its owning
  session. A singleton coordinates processes; displayed projects are chosen
  from published live activity, not solely that owner ID.
- `src/state.rs` defines the wire structs. Top-level state contains `version`,
  `sessions`, `known_projects`, `config`, `window_positions`, and
  `preset_requests`. Session data includes cwd, agents/model details, status,
  monotonic `attention_seq`, PID, configuration, and preset acknowledgements.
- Native writes geometry and typed preset requests while preserving shared
  state under the sibling `companion-state.json.lock` directory lock. Respect
  that lock and existing atomic-write/stale-owner recovery rules in any writer;
  never overwrite another process's fields from an unlocked stale snapshot.
- Preset requests carry `request_id`, `session_id`, optional `target_cwd`,
  `scope`, `preset`, and `inherit`. TypeScript validates/applies configuration
  and publishes acknowledgement/result fields; native does not edit JSONC.
- Animation keys, JSON names (including `gifPack`/`loopStyle`), environment
  variables, and storage paths are integration contracts, not renamed with the
  repository. Coordinate any future protocol changes with the plugin.

`OH_MY_OPENCODE_SLIM_COMPANION_DEBUG=1` enables additional native logging.
See [`codemap.md`](codemap.md) for native architecture and regression coverage.

## License

MIT; see [LICENSE](LICENSE).
