```markdown
# Unix パイプ処理

Codanna はあなた同様に CLI を操ります。シンプルな場合は位置引数、複雑な場合は key:value。  
すべての MCP ツールは `--json` をサポートしているため、パイプはノイズではなく音楽です。

## 基本的なパイプ処理

### 言語フィルター付き MCP セマンティック検索
```bash
codanna mcp semantic_search_with_context query:"error handling" limit:2 lang:rust --json | jq -r '.data[] | "\(.symbol.name) (\(.symbol.scope_context)) (score: \(.score)) - \(.context.file_path) - \(.symbol.doc_comment)"'
# Output: error (ClassMember) (score: 0.6421908) - src/io/format.rs:148 - Create a generic error response.
#         add_error (ClassMember) (score: 0.6356536) - src/indexing/progress.rs:46 - Add an error (limited to first 100 errors)
```

### シンボルの型・名前・場所を表示
```bash
codanna retrieve search "config" --json | jq -r '.items[] | "\(.symbol.kind) \(.symbol.name) @ \(.file_path)"'
# Output: Function test_partial_config @ src/config.rs:911
#         Method config_key @ src/parsing/language.rs:114

# 検索結果のファイルパスを一意に取得
codanna retrieve search "parser" --json | jq -r '.items[].file_path' | sort -u

# スコープ情報付きで関数シグネチャを抽出
codanna retrieve search "create_parser" --json | jq -r '.items[] | "\(.symbol.name) (\(.symbol.scope_context)) - \(.file_path)\n  \(.symbol.signature)"'
```

## 応用パイプ処理: システムメッセージ抽出とコールグラフのマッピング

システムメッセージはエージェントを次のホップへ導きます。人間には見えませんが、`jq` で可視化できます:

```bash
# ツールレスポンスからシステムガイダンスを抽出
codanna mcp find_callers walk_and_stream --json | jq -r '.system_message'
# Output: Found 18 callers. Run 'analyze_impact' to map the change radius.

# 完全なコールグラフを構築: シンボルを見つけ、その呼び出し先を表示し、さらに1段階深掘り
codanna mcp semantic_search_with_context query:"file processing" limit:1 --json | \
jq -r '.data[0].symbol.name' | \
xargs -I {} sh -c '
  echo "=== Symbol: {} ==="
  codanna mcp get_calls {} --json | jq -r ".data[]? | \"\(.name) - \(.file_path):\(.range.start_line)-\(.range.end_line)\""
'
# Output:
# === Symbol: walk_and_stream ===
# process_entry - src/io/parse.rs:285-291
# parse_file - src/io/parse.rs:219-282
# ...

# 逆方向: 重要な関数を呼び出している箇所と正確な行範囲を表示
codanna mcp find_callers parse_file --json | \
jq -r '.data[]? | "\(.name) (\(.kind)) - \(.file_path):\(.range.start_line)-\(.range.end_line)"'
# Output:
# walk_and_stream (Function) - src/io/parse.rs:144-213
# index_project (Method) - src/indexing/mod.rs:423-502
```

## よく使うパターン

### 検索とカウント
```bash
# 型別にシンボル数をカウント
codanna retrieve search "" --json | jq -r '.items[].symbol.kind' | sort | uniq -c | sort -rn

# ファイルごとのシンボル数をカウント
codanna retrieve search "" --json | jq -r '.items[].file_path' | sort | uniq -c | sort -rn
```

### フィルターと変換
```bash
# すべての public 関数を取得
codanna retrieve search "" --json | jq -r '.items[] | select(.symbol.kind == "Function") | .symbol.name'

# 構造体とそのファイル位置を取得
codanna retrieve search "" --json | jq -r '.items[] | select(.symbol.kind == "Struct") | "\(.symbol.name) in \(.file_path)"'
```

### コマンド連鎖
```bash
# あるトレイトとその実装を探す
TRAIT="Parser"
echo "=== Trait: $TRAIT ==="
codanna mcp find_symbol $TRAIT --json | jq -r '.data[0].file_path'
echo "=== Implementations ==="
codanna retrieve implementations $TRAIT --json | jq -r '.items[].symbol.name'
```

### 依存関係の解析
```bash
# シンボルの完全な依存関係グラフを取得
SYMBOL="SimpleIndexer"
codanna retrieve dependencies $SYMBOL --json | \
jq -r '.dependencies[] | "\(.name) (\(.kind)) - \(.file_path)"'
```

## ヒント

- すべてのコマンドで `--json` フラグを使用して構造化出力を得る  
- `jq` と組み合わせて JSON を加工  
- `sort`, `uniq`, `grep`, `awk` など標準 Unix ツールと併用  
- `xargs` で出力を基にコマンドを連鎖  
- 複雑なパイプラインはシェルスクリプトとして保存  

## 退出コード

すべての `retrieve` コマンドは、対象が見つからない場合に終了コード 3 を返します。スクリプトで便利です:

```bash
if codanna retrieve symbol MySymbol --json > /dev/null 2>&1; then
    echo "Symbol found"
else
    if [ $? -eq 3 ]; then
        echo "Symbol not found"
    else
        echo "Error occurred"
    fi
fi
```

## 関連項目

- [CLI リファレンス](../user-guide/cli-reference.md)
- [MCP ツール](../user-guide/mcp-tools.md)
```