```markdown
# MCP Tools Reference

MCP サーバーで使用できるツール一覧。すべてのツールは構造化出力用に `--json` フラグをサポートします。

## Tool Categories

### Discovery Tools
- **find_symbol** - シンボルを正確な名前で検索
- **search_symbols** - ファジー一致を含む全文検索
- **semantic_search_docs** - 自然言語検索
- **semantic_search_with_context** - 関係性を含む自然言語検索

### Relationship Tools
- **get_calls** - 関数が呼び出す関数を取得
- **find_callers** - 関数を呼び出す関数を取得
- **analyze_impact** - シンボル変更の影響範囲を解析

### Information Tools
- **get_index_info** - インデックス統計情報

## Tool Details

### `find_symbol`

正確な名前でシンボルを検索します。

**Parameters:**
- `name` (必須) - 検索するシンボル名（完全一致）

**Example:**
```bash
codanna mcp find_symbol main
codanna mcp find_symbol Parser --json
```

**Returns:** ファイルパス、行番号、種類、シグネチャを含むシンボル情報。

### `search_symbols`

ファジーマッチを用いたシンボル全文検索。

**Parameters:**
- `query` (必須) - 検索クエリ（ファジーマッチ対応）
- `limit` - 最大取得件数（デフォルト: 10）
- `kind` - シンボル種別でフィルタ（例: "Function", "Struct", "Trait"）
- `module` - モジュールパスでフィルタ

**Example:**
```bash
codanna mcp search_symbols query:parse kind:function limit:10
codanna mcp search_symbols query:Parser --json
```

**Returns:** 関連度ランキング付きの一致シンボル一覧。

### `semantic_search_docs`

自然言語クエリで検索します。

**Parameters:**
- `query` (必須) - 自然言語検索クエリ
- `limit` - 最大取得件数（デフォルト: 10）
- `threshold` - 最低類似度スコア (0-1)
- `lang` - プログラミング言語でフィルタ（例: "rust", "typescript"）

**Example:**
```bash
codanna mcp semantic_search_docs query:"error handling" limit:5
codanna mcp semantic_search_docs query:"authentication" lang:rust limit:5
```

**Returns:** ドキュメントに基づく意味的に類似したシンボル。

### `semantic_search_with_context`

関係性を含む拡張コンテキスト付き自然言語検索。

**Parameters:**
- `query` (必須) - 自然言語検索クエリ
- `limit` - 最大取得件数（デフォルト: 5。各結果にフルコンテキスト含む）
- `threshold` - 最低類似度スコア (0-1)
- `lang` - プログラミング言語でフィルタ

**Example:**
```bash
codanna mcp semantic_search_with_context query:"parse files" threshold:0.7
codanna mcp semantic_search_with_context query:"parse config" lang:typescript limit:3
```

**Returns:** 以下を含むシンボル情報:
- ドキュメント
- そのシンボルを呼び出すもの
- そのシンボルが呼び出すもの
- 完全な影響グラフ（呼び出し、型使用、構成すべてを含む）

### `get_calls`

指定関数が呼び出す関数を表示します。

**Parameters:**
- `function_name` または `symbol_id` (いずれか必須) - 関数名またはシンボル ID

**Example:**
```bash
codanna mcp get_calls process_file
codanna mcp get_calls symbol_id:1883
codanna mcp get_calls main --json
```

**Returns:** 指定関数が呼び出す関数の一覧。各結果にはフォローアップ用に `[symbol_id:123]` が含まれます。

### `find_callers`

指定関数を呼び出す関数を表示します。

**Parameters:**
- `function_name` または `symbol_id` (いずれか必須) - 関数名またはシンボル ID

**Example:**
```bash
codanna mcp find_callers init
codanna mcp find_callers symbol_id:1883
codanna mcp find_callers parse_file --json
```

**Returns:** 指定関数を呼び出す関数の一覧。各結果には `[symbol_id:123]` が含まれます。

### `analyze_impact`

シンボル変更の影響範囲を解析します。

**Parameters:**
- `symbol_name` または `symbol_id` (いずれか必須) - シンボル名またはシンボル ID
- `max_depth` - 探索の最大深さ（デフォルト: 3）

**Example:**
```bash
codanna mcp analyze_impact Parser
codanna mcp analyze_impact symbol_id:1883
codanna mcp analyze_impact SimpleIndexer --json
```

**Returns:** 完全な依存グラフを表示:
- この関数を CALL するもの
- 型として USE するもの（フィールド、パラメータ、戻り値）
- RENDER/COMPOSE するもの（JSX `<Component>`, Rust 構造体フィールド等）
- ファイルを跨ぐ完全な依存グラフ
- 各結果に `[symbol_id:123]` を含む

### `get_index_info`

インデックスの統計情報とメタデータを取得します。

**Parameters:** なし

**Example:**
```bash
codanna mcp get_index_info
codanna mcp get_index_info --json
```

**Returns:**
- インデックスされたシンボル総数
- 言語別シンボル数
- 種別別シンボル数
- インデックス作成/更新タイムスタンプ
- ファイル数

## Understanding Relationship Types

### Calls
括弧付きの関数呼び出し
- `functionA()` が `functionB()` を呼び出す
- 表示ツール: `get_calls`, `find_callers`

### Uses
型依存、構成、レンダリング
- 関数パラメータ/戻り値: `fn process(data: MyType)`
- コンポーネントレンダリング: `<CustomButton>` (JSX)
- 構造体フィールド: `struct Container { inner: Type }`
- 表示ツール: `analyze_impact`

## Language Filtering

混在コードベース（例: Python バックエンド + TypeScript フロントエンド）の場合、`lang` パラメータでノイズを削減。

対応言語: rust, python, typescript, go, php, c, cpp

言語フィルタにより、複数言語で類似ドキュメントが存在する際の重複結果を最大 75% 削減し、同一の類似度スコアを維持します。

## JSON Output

すべてのツールは `--json` フラグで構造化出力をサポートし、パイプ処理に最適です:

```bash
# 特定フィールドを抽出
codanna mcp find_symbol Parser --json | jq '.data[].symbol.name'

