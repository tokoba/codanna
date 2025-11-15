Path: integrations\claude-code.md

```markdown
# Claude Code Integration
# Claude Code 連携

Set up Codanna with Claude's official CLI.
Claude 公式 CLI を使用して Codanna をセットアップします。

## Configuration
## 設定

Add this to your local `.mcp.json`:
ローカルの `.mcp.json` に次を追加します:

```json
{
  "mcpServers": {
    "codanna": {
      "command": "codanna",
      "args": ["serve", "--watch"]
    }
  }
}
```

## Features
## 機能

- File watching with `--watch` flag  
  `--watch` フラグによるファイル監視
- Auto-reload on index changes  
  インデックス変更時の自動リロード
- stdio transport (default)  
  標準入出力 (stdio) トランスポート (デフォルト)

## Verification
## 接続確認

After configuration, verify the connection:
設定後、接続を確認します:

```bash
codanna mcp-test
```

This will confirm Claude can connect and list available tools.
これにより Claude が接続でき、利用可能なツールを一覧できることを確認します。

## Agent Workflow
## エージェントワークフロー

Tool priority:  
ツールの優先度:
- **Tier 1**: semantic_search_with_context, analyze_impact
- **Tier 2**: find_symbol, get_calls, find_callers
- **Tier 3**: search_symbols, semantic_search_docs, get_index_info

Workflow:  
ワークフロー:
1. semantic_search_with_context - Find relevant code with context  
   semantic_search_with_context - コンテキスト付きで関連コードを検索
2. analyze_impact - Map dependencies and change radius  
   analyze_impact - 依存関係と変更範囲をマッピング
3. find_symbol, get_calls, find_callers - Get specific details  
   find_symbol, get_calls, find_callers - 具体的な詳細を取得

Start with semantic search, then narrow with specific queries.  
まずセマンティック検索から始め、その後特定のクエリで絞り込みます。

## Troubleshooting
## トラブルシューティング

- Ensure Codanna is in your PATH  
  Codanna が PATH に含まれていることを確認してください
- Check `.codanna/settings.toml` exists in your project  
  プロジェクトに `.codanna/settings.toml` が存在することを確認してください
- Run `codanna index` before starting the server  
  サーバーを起動する前に `codanna index` を実行してください

## See Also
## 参考

- [MCP Tools Reference](../user-guide/mcp-tools.md)
- [Agent Guidance](agent-guidance.md)
- [Configuration](../user-guide/configuration.md)
```