# AGENTS.md

このリポジトリは `moomoo` の `OpenD` に接続する Rust 製 MCP サーバです。

## 目的

- `OpenD` の TCP / protobuf API を MCP ツールとして公開する
- 気配取得と売買系 API をローカル MCP サーバ経由で利用できるようにする

## 重要な注意

- `place_order` と `modify_order` は実口座に影響する可能性がある
- 本番確認前は `trd_env=SIMULATE` を優先する
- 書き込み系 API は重複実行しない
- 書き込み系 API に安易な自動リトライを入れない
- `OpenD` 側のログイン状態と口座状態に依存するため、実接続確認なしで本番安全性を断言しない
- 初回ログインとアカウント切り替えは OpenD 側の責務であり、MCP 側では運用中の再認証を優先する

## 開発コマンド

```bash
cargo check
cargo test
cargo build --release
cargo fmt --all
```

## 主要ファイル

- `src/main.rs`
  - MCP stdio サーバの起動
- `src/server.rs`
  - MCP ツール定義
- `src/opend.rs`
  - OpenD transport
  - `InitConnect`
  - 暗号化
  - `KeepAlive`
- `src/opend_cmd.rs`
  - OpenD Operation Command / Telnet
  - `relogin`
  - phone / picture verification
- `src/config.rs`
  - 環境変数設定
- `src/proto.rs`
  - `prost` 生成型の取り込み
  - `prost-reflect` による JSON 変換
- `build.rs`
  - proto コンパイル
- `proto/`
  - 公式 Python SDK 由来の `.proto`
- `vendor/moomoo/conn_key.pem`
  - 既定 RSA 鍵

## 実装方針

- transport の仕様変更は `src/opend.rs` を最初に確認する
- 認証や運用コマンドの変更は `src/opend_cmd.rs` と公式 Operation Command を確認する
- 新しい API を追加する場合は、まず対応する `.proto` を `proto/` に追加する
- JSON 変換は手書き struct を増やさず、できるだけ `prost-reflect` を使う
- 新しいツール追加時は入力の enum 文字列を正規化してから数値へ変換する
- 市場コードは気配系では `US.AAPL` のような市場付きコードを使う
- 発注系では raw code と `sec_market` の扱いを崩さない

## 環境変数

- `MOOMOO_HOST`
- `MOOMOO_PORT`
- `MOOMOO_USE_ENCRYPTION`
- `MOOMOO_OPEND_TELNET_HOST`
- `MOOMOO_OPEND_TELNET_PORT`
- `MOOMOO_OPEND_TELNET_TIMEOUT_MS`
- `MOOMOO_RSA_PRIVATE_KEY_PATH`
- `MOOMOO_CLIENT_ID`
- `MOOMOO_CLIENT_VER`
- `MOOMOO_RECV_NOTIFY`

詳細は `README.md` を参照。

## 変更時のチェックポイント

- proto2 の `optional` / `required` が `prost` 上でどう生成されるか確認する
- `C2S` が `prost` では `C2s` になるような命名差に注意する
- OpenD 応答の `ret_type` / `ret_msg` / `err_code` を必ず評価する
- 暗号化時は `InitConnect` が RSA、その後が AES-CBC であることを崩さない
- `KeepAlive` の送信間隔は `keep_alive_interval * 4 / 5` 相当を維持する
- auth 系ツールは OpenD の Telnet 有効化が前提であることを README に反映する

## 追加候補

- push 購読対応
- 約定履歴取得
- 銘柄検索系ツール
- Docker 化
