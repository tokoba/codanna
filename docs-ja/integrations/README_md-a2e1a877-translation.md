Path: integrations\README.md

```markdown
[Documentation](../README.md) / **Integrations**

---

# インテグレーション

codanna を MCP サーバーとして導入し、エージェントをそれに向けるだけで、あいまいな回答が領収書付きの回答に早変わりします。

## 利用可能なインテグレーション

- **[Claude Code](claude-code.md)** - Claude 公式 CLI
- **[Claude Desktop](claude-desktop.md)** - デスクトップアプリの設定
- **[Codex CLI](codex-cli.md)** - 代替 CLI クライアント
- **[HTTP/HTTPS Server](http-server.md)** - リアルタイムでファイル監視を行う常駐サーバー
- **[Agent Guidance](agent-guidance.md)** - システムメッセージと誘導

## プラグインによる拡張

- **[Plugin System](../plugins/)** - Claude Code に独自のコマンド、エージェント、MCP サーバーを追加

## クイックセットアップ

### Claude Code
```json
# Add this to your local .mcp.json:
{
  "mcpServers": {
    "codanna": {
      "command": "codanna",
      "args": ["serve", "--watch"]
    }
  }
}
```

### Claude Desktop
Claude Desktop では異なる場所から実行されるため `--config` フラグが必要です。

`~/Library/Application Support/Claude/claude_desktop_config.json` (Mac) に設定してください:
```json
{
  "mcpServers": {
    "codanna": {
      "command": "codanna",
      "args": ["--config", "/absolute/path/to/your/project/.codanna/settings.toml", "serve", "--watch"]
    }
  }
}
```

`/absolute/path/to/your/project/` を実際のプロジェクトパスに置き換えてください。

## エージェントワークフロー

ツールの優先順位:
- **Tier 1**: semantic_search_with_context, analyze_impact
- **Tier 2**: find_symbol, get_calls, find_callers
- **Tier 3**: search_symbols, semantic_search_docs, get_index_info

ワークフロー:
1. semantic_search_with_context - コンテキスト付きで関連コードを検索
2. analyze_impact - 依存関係と変更範囲をマッピング
3. find_symbol, get_calls, find_callers - 具体的な詳細を取得

まずセマンティック検索を行い、その後に特定のクエリで絞り込みます。

## 次のステップ

- 最適な誘導のために [Agent Guidance](agent-guidance.md) を設定
- [MCP Tools](../user-guide/mcp-tools.md) を詳細に学習
- [Advanced](../advanced/) 機能を探索

[ドキュメントに戻る](../README.md)
```