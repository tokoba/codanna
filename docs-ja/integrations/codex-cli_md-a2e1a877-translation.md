```markdown
# Codex CLI 統合

Codanna は標準的な MCP サーバーとして Codex CLI と連携します。

## 設定

`~/.codex/config.toml` に以下を設定します:

```toml
[mcp_servers.codanna]
command = "codanna"
args = ["serve", "--watch"]
startup_timeout_ms = 20_000
```

## 機能

- 標準 MCP サーバー統合
- ファイル監視機能
- 起動タイムアウトの設定が可能

## 検証

設定後、接続を検証します:

```bash
codanna mcp-test
```

## 使い方

一度設定すると、Codex CLI は必要に応じて自動的に Codanna を起動し、すべての MCP ツールへアクセスできます。

## トラブルシューティング

- Codanna が PATH に含まれていることを確認してください
- プロジェクトに `.codanna/settings.toml` が存在することを確認してください
- 大規模なコードベースでインデックス作成に時間がかかる場合は `startup_timeout_ms` を調整してください

## 参考

- [MCP ツールリファレンス](../user-guide/mcp-tools.md)
- [設定](../user-guide/configuration.md)
```