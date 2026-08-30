---
title: 検証方針と0.1.0の記録
description: 自動品質ゲートとリリース候補の検証記録を説明します。
tags:
  - テスト
  - 検証
  - リリース
sidebar:
  order: 2
---

開発中は対象を確認できる最小のテストから始め、Rust変更では最後に完全なゲートを実行します。描画、キー、ターミナルcleanup、プラットフォーム対応の主張には、[実ターミナルのスモークテスト](/ja/developer/terminal-smoke/)も必要です。

## 自動品質ゲート

リポジトリルートで実行します。

```sh title="ターミナル"
cargo fmt --all --check
cargo clippy --all-targets --all-features --tests --benches -- -D warnings
cargo test --all-features
cargo build --release --locked
```

ドキュメント変更では、サイトを別途ビルドします。

```sh title="ターミナル"
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs build
```

対象を絞ったRustコマンドには`cargo test --lib`、`cargo test --test git_service`、`cargo test --test cli`があります。絞ったテストの成功を、リリース全体の証拠として扱わないでください。

## カバレッジの構成

- domainとparserのunit testが、値の不変条件、バイト解析、行番号、不正出力を扱います。
- reducerとeffectのtestが、Historyの全panel、commitから変更ファイルへの確定操作、全文message overlay、本文レイアウトの遷移/選択/scroll、pagination、tree展開、差分フロートのopen/scroll/検索/close遷移、差分読み込み中の移動入力、message読み込み、古い応答の拒否、差分要求の集約を扱います。
- renderとkeyのtestが、loading、empty、failure、truncation、最小幅のHistory 3段レイアウト、message overlay、差分の現在行強調、差分全体のscroll、比較ラベル、`Ctrl-j`/`Ctrl-k`、`m`/`b`、検索/helpキー、意味色、キーシーケンスを扱います。
- 一時リポジトリのintegration testが、root/merge commit、detached/unborn repository、linked worktree、staged-only/mixed change、conflict、rename、delete、type change、symlink、submodule、binary/oversized diff、非UTF-8/先頭dash pathを扱います。
- CLI testが、help/version、非repository診断、Git未検出、権限エラー、非TTY拒否を扱います。
- 読み取り専用integration checkが、Git service操作の前後で`HEAD`、porcelain status、worktree bytesを比較します。
- ターミナルライフサイクルtestが、復元シーケンスと意図的にpanicするchild processを検査します。

## 現在の変更の検証

現在のrevisionでは、上記のRust完全ゲートがunit、CLI、Git serviceの計77 testとlock済みrelease buildを含めて成功しました。ドキュメントbuildも29ページを生成して成功しました。

release binaryを別のcoreutilsリポジトリに対する実80×24 PTYでも操作しました。通常のHistoryでcommitペインの`Enter`により変更ファイルへフォーカスが移り、2回目の`Enter`で選択diffが開きました。`j`と`k`で強調中の現在行が目に見えて移動しました。別の未cacheファイルでは`Loading diff…`の表示中に`Ctrl-d`を入力し、完了直後に10行先が強調され、`Ctrl-u`で先頭行へ戻りました。`q`はターミナル復元シーケンスを出して正常終了しました。直前revisionで実施したmessage全文とbodyレイアウトの確認は、下の履歴記録に残しています。

## `0.1.0`リリース候補の履歴記録

:::note[記録済みの証拠]
この節以降は`0.1.0`リリース候補で実施したチェックを保存したものです。後のrevisionでゲートを再実行する代わりにはなりません。
:::

宣言MSRVである`rustc 1.88.0`を使ったLinux環境で、lock済み依存関係によるformat、Clippy、全feature test、release buildのゲートが成功しました。

license file追加後の`cargo package --allow-dirty --locked`は49ファイル（284.2 KiB、圧縮後72.2 KiB）をpackageし、Rust 1.97.1で生成packageを再buildしました。Cargoのrepository未設定warningは意図的なrelease blockerとして残しました。Linux archive手順を一時staging directoryで実行し、実行ファイル、README、changelog、license fileについて`sha256sum -c`が成功しました。

`cargo install --path . --locked`は成功しました。別の大規模リポジトリのサブディレクトリから、インストール済みバイナリを実PTYで操作しました。Changes、History、変更ファイル、コミットメッセージ、コミットツリー、ヘルプ、1ペインレイアウト、`q`、Ctrl-Cを確認しました。両方の終了経路が成功を返し、cursor、mouse mode、raw mode、alternate screenの復元シーケンスを出力しました。

## 記録した性能と上限

別のcoreutilsリポジトリでは、最初の200履歴項目が1秒以内の最初の観測で表示されました。操作と終了を含むPTY実行は合計3.40秒、最大常駐メモリは約6 MiBでした。

維持した実装上限は、差分debounce 75 ms、Git同時読み取り2、1ページ200コミット、差分cache 16項目/16 MiB、標準出力8 MiB、標準エラー64 KiB、Gitコマンド30秒です。

## 記録した依存関係とソース監査

`cargo-audit 0.22.2`はlock済み118 packageを1,226件のRustSec advisoryに対して検査し、脆弱性なしを報告しました。`cargo-deny 0.16.3`は`bans ok`と`sources ok`を報告しました。このdeny versionはCVSS 4.0を解析できないため、advisory modeは`cargo-audit`で代替しました。重複する`hashbrown`と`syn`は、直接依存の重複ではなくRatatuiの依存グラフ由来でした。

静的検索では、secret、`unsafe`、`TODO`/`FIXME`、debug macro、直接の`anyhow`/`thiserror`、アプリケーション経路の`unwrap`/`expect`は見つかりませんでした。本番コードでprocessを作るのは、`src/git/runner.rs`のシェルを使わない型付きGit許可リストだけでした。

## プラットフォームとリリース状況

GitHub ActionsはUbuntuとmacOSでtestとrelease buildを実行し、UbuntuではformatとClippyも実行します。Linuxの実ターミナル動作はローカルで検証済みです。手動チェックリストのmacOS行は、CIまたはrelease環境の実ターミナルで引き続き確認が必要です。

registryへの公開は`publish = false`で無効のままです。公開artifactまたはtagを作る前に、リポジトリ所有者が最終配布先を選ぶ必要があります。その判断と現在のcheckが完了してから[リリース手順](/ja/developer/release/)へ進んでください。
