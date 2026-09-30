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

この手順はworkspace内の`chronogit`と`vim-navigation`を対象にします。
依存crateを、それを必要とするアプリケーションより先に公開します。

## リリースの前提条件

crateの公開、公開artifactの作成、release tagの作成を行う前に、maintainerは次を完了する必要があります。

1. 公開する各crateのcrates.io公開権限を用意する。
2. [手動ターミナルスモークテスト](/ja/developer/terminal-smoke/)の両platform行を完了する。
3. 変更した各packageに未使用のversionを選び、manifest、lockfile、release note、予定tagを整合させる。crates.ioの既存versionは再公開できない。

`vim-navigation`を変更した場合はそのversionを上げ、ChronoGitの`version`指定も
利用APIを含む最初のrelease以降に更新します。workspace開発用の相対`path`は残します。
Cargoは公開manifestから`path`を除き、crates.ioから指定versionを解決します。
この更新がないと、ローカルではビルドできても公開済みの依存crateに必要なAPIがない
問題を見落とします。

`Cargo.toml`は公開先をcrates.ioに限定しています。前提条件または必須checkが未完了なら、公開もtag作成も行わないでください。

## local workspaceゲート

review対象revisionそのもののcleanなcheckoutで、Rust 1.88.0以降を使って実行します。

```sh title="ターミナル"
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

このgateはlocal path統合とsource install経路を検証します。未公開dependencyを
crates.ioから解決できることの証拠ではありません。

## 順序付きregistryゲート

新しい`vim-navigation`を公開する場合は、まず依存packageを検証します。

```sh title="ターミナル"
cargo package -p vim-navigation --locked
cargo publish -p vim-navigation --dry-run --locked
```

必要な依存versionを公開し、crates.ioから取得できることを確認した後で、ChronoGitを
検証します。

```sh title="ターミナル"
cargo package -p chronogit --locked
cargo publish -p chronogit --dry-run --locked
```

必要な依存crateをregistryから取得できなければChronoGitのpackage検証は失敗します。
workspaceのtest成功だけでは、このregistry gateを満たしません。公開済みの依存crateを
変更せずにChronoGitだけを公開する場合は、`vim-navigation`を再公開せずChronoGitの
検証を実行します。CIもworkspace検証とregistry package検証を別の必須jobにしています。

正確なdiffとpackage内容を確認し、資格情報、個人path、内部専用note、無関係なartifactが含まれていないことを確認します。生成ドキュメント、dependency warning、両platformのsmoke test行もレビューします。許容された保守warningを脆弱性として扱わない一方、記録を残し、その依存経路を削除または更新できるか確認します。

## 各crateの内容を確認して公開する

公開前にregistryへ送る正確な内容を確認します。

```sh title="ターミナル"
cargo package -p vim-navigation --list
cargo package -p chronogit --list
```

`vim-navigation`の一覧には再利用library source、互換性/oracle test、README、互換性
契約、2つのlicense file、Cargo metadataだけを含めます。ChronoGitの一覧にはRust
application・test source、sample keymap・LSP profile、README、changelog、2つの
license file、Cargoが生成するmanifest・lock・VCS metadataだけを含めます。
documentation site、repository workflow、agent integration file、contributor専用
documentを含めてはいけません。

リリース対象そのもののrevisionで、すべての前提条件と品質ゲートが成功した後、権限を持つmaintainerが公開します。

```sh title="ターミナル"
cargo publish -p vim-navigation --locked
# vim-navigationがcrates.ioから解決可能になった後、ChronoGit gateを再実行する。
cargo publish -p chronogit --locked
```

最初のcommandは依存crateの新しいreleaseがある場合だけ実行します。2つ目のcommandは、
ChronoGit自身の未使用versionとpackage検証の成功が必要です。

公開したcrate versionは取り消せません。registry account、crate名、version、package内容、dry-runの出力を確認してから、このコマンドを実行してください。

## ネイティブarchiveを作る

各archiveは対象OSでbuildします。対応するtarget labelを明示します。

- `x86_64-unknown-linux-gnu`
- `aarch64-apple-darwin`
- `x86_64-apple-darwin`

クリーンなチェックアウトで、必要に応じてtargetを置き換えて実行します。

```sh title="ターミナル"
release_version=0.8.0
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
