Path: advanced\README.md

```markdown
[Documentation](../README.md) / **高度な機能**

---

# 高度な機能

## オタク向けセクション

Codanna は `.gitignore` を尊重し、独自に `.codannaignore` を追加します。

## Unix ネイティブ。パイプで行こう！

Codanna はあなたと同じく CLI を話します。シンプルなときは位置引数、そうでないときは key:value 形式。
すべての MCP ツールは `--json` をサポートしているため、パイプはノイズではなく音楽です。

## このセクションで扱う内容

- **[Unix Piping](unix-piping.md)** - 高度なパイプワークフローと例
- **[Slash Commands](slash-commands.md)** - カスタム /find と /deps コマンド
- **[Project Resolution](project-resolution.md)** - TypeScript の tsconfig.json とパスエイリアス
- **[Performance](performance.md)** - ベンチマークと最適化

## クイック例

### 言語フィルター付きセマンティック検索
```bash
codanna mcp semantic_search_with_context query:"error handling" limit:2 lang:rust --json | jq -r '.data[] | "\(.symbol.name) (\(.symbol.scope_context)) (score: \(.score)) - \(.context.file_path) - \(.symbol.doc_comment)"'
```

### 完全な呼び出しグラフの構築
```bash
# シンボルを見つけてその呼び出し先を表示し、更に1段深く追跡
codanna mcp semantic_search_with_context query:"file processing" limit:1 --json | \
jq -r '.data[0].symbol.id' | \
xargs -I {} sh -c '
  echo "=== Symbol ID: {} ==="
  codanna mcp get_calls symbol_id:{} --json | jq -r ".data[]? | \"\(.name) [symbol_id:\(.id)] - \(.file_path):\(.range.start_line)-\(.range.end_line)\""
'
```

### symbol_id を用いた曖昧性のないクエリ
```bash
# 検索結果から symbol_id を取得し、正確な後続操作に使用
codanna mcp semantic_search_with_context query:"error handling" limit:1 --json | \
jq -r '.data[0] | "Symbol: \(.symbol.name) [symbol_id:\(.symbol.id)]"'

# ID で直接ルックアップ（曖昧性なし）
codanna mcp get_calls symbol_id:1883 --json | jq -r '.data[] | "\(.name) [symbol_id:\(.id)]"'
```

### システムメッセージの抽出
システムメッセージはエージェントが次に進むべき手順を示します。人間には表示されませんが、jq でパイプすれば見えます。
```bash
codanna mcp find_callers walk_and_stream --json | jq -r '.system_message'
# Output: Found 18 callers. Run 'analyze_impact' to map the change radius.
```

## なぜ重要か

- ラウンドトリップが減少。エージェントが次のコマンドを自動提案。
- 説明より実行が中心。
- grep に頼る試行錯誤が、誘導されたホップへ。

## 次のステップ

- [Architecture](../architecture/) の内部構造を探る
- 基本的な使い方は [User Guide](../user-guide/) を参照
- 仕様は [Reference](../reference/) を確認

[Back to Documentation](../README.md)
```