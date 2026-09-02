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
cargo package --locked
```

ドキュメント変更では、サイトを別途ビルドします。

```sh title="ターミナル"
pnpm --dir docs install --frozen-lockfile
pnpm --dir docs build
```

対象を絞ったRustコマンドには`cargo test --lib`、`cargo test --test git_service`、`cargo test --test cli`があります。絞ったテストの成功を、リリース全体の証拠として扱わないでください。

## カバレッジの構成

- domainとparserのunit testが、値の不変条件、バイト解析、行番号、不正出力を扱います。
- reducerとeffectのtestが、Historyの全panel、浮動Graph詳細と戻る操作、live repository searchの入力、Search/Resultsフォーカスの往復と再検索、古い結果の拒否と要求集約、file history/current contentの遷移、全文message/content overlay、本文レイアウトの遷移/選択/scroll、pagination、tree展開、差分フロートのopen/scroll/検索/close遷移、差分読み込み中の移動入力、message読み込み、古い応答の拒否、差分要求の集約を扱います。
- renderとkeyのtestが、loading、empty、failure、truncation、最小幅のHistoryレイアウト、Graphのlaneと詳細、repository searchのフォーカス案内と結果、file historyとcurrent content、message overlay、ソースとdiff hunkのシンタックスハイライト、コードを邪魔しないガターマーカー移動、控えめな差分種別色、差分全体のscroll、比較ラベル、既定/設定済みbinding、leader sequence、曖昧なキー設定の拒否を扱います。
- 一時リポジトリのintegration testが、root/merge commit、detached/unborn repository、linked worktree、staged-only/mixed change、file/fixed-text search、現在/過去のfile content、中間シンボリックリンクの拒否、conflict、rename、delete、type change、symlink、submodule、binary/oversized diff、非UTF-8/先頭dash pathを扱います。
- CLI testが、help/version、非repository診断、Git未検出、権限エラー、非TTY拒否、ターミナル設定前の不正な明示keymap fileを扱います。
- 読み取り専用integration checkが、Git service操作の前後で`HEAD`、porcelain status、worktree bytesを比較します。
- ターミナルライフサイクルtestが、復元シーケンスと意図的にpanicするchild processを検査します。

## 現在の変更の検証

このrevisionでは、上記のRust完全ゲートがunit、binary、CLI、Git service、rustdocの計90 testとlock済みrelease buildを含めて成功しました。`cargo package --allow-dirty --locked`は46ファイルのcrate（397.3 KiB、圧縮後85.0 KiB）を生成・再buildし、`cargo publish --dry-run --allow-dirty --locked`もuploadせずに完了しました。ドキュメントbuildも33ページを生成して成功しました。

最新のpromptキー変更より前に、このリポジトリに対する実80×24 PTYでもrelease binaryを操作しました。Graphはparent lane付きで表示され、`Enter`で変更ファイル/diff詳細がGraphに重なる中央のfloating windowとして開きました。`q`でGraphへ戻りました。`Space f`でrepository file searchを開き、`R`を入力するとResultsがlive更新され、`Ctrl-j`でResultsへ移り、`Ctrl-k`で`R`を維持したSearchへ戻りました。`E`を追加すると再びlive searchが発行され、READMEのpathだけに絞られました。現在の`q`/`Q`クエリ動作と中間シンボリックリンク拒否は、対象を絞った自動回帰テストで検証しています。公開前には更新済みのplatform smoke test行を完了する必要があります。fixed-text search、古い結果の破棄、close/back動作、Ctrl-C終了はintegration、reducer、executor、keymap、rendering testでも検証しています。以前coreutilsで実施したHistory確認は下の履歴記録に残しています。

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

GitHub Actionsの`CI` workflowは手動実行専用で、pushやpull requestでは起動しません。リモートcheckが必要な場合は、maintainerがリポジトリの**Actions** tabを開き、**CI**を選択して**Run workflow**を実行します。このworkflowはUbuntuとmacOSでtestとrelease buildを実行し、UbuntuではformatとClippyも実行します。Linuxの実ターミナル動作はローカルで検証済みです。手動チェックリストのmacOS行は、CIまたはrelease環境の実ターミナルで引き続き確認が必要です。

manifestは公開先をcrates.ioだけに限定し、registryへ送る内容をapplication・test sourceとユーザー向けrelease fileに絞っています。公開には、権限を持つmaintainerが現在のcheckと[リリース手順](/ja/developer/release/)の明示的な公開操作を完了する必要があります。
