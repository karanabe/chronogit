---
title: リリース手順
description: 公開を行わず、ChronoGitのリリースarchiveとchecksumを準備します。
tags:
  - リリース
  - パッケージ
  - チェックリスト
sidebar:
  order: 4
---

この手順はChronoGit `0.1.0`のネイティブarchiveとSHA-256 checksumを準備します。公開を許可する手順ではありません。

## 最初に必要な所有者の判断

公開artifactまたはtagを作る前に、リポジトリ所有者は次を行う必要があります。

1. 正式なリポジトリURLを選び、Cargoの`repository`フィールドを設定する。
2. 配布方法をsource checkout、GitHub Releases、crates.io、または明記した組み合わせから決める。
3. [手動ターミナルスモークテスト](/ja/developer/terminal-smoke/)の両platform行を完了する。

crates.ioへの公開を明示的に選ぶまでは`publish = false`を維持します。判断や必須checkが未完了ならtagを作らないでください。

## 品質ゲート

リリースするrevisionそのもののクリーンなチェックアウトで、Rust 1.88.0以降を使って実行します。

```sh title="ターミナル"
cargo fmt --all --check
cargo clippy --all-targets --all-features --tests --benches -- -D warnings
cargo test --all-features
cargo build --release --locked
cargo install --path . --locked
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs build
```

[検証](/ja/developer/validation/)に記録された依存関係・ソース監査も再実行します。`Cargo.toml`、`Cargo.lock`、`CHANGELOG.md`、予定tagのversionが一致することを確認します。生成ドキュメントと両platformのsmoke test行もレビューします。

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

artifact作成とchecksum検証だけでは、先の所有者判断は解決されず、tag、release upload、registry公開も許可されません。
