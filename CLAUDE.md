# CLAUDE.md

Claude でこのリポジトリを触るときのメモです。

## 最初に見る場所

- `README.md`
- `src/server.rs`
- `src/opend.rs`
- `src/opend_cmd.rs`

## このリポジトリで優先すること

- まず `cargo check` を通す
- API 追加時は `.proto` と `build.rs` の整合を保つ
- OpenD の実運用リスクがある変更では、実注文につながるかを明示する

## 作業ルール

- ドキュメントだけの変更でなければ、最低でも `cargo check` を実行する
- Rust の型名は `prost` 生成名に合わせる
- proto2 の optional field は `Option<T>` になる前提で書く
- レスポンス JSON は `message_to_json()` を優先して再利用する
- 書き込み系ツールの変更では idempotency を安易に仮定しない
- OpenD の auth 周りは API port と Telnet port の分担を崩さない

## よく使う確認

```bash
cargo check
cargo test
rg "PROTO_ID_" src
rg "parse_named_enum" src/server.rs
```

## 注意点

- `OpenD` が未ログインだと多くのツールは正常動作しない
- 初回ログインやアカウント切り替えは OpenD UI / 起動設定の責務
- 再認証系ツールは `MOOMOO_OPEND_TELNET_PORT` と OpenD 側の Telnet 有効化が必要
- `MOOMOO_USE_ENCRYPTION=true` の場合、OpenD 側設定と RSA 鍵が一致している必要がある
- `place_order` / `modify_order` の変更は、既存の安全側の挙動を崩さない

## ツール追加の流れ

1. 対応する `.proto` を `proto/` に追加
2. `cargo check` で生成コードを確認
3. `src/server.rs` に request schema と tool を追加
4. 必要なら enum map を追加
5. `README.md` も更新
