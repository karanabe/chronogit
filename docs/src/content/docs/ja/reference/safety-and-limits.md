---
title: 安全性、上限、非目標
description: 閲覧とブランチ切替の保証、リソース上限、サポートしない操作を説明します。
tags:
  - 安全性
  - 上限
  - セキュリティ
sidebar:
  order: 2
---

ChronoGitはリポジトリの内容と設定を信頼できない入力として扱います。Gitとの境界は閉じており、シェルを使わず、リソースに上限があります。

## 閲覧とブランチ切替の契約

アプリが要求できるのは、リポジトリ検出、bare/`HEAD`確認、ワークツリー状態、履歴、メッセージ、変更ファイル、差分、ツリー項目、リポジトリファイル一覧、固定文字列grep、ローカルブランチ一覧、明示的なローカルブランチ切替の型付き操作だけです。任意のGit引数を実行する経路はありません。

- シェルを介さずGitを直接起動します。
- リポジトリパスとpathspecは別々のプロセス引数とし、該当するコマンドでは`--`の後ろに渡します。履歴ファイル全文は、検証済み16進object IDと検証済み相対リポジトリパスからだけ組み立てた1つの`object:path`引数で読みます。
- 任意のGitロックとターミナルプロンプトを無効にします。
- pager、色付け、外部diff driver、textconv、fsmonitorの実行を無効にします。
- revisionとして再利用するobject IDは、16進数として検証済みの値だけです。
- 現在ファイルの読み取りは、検出済みワークツリーディレクトリから各パス要素を相対的に開き、シンボリックリンクをたどらず、8 MiBで停止します。
- 履歴ファイル全文はGit object databaseから読み、revisionをcheckoutせず、同じ8 MiB上限を適用します。
- キーマップファイルは文書化されたaction名とキー名だけを受け付け、コマンドを実行できません。

リポジトリを書き換える操作は、`Space b`で選んだ既存ローカルブランチへの切替だけです。Gitの通常の競合検査を使い、force、merge、自動stash、remoteからのブランチ作成は行いません。hookとsubmoduleの再帰更新は無効にします。Gitに設定されたcheckout filterは実行される場合があるため、ブランチ切替には信頼できるリポジトリを使ってください。ステージ、復元、コミット、リセット、ブランチ作成の操作は提供しません。

## 明示的なLSP trust boundary

language server対応によってChronoGitのGit保証は変わりませんが、外部serverは独自に動作する別processです。`--lsp PROFILE`を指定しない限りLSPは無効です。信頼できるrepositoryだけで有効化してください。rust-analyzerはbuild scriptやprocedural macroを評価する場合があり、Java/Python serverはproject toolingの起動、environment参照、そのtoolingを介したdependency取得、cache/build artifactの書き込みを行う場合があります。

ChronoGitはserverを同梱・downloadしません。検証済みのtrusted user-level引数配列を直接起動し、repository設定からserver commandを読み込まず、暗黙のshell展開もしません。JDT workspace dataと書き込み可能なOSGi configurationはprocessごとにrepository外の固有temporary treeへ置きます。同期するのは表示中の完全なUTF-8 fileだけです。位置を使う要求ではcurrent working-tree text、document-symbol contextでは選択コミットのnew-state textも対象になります。返されたlocationは`file:` pathがrepository内に残り、既存no-follow readerを通る場合だけ開きます。外部URIとvirtual URIはnoticeとして表示するだけです。

:::note[外部からの同時変更]
閲覧操作は読み取り専用で、明示的なブランチ切替だけがリポジトリを変更します。エディター、別の場所で開始したhook、他のGitプロセスはTUIの起動中にも変更できます。変更後は`r`で更新してください。
:::

## リソース上限

| リソース | 上限または動作 |
| --- | --- |
| Git標準出力 | 1コマンドあたり8 MiB |
| Git標準エラー | 1コマンドあたり64 KiB |
| Gitコマンド時間 | 30秒 |
| 同時Git読み取り | 最大2 |
| 差分要求のdebounce | 75 ms |
| live repository searchのdebounce | 100 ms |
| 差分キャッシュ | 16項目、合計16 MiB |
| 履歴ページ | 200コミット |
| ファイル履歴 | 200コミット |
| 現在または履歴のファイル全文 | 8 MiB |
| LSP message body | 受信・送信とも8 MiB |
| LSP response header | 16 KiB |
| LSP writer queue | sessionごとに64 message |
| 常駐LSP session | profile/workspaceの組を最大4、LRUで終了 |
| 同期document | sessionごとに完全な1 file、最大8 MiB |
| LSP navigation / hover / document-symbol要求 | 15秒 |
| 正規化後hover text | 262,144文字 |
| LSP initialize要求 | 30秒 |
| LSP shutdown猶予 | child終了まで2秒 |
| 保持するLSP stderr | 末尾16 KiB |
| semantic jump履歴 | 古い/新しいlocationの合計64件 |
| 連続キーの入力間隔 | 750 ms |
| 最小ターミナル | 80×24 |
| Changesの複数ペイン境界 | 110列（Historyは常に3段表示） |

テキスト差分が標準出力上限に達するとプロセスを止め、取得できたパッチを切り詰め済みとして表示します。status、log、treeなど機械可読な結果は、途中までの内容を解析せずエラーにします。タイムアウトや標準エラー上限超過は復旧可能なエラーになります。

## サポート環境

バージョン`0.6.0`はLinux、macOS、bareではないリポジトリ、対話型ターミナルをサポートします。Unixのパスは内部でバイト列のまま保持します。Windowsは現在の互換性境界外です。

## `0.6.0`の非目標

ChronoGitは次の機能を提供しません。

- ステージ済み変更の確認
- 既存ローカルブランチへの切替以外のリポジトリ変更
- remote、pull request、blame、stashのワークフロー
- エディター、プラグイン実行環境
- マージの結合差分や選択可能な親との差分
- 機械可読エクスポート、バッチモード、非対話UI
- サブモジュールリポジトリ内への移動
- active repository外のdependency、standard library、archive、virtual LSP documentを開くこと
