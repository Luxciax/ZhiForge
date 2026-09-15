# Versioning and Release Policy

SelectionTranslator uses [Semantic Versioning](https://semver.org/) starting with the first public stable release, `1.0.0`.

## Version format

Stable releases use:

`MAJOR.MINOR.PATCH`

Git tags use:

`vMAJOR.MINOR.PATCH`

Examples: `v1.0.0`, `v1.1.0`, `v1.1.1`, `v2.0.0`.

## When to increment

- **MAJOR**: incompatible changes to persisted configuration, public behavior, extension contracts, automation interfaces, or other compatibility guarantees.
- **MINOR**: backward-compatible features, new providers/adapters/actions, substantial UI capabilities, or optional new configuration fields.
- **PATCH**: backward-compatible bug fixes, compatibility fixes, small UX improvements, dependency/security fixes, and packaging corrections.

## Pre-releases

Use standard SemVer suffixes:

- `1.1.0-alpha.1`
- `1.1.0-beta.1`
- `1.1.0-rc.1`

Pre-release tags use the same `v` prefix, for example `v1.1.0-rc.1`.

## Version synchronization

Before every release, these values must match exactly:

- `package.json`
- `src-tauri/Cargo.toml`
- `src-tauri/Cargo.lock` (`selection-translator` package entry)
- `src-tauri/tauri.conf.json`

## Release artifacts

A stable Windows release should include:

- `SelectionTranslator_X.Y.Z_x64-setup.exe`
- `SelectionTranslator_X.Y.Z_x64_en-US.msi`
- `SelectionTranslator_X.Y.Z_x64_Portable.exe`
- SHA-256 checksums
- Release notes / changelog summary

## Release gate

Before publishing a stable release:

1. `pnpm check`
2. `pnpm build`
3. `cargo test --manifest-path src-tauri/Cargo.toml`
4. `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings`
5. `pnpm tauri build`
6. Verify installer / MSI / portable artifact names and hashes.
7. Commit the release state.
8. Create annotated or GitHub release tag `vX.Y.Z` from that commit.
9. Upload the artifacts to the GitHub Release.

## Public history

Versions before `1.0.0` were internal development snapshots. The public stable version history begins at `v1.0.0`.
