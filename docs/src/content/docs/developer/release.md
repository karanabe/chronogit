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

This procedure validates the ChronoGit `0.2.0` crates.io package and prepares native archives with SHA-256 checksums. The actual registry publish remains an explicit maintainer action.

## Release prerequisites

Before publishing a crate, creating a public artifact, or tagging the release, the maintainer must:

1. have publish access to the `chronogit` crate on crates.io;
2. complete both platform rows in the [manual terminal smoke test](/developer/terminal-smoke/);
3. confirm the version in `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, and the proposed tag agrees.

`Cargo.toml` restricts publication to crates.io. Do not publish or tag a release while any prerequisite or required check is incomplete.

## Quality gate

From a clean checkout of the exact revision to release, using Rust 1.88.0 or newer:

```sh title="Terminal"
cargo fmt --all --check
cargo clippy --all-targets --all-features --tests --benches -- -D warnings
cargo test --all-features
cargo build --release --locked
cargo install --path . --locked
cargo package --locked
cargo publish --dry-run --locked
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs build
```

Repeat the dependency and source audits described in [Validation](/developer/validation/). Review the generated documentation, packaged file list, and both platform smoke-test rows.

## Inspect and publish the crate

Inspect the exact registry payload before publishing:

```sh title="Terminal"
cargo package --list
```

The list must contain the Rust application and test sources, sample keymap, README, changelog, both license files, and Cargo-generated manifest, lock, and VCS metadata only. It must not contain the documentation site, repository workflows, agent integration files, or contributor-only documents.

After every prerequisite and quality gate passes on the exact revision to release, an authorized maintainer publishes it:

```sh title="Terminal"
cargo publish --locked
```

Publishing a crate version cannot be undone. Run this command only after checking the registry account, crate name, version, package contents, and dry-run output.

## Create a native archive

Build each archive on its target OS. Set one supported target label explicitly:

- `x86_64-unknown-linux-gnu`
- `aarch64-apple-darwin`
- `x86_64-apple-darwin`

From the clean checkout, replace the target value as needed:

```sh title="Terminal"
release_version=0.2.0
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
