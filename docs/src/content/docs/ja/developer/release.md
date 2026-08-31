---
title: リリース手順
description: crates.io packageを検証・公開し、native archiveとchecksumを準備します。
tags:
  - リリース
  - パッケージ
  - チェックリスト
sidebar:
  order: 4
---

この手順はChronoGit `0.1.0`のcrates.io packageを検証し、SHA-256 checksum付きのnative archiveを準備します。registryへの実際の公開は、maintainerが明示的に行う操作です。

## リリースの前提条件

crateの公開、公開artifactの作成、release tagの作成を行う前に、maintainerは次を完了する必要があります。

1. crates.ioの`chronogit` crateに対する公開権限を用意する。
2. [手動ターミナルスモークテスト](/ja/developer/terminal-smoke/)の両platform行を完了する。
3. `Cargo.toml`、`Cargo.lock`、`CHANGELOG.md`、予定tagのversionが一致することを確認する。

`Cargo.toml`は公開先をcrates.ioに限定しています。前提条件または必須checkが未完了なら、公開もtag作成も行わないでください。

## 品質ゲート

リリースするrevisionそのもののクリーンなチェックアウトで、Rust 1.88.0以降を使って実行します。

```sh title="ターミナル"
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

[検証](/ja/developer/validation/)に記録された依存関係・ソース監査も再実行します。生成ドキュメント、packageに含まれるファイル一覧、両platformのsmoke test行もレビューします。

## crateの内容を確認して公開する

公開前にregistryへ送る正確な内容を確認します。

```sh title="ターミナル"
cargo package --list
```

一覧にはRustのapplication・test source、README、changelog、2つのlicense file、Cargoが生成するmanifest・lock・VCS metadataだけが含まれている必要があります。documentation site、repository workflow、agent integration file、contributor専用documentを含めてはいけません。

リリース対象そのもののrevisionで、すべての前提条件と品質ゲートが成功した後、権限を持つmaintainerが公開します。

```sh title="ターミナル"
cargo publish --locked
```

公開したcrate versionは取り消せません。registry account、crate名、version、package内容、dry-runの出力を確認してから、このコマンドを実行してください。

## ネイティブarchiveを作る

各archiveは対象OSでbuildします。対応するtarget labelを明示します。

- `x86_64-unknown-linux-gnu`
- `aarch64-apple-darwin`
- `x86_64-apple-darwin`

クリーンなチェックアウトで、必要に応じてtargetを置き換えて実行します。

```sh title="ターミナル"
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

archiveには実行ファイル、README、changelog、Apache-2.0とMITのlicense fileだけを含めます。

## checksumを作成・検証する

Linuxでは次を実行します。

```sh title="ターミナル"
sha256sum "${release_name}.tar.gz" > "${release_name}.tar.gz.sha256"
sha256sum -c "${release_name}.tar.gz.sha256"
```

macOSでは次を実行します。

```sh title="ターミナル"
shasum -a 256 "${release_name}.tar.gz" > "${release_name}.tar.gz.sha256"
shasum -a 256 -c "${release_name}.tar.gz.sha256"
```

公開前に内容を確認します。

```sh title="ターミナル"
tar -tzf "${release_name}.tar.gz"
```

`release_stage`が`mktemp -d`から返された正確なディレクトリであることを確認してから、staging directoryを削除します。

```sh title="ターミナル"
test -n "${release_stage}" && test "${release_stage}" != / && rm -rf -- "${release_stage}"
```

artifact作成とchecksum検証だけではcrateは公開されず、tagやrelease uploadも許可されません。registry公開は、上記の`cargo publish --locked`を明示的に実行した場合にだけ行われます。
