# Agent Guidance

プロジェクトの指示（`CLAUDE.md`、`AGENTS.md`、またはシステムプロンプト）に以下を追加すると、最適に活用できます。

```markdown
## Codanna MCP Tools

Tool priority:
- **Tier 1**: semantic_search_with_context, analyze_impact
- **Tier 2**: find_symbol, get_calls, find_callers
- **Tier 3**: search_symbols, semantic_search_docs, get_index_info

Workflow:
1. semantic_search_with_context - Find relevant code with context
2. analyze_impact - Map dependencies and change radius
3. find_symbol, get_calls, find_callers - Get specific details

Start with semantic search, then narrow with specific queries.
```

## Claude Sub Agent

**codanna-navigator** サブエージェント（`.claude/agents/codanna-navigator.md`）を同梱しており、Codanna を効果的に活用できます。

## Agent Steering

Codanna のガイダンスはモデル向けです。各ツールのレスポンスには LLM が読み取り行動する `system_message` が含まれます。人間には表示されません。このメッセージはエージェントに次の手順――掘り下げ、呼び出し追跡、影響分析、クエリの洗練――を指示します。

### Behavior Examples

```json
{
  "system_message": "Found 1 match. Use 'find_symbol' or 'get_calls' next."
}
```

```json
{
  "system_message": "Found 18 callers. Run 'analyze_impact' to map the change radius."
}
```

```json
{
  "system_message": "No semantic matches. Try broader phrasing or ensure docs exist."
}
```

## Configuration

設定はプレーン TOML 形式 `.codanna/settings.toml` で行います。

```toml
[guidance]
enabled = true

[guidance.templates.find_callers]
no_results = "No callers found. Might be an entry point or dynamic dispatch."
single_result = "Found 1 caller. Use 'find_symbol' to inspect usage."
multiple_results = "Found {result_count} callers. Try 'analyze_impact' for the full graph."

[guidance.templates.analyze_impact]
no_results = "No impact detected. Likely isolated."
single_result = "Minimal impact radius."
multiple_results = "Impact touches {result_count} symbols. Focus critical paths."

[[guidance.templates.analyze_impact.custom]]
min = 20
template = "Significant impact with {result_count} symbols. Break the change into smaller parts."
```

## Why It Matters

- ラウンドトリップを削減。エージェントが次のコマンドを自動提案。
- 余分な解説を減らし、実行を重視。
- 「Grep-and-hope」が指向性のあるホップへ。

## Claude Slash Commands

Codanna には、コード探索をインテリジェントに支援するカスタムスラッシュコマンドが含まれています。

| Command | Description | Example Report |
|---------|-------------|----------------|
| `/find <query>` | 自然言語によるスマートなセマンティック検索 ― シンボル、パターン、実装を最適化クエリで検索 | [Language Registry Investigation](../../reports/find/find-language-registry-scaffold.md) |
| `/deps <symbol>` | シンボルの依存関係を分析 ― 依存先、依存元、結合度メトリクス、リファクタリング機会を表示 | [find_symbol Dependencies](../../reports/deps/find_symbol-method-dependencies.md) |

これらのコマンドは裏で Codanna の MCP ツールを利用し、包括的な分析と自動レポート生成を提供します。

## Extracting System Messages

システムメッセージはエージェントを導きますが、ユーザーには非表示です。パイプ処理で抽出できます。

```bash
# ツールレスポンスからシステムガイダンスを抽出
codanna mcp find_callers walk_and_stream --json | jq -r '.system_message'
# 出力例: Found 18 callers. Run 'analyze_impact' to map the change radius.
```

## See Also

- [MCP Tools](../user-guide/mcp-tools.md)
- [Claude Code Integration](claude-code.md)
- [Configuration](../user-guide/configuration.md)