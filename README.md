# moomoo-mcp

Rust 製の moomoo OpenD 向け MCP サーバです。ローカルの `OpenD` に TCP 接続し、quote / trade / auth recovery を MCP ツールとして公開します。

## ツール

quote:

- `get_global_state`
- `get_auth_status`
- `get_static_info`
- `get_trade_dates`
- `get_quote_subscriptions`
- `subscribe_quotes`
- `unsubscribe_quotes`
- `get_basic_quote`
- `get_security_snapshot`
- `get_history_kl`

trade:

- `list_accounts`
- `unlock_trade`
- `get_funds`
- `get_max_trade_qtys`
- `get_positions`
- `get_order_fills`
- `get_orders`
- `get_history_orders`
- `get_history_order_fills`
- `get_order_fee`
- `place_order`
- `modify_order`

auth / Operation Command:

- `relogin_opend`
- `request_phone_verify_code`
- `submit_phone_verify_code`
- `request_picture_verify_code`
- `submit_picture_verify_code`

## 前提

- moomoo OpenD が起動済みであること
- デフォルト接続先は `127.0.0.1:11111`
- auth 系ツールを使うなら OpenD の Telnet / Operation Command を有効化し、通常は `127.0.0.1:22222` を開けること

初回ログインやアカウント切替は OpenD 側の責務です。このサーバは運用中の再認証と API 利用を対象にしています。

## クイックスタート

```bash
cargo build --release
./target/release/moomoo-mcp
```

## 環境変数

- `MOOMOO_HOST`
  - default: `127.0.0.1`
- `MOOMOO_PORT`
  - default: `11111`
- `MOOMOO_OPEND_TELNET_HOST`
  - default: `MOOMOO_HOST`
- `MOOMOO_OPEND_TELNET_PORT`
  - auth 系ツール用
- `MOOMOO_OPEND_TELNET_TIMEOUT_MS`
  - default: `500`
- `MOOMOO_USE_ENCRYPTION`
  - default: `false`
- `MOOMOO_RSA_PRIVATE_KEY_PATH`
  - OpenD 側でカスタム RSA 鍵を使う場合
- `MOOMOO_CLIENT_ID`
  - default: `moomoo-mcp-<pid>`
- `MOOMOO_CLIENT_VER`
  - default: `300`
- `MOOMOO_RECV_NOTIFY`
  - default: `false`

## MCP 設定例

```json
{
  "mcpServers": {
    "moomoo": {
      "command": "/absolute/path/to/moomoo-mcp/target/release/moomoo-mcp",
      "env": {
        "MOOMOO_HOST": "127.0.0.1",
        "MOOMOO_PORT": "11111",
        "MOOMOO_OPEND_TELNET_HOST": "127.0.0.1",
        "MOOMOO_OPEND_TELNET_PORT": "22222",
        "MOOMOO_USE_ENCRYPTION": "false"
      }
    }
  }
}
```

## 実装方針

- request/response 型の MCP ツールを優先
- `get_basic_quote` は OpenD の仕様に合わせて `BASIC` subscription を前提にする
- 発注系は重複発注回避のため自動リトライしない
- JSON 変換は `prost-reflect` を使う

## 使用上の注意

- `place_order` / `modify_order` は real account に実注文を送る可能性があります
- `unlock_trade` 中のローカル process は強く信頼されます
- 初回ログインやアカウント切替そのものは OpenD の責務です

## 検証

```bash
cargo fmt --all
cargo test
cargo build --release
```

## 公式ドキュメント

- [OpenAPI Introduction](https://openapi.moomoo.com/moomoo-api-doc/en/intro/intro.html)
- [Quote API Overview](https://openapi.moomoo.com/moomoo-api-doc/en/quote/overview.html)
- [Trade API Overview](https://openapi.moomoo.com/moomoo-api-doc/en/trade/overview.html)
- [Operation Command](https://openapi.moomoo.com/moomoo-api-doc/en/opend/opend-operate.html)
- [Quote API Overview](https://openapi.moomoo.com/moomoo-api-doc/en/quote/overview.html)
- [Trade API Overview](https://openapi.moomoo.com/moomoo-api-doc/en/trade/overview.html)
- [Operation Command](https://openapi.moomoo.com/moomoo-api-doc/en/opend/opend-operate.html)
