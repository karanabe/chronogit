---
title: Release procedure
description: Prepare ChronoGit release archives and checksums without publishing them.
tags:
  - release
  - packaging
  - checklist
sidebar:
  order: 4
---

This procedure prepares native ChronoGit `0.1.0` archives and SHA-256 checksums. It does not authorize publication.

## Owner decisions required first

Before creating a public artifact or tag, the repository owner must:

1. select the canonical repository URL and set Cargo's `repository` field;
2. decide whether distribution uses source checkout, GitHub Releases, crates.io, or a documented combination;
3. complete both platform rows in the [manual terminal smoke test](/developer/terminal-smoke/).

Keep `publish = false` until crates.io publication is explicitly selected. Do not tag a release while any decision or required check is incomplete.

## Quality gate

From a clean checkout of the exact revision to release, using Rust 1.88.0 or newer:

```sh title="Terminal"
cargo fmt --all --check
cargo clippy --all-targets --all-features --tests --benches -- -D warnings
cargo test --all-features
cargo build --release --locked
cargo install --path . --locked
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs build
```

Repeat the dependency and source audits described in [Validation](/developer/validation/). Confirm the version in `Cargo.toml`, `Cargo.lock`, `CHANGELOG.md`, and the proposed tag agrees. Review the generated documentation and both platform smoke-test rows.

## Create a native archive

Build each archive on its target OS. Set one supported target label explicitly:

- `x86_64-unknown-linux-gnu`
- `aarch64-apple-darwin`
- `x86_64-apple-darwin`

From the clean checkout, replace the target value as needed:

```sh title="Terminal"
release_version=0.1.0
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

Artifact creation and checksum verification do not resolve the owner decisions above and do not authorize a tag, release upload, or registry publication.
