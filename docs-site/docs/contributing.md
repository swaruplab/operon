# Contributing

We welcome contributions from the bioinformatics community.

## Branch model

- **`cross-platform`** — active development and the default branch. PRs
  go here.
- Release tags (`v0.7.x`) are cut directly from `cross-platform` and
  trigger the multi-OS Release workflow.

There is no `main`/`dev` split — the project moved to a single
default branch model.

## Setting up the dev environment

```bash
# Fork on GitHub, then:
git clone https://github.com/<your-username>/operon.git
cd operon
git remote add upstream https://github.com/swaruplab/operon.git

# Install dependencies (Rust + Node + per-OS native libs)
# See: install/build-from-source.md

npm install
npm run tauri dev    # opens the app with HMR
```

See [Build from source](install/build-from-source.md) for the full
per-OS prerequisites.

## Making a change

```bash
# Branch off cross-platform
git checkout -b feature/my-change cross-platform

# Edit. Frontend hot-reloads; Rust changes trigger a backend rebuild.

# Format Rust before committing — CI enforces this
(cd src-tauri && cargo fmt)

# Make sure it builds clean
(cd src-tauri && cargo check)
npm run build

# Commit & push
git push -u origin feature/my-change

# Open a PR targeting cross-platform
gh pr create --base cross-platform
```

## Style

| Language | Tooling | Notes |
|---|---|---|
| Rust | `cargo fmt` + `cargo clippy` | Standard rustfmt defaults |
| TypeScript / React | Vite + tsc | No prescribed prettier config; match existing style |
| Markdown (this site) | None — write for clarity | Material's `pymdownx.tabbed`, `admonition`, and `attr_list` are available |

## Commit messages

We use short, prefix-style messages:

| Prefix | When |
|---|---|
| `v0.7.X:` | Release commits — version bump, CHANGELOG entry |
| `feat:` | New user-visible feature |
| `fix:` | Bug fix |
| `docs:` | Docs site / README only |
| `style:` | Formatting only (cargo fmt etc.) |
| `refactor:` | Internal refactor with no behaviour change |
| `chore:` | Build, deps, CI |

Examples:

```
feat: add Report mode with tool-restricted writeup
fix: clear stale provider env vars on Claude spawn
docs: rewrite README for cross-platform + v0.7.x features
```

## What we welcome

- **Bug fixes** — these are the easiest to land
- **New protocols** — add them under `protocols/<category>/<name>/SKILL.md`
- **Per-OS install/build improvements** — Linux distros, Windows installer tweaks
- **Documentation** — examples, recipes, troubleshooting entries
- **Performance fixes** — file explorer on large mounts, terminal lag, etc.
- **MCP integrations** — biology-aware MCPs are a sweet spot

## What needs discussion first

- **Breaking IPC changes** — open an issue
- **New top-level features** that touch the Tauri builder or state managers
- **Bumps to Tauri major version** — these touch a lot
- **Anything that changes the default privacy posture** (telemetry, etc.)

## Releasing (maintainers)

1. Bump version in `package.json`, `src-tauri/tauri.conf.json`,
   `src-tauri/Cargo.toml`.
2. Add a CHANGELOG entry.
3. Run `cargo fmt` so CI doesn't fail on whitespace.
4. Commit as `vX.Y.Z: <one-line summary>`.
5. Tag: `git tag vX.Y.Z`.
6. Push branch and tag: `git push origin cross-platform && git push origin vX.Y.Z`.
7. The Release workflow builds for macOS arm64 + x64, Windows x64, and
   Linux x64. ~15 minutes.
8. Once green, `gh release edit vX.Y.Z --draft=false --latest` to publish.

Per-release approval is required — don't push tags speculatively.

## Code of conduct

Be kind. Assume good faith. We're a small open-source project, mostly
maintained part-time by working researchers — patience and constructive
feedback go a long way.

## License

MIT. See [LICENSE](https://github.com/swaruplab/operon/blob/cross-platform/LICENSE).
