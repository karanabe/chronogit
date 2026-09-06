---
title: Release procedure
description: Validate and publish the crates.io package, then prepare native archives and checksums.
tags:
  - release
  - packaging
  - checklist
sidebar:
  order: 4
---

This procedure covers the `chronogit` and `vim-navigation` workspace packages.
Publish dependencies before the applications that require them.

## Release prerequisites

Before publishing a crate, creating a public artifact, or tagging the release, the maintainer must:

1. have publish access to each crate being released on crates.io;
2. complete both platform rows in the [manual terminal smoke test](/developer/terminal-smoke/);
3. choose an unused version for each changed package and update its manifest, lockfile, release notes, and proposed tag consistently. An existing crates.io version cannot be republished.

When changing `vim-navigation`, bump its version and update ChronoGit's
`version` requirement to the first release containing the APIs it uses. Keep
the relative `path` for workspace development: Cargo removes it from the
published manifest and resolves the version from crates.io. A local build can
otherwise hide calls to APIs missing from the published dependency.

`Cargo.toml` restricts publication to crates.io. Do not publish or tag a release while any prerequisite or required check is incomplete.

## Local workspace gate

From a clean checkout of the exact revision under review, using Rust 1.88.0 or newer:

```sh title="Terminal"
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features --tests --benches -- -D warnings
cargo test --workspace --all-features
cargo build --workspace --release --locked
cargo install --path . --locked
cargo audit
cargo tree --duplicates
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs build
```

This gate validates the local path integration and source-install route. It
does not prove that an unpublished dependency can be resolved from crates.io.

## Ordered registry gate

Validate the dependency package when preparing a new `vim-navigation` release:

```sh title="Terminal"
cargo package -p vim-navigation --locked
cargo publish -p vim-navigation --dry-run --locked
```

After the required dependency version is published and resolves from crates.io,
validate ChronoGit:

```sh title="Terminal"
cargo package -p chronogit --locked
cargo publish -p chronogit --dry-run --locked
```

ChronoGit packaging fails if the registry cannot supply its dependency; passing
workspace tests does not satisfy this registry gate. For a ChronoGit-only
release using an unchanged, already-published dependency, skip republishing
`vim-navigation` and run the ChronoGit checks directly. CI keeps workspace
validation and registry packaging as separate required jobs.

Review the exact diff and packaged files for credentials, private paths, internal-only notes, and unrelated artifacts. Also review the generated documentation, dependency warnings, and both platform smoke-test rows. Do not treat an allowed maintenance warning as a vulnerability, but record it and confirm whether its dependency path can be removed or upgraded.

## Inspect and publish each crate

Inspect the exact registry payload before publishing:

```sh title="Terminal"
cargo package -p vim-navigation --list
cargo package -p chronogit --list
```

The `vim-navigation` list must contain only its reusable library source,
compatibility/oracle tests, README, compatibility contract, both license files,
and Cargo metadata.
The ChronoGit list must contain the Rust application and test sources, sample
keymap and LSP profile, README, changelog, both license files, and
Cargo-generated manifest, lock, and VCS metadata only. It must not contain the
documentation site, repository workflows, agent integration files, or
contributor-only documents.

After every prerequisite and quality gate passes on the exact revision to release, an authorized maintainer publishes it:

```sh title="Terminal"
cargo publish -p vim-navigation --locked
# Wait for vim-navigation to resolve from crates.io, then re-run ChronoGit gates.
cargo publish -p chronogit --locked
```

Run the first command only for a new dependency release. The second command
requires ChronoGit's own unused version and passing package checks.

Publishing a crate version cannot be undone. Run this command only after checking the registry account, crate name, version, package contents, and dry-run output.

## Create a native archive

Build each archive on its target OS. Set one supported target label explicitly:

- `x86_64-unknown-linux-gnu`
- `aarch64-apple-darwin`
- `x86_64-apple-darwin`

From the clean checkout, replace the target value as needed:

```sh title="Terminal"
release_version=0.5.0
release_target=x86_64-unknown-linux-gnu
release_name="chronogit-${release_version}-${release_target}"
release_stage=$(mktemp -d)

cargo build --release --locked
mkdir -p "${release_stage}/${release_name}"
cp target/release/chronogit "${release_stage}/${release_name}/chronogit"
cp README.md CHANGELOG.md LICENSE-APACHE LICENSE-MIT \
  "${release_stage}/${release_name}/"
tar -C "${release_stage}" -czf "${release_name}.tar.gz" "${release_name}"
```

The archive contains only the executable, README, changelog, and the Apache-2.0 and MIT license files.

## Create and verify the checksum

On Linux:

```sh title="Terminal"
sha256sum "${release_name}.tar.gz" > "${release_name}.tar.gz.sha256"
sha256sum -c "${release_name}.tar.gz.sha256"
```

On macOS:

```sh title="Terminal"
shasum -a 256 "${release_name}.tar.gz" > "${release_name}.tar.gz.sha256"
shasum -a 256 -c "${release_name}.tar.gz.sha256"
```

Inspect the contents before publication:

```sh title="Terminal"
tar -tzf "${release_name}.tar.gz"
```

Remove the staging directory only after confirming `release_stage` is the exact directory returned by `mktemp -d`:

```sh title="Terminal"
test -n "${release_stage}" && test "${release_stage}" != / && rm -rf -- "${release_stage}"
```

Artifact creation and checksum verification do not publish the crate or authorize a tag or release upload. Registry publication occurs only through the explicit `cargo publish --locked` step above.