# 呼び出しグラフを構築
codanna mcp find_callers parse_file --json | \
jq -r '.data[]? | "\(.name) - \(.file_path):\(.range.start_line)"'

# スコアでフィルタ
codanna mcp semantic_search_docs query:"config" --json | \
jq '.data[] | select(.score > 0.5)'
```

## Using symbol_id for Unambiguous Queries

すべてのツールは結果に `[symbol_id:123]` を含めます。フォローアップ時はこれらの ID を使用すると正確です。

**Benefits:**
- **曖昧さ排除** - 同名シンボルが複数あっても機能
- **効率的** - ディスアンビギュエーション不要、直接検索
- **ワークフロー最適化** - 結果から ID をコピーして次のコマンドへ貼り付け

**Example workflow:**
```bash
# Step 1: 検索で symbol_id を取得
codanna mcp semantic_search_with_context query:"indexing" limit:1 --json
# Returns: SimpleIndexer [symbol_id:1883]

# Step 2: symbol_id を使って正確に追跡
codanna mcp get_calls symbol_id:1883

# Step 3: 結果の ID で関係を辿る
codanna mcp analyze_impact symbol_id:1926
```

## Tool Workflow

### Recommended Approach

**Tier 1: 高品質コンテキスト（ここから開始）**
- `semantic_search_with_context` - フルコンテキスト、影響分析、関係性付きでシンボルを返す
- `analyze_impact` - 完全な依存グラフを表示（Calls + Uses + Composes）

**Tier 2: 正確な検索（名前が分かる場合）**
- `find_symbol` - 完全一致検索
- `search_symbols` - ファジー検索とフィルタ

**Tier 3: 関係詳細（特定パターンの検証）**
- `get_calls` - 関数呼び出しのみ（括弧）
- `find_callers` - 逆方向の関数呼び出し

### When to Use What

- **全体像が必要？** → `semantic_search_with_context` か `analyze_impact`
- **特定の呼び出しが必要？** → `get_calls` または `find_callers`
- **迷ったら？** → Tier 1 ツールがすべてを表示
- **関係を辿る？** → 前回結果の `symbol_id:ID` を使用

## System Messages

各ツールのレスポンスには、次のアクションを案内する `system_message` が含まれています。これはユーザーには非表示ですが、AI アシスタントがコマンドを連鎖させるのに役立ちます。

```bash
# system_message を抽出
codanna mcp find_callers walk_and_stream --json | jq -r '.system_message'
# Output: Found 18 callers. Run 'analyze_impact' to map the change radius.
```

## See Also

- [CLI Reference](cli-reference.md#codanna-mcp-tool-positional) - コマンドライン使用方法
- [Unix Piping](../advanced/unix-piping.md) - 高度なパイプワークフロー
- [Agent Guidance](../integrations/agent-guidance.md) - system_message の設定
```