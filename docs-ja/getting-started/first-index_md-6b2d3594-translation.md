```markdown
# 最初のインデックス

初めての Codanna インデックスの作成と使用方法を学びましょう。

## Codanna の初期化

```bash
codanna init
```

これにより `.codanna/` ディレクトリが作成され、次のものが含まれます:
- `settings.toml` - 設定ファイル
- `index/` - コードインデックスの保存先

## .codannaignore を理解する

Codanna は `.gitignore` を尊重し、独自の `.codannaignore` を追加します:

```bash
# Created automatically by codanna init
.codanna/       # Don't index own data
target/         # Skip build artifacts
node_modules/   # Skip dependencies
*_test.rs       # Optionally skip tests
```

## コードをインデックスする

### ドライラン（プレビュー）

実際にインデックスせず、どのファイルが対象になるかを確認します:

```bash
codanna index src --dry-run
```

### インデックスを構築する

```bash
# Index entire project (respects .gitignore and .codannaignore)
codanna index . --progress

# Index specific directory
codanna index src --progress

# Index a single file
codanna index src/main.rs

# Force re-index
codanna index src --force
```

## インデックスを確認する

インデックスが正しく作成されたか確認します:

```bash
# Get index statistics
codanna mcp get_index_info

# Search for a known function
codanna mcp find_symbol main

# Try semantic search
codanna mcp semantic_search_docs query:"error handling" limit:5
```

## インデックスの仕組み

1. **高速解析** - Rust、Python、TypeScript、Go、PHP に対して Tree-sitter の AST 解析（GitHub Code Navigator と同じ）
2. **実体を抽出** - 関数、トレイト、型の関係、呼び出しグラフ
3. **埋め込み** - ドキュメントコメントから生成されたセマンティックベクトル
4. **インデックス** - Tantivy + メモリマップドのシンボルキャッシュにより <10ms の検索

## より良いインデックス作成のためのヒント

### ドキュメントコメント

セマンティック検索はドキュメントコメントを理解して機能します:

```rust
/// Parse configuration from a TOML file and validate required fields
/// This handles missing files gracefully and provides helpful error messages
fn load_config(path: &Path) -> Result<Config, Error> {
    // implementation...
}
```

適切なコメントがあれば、セマンティック検索は次のようなクエリでこの関数を見つけられます:
- 「configuration validation」（設定の検証）
- 「handle missing config files」（設定ファイルの欠落を処理）
- 「TOML parsing with error handling」（エラー処理付きの TOML 解析）

### 複数言語のコードベース

Python バックエンドと TypeScript フロントエンドに類似の認証機能があるなど、複数言語で同一のドキュメントが存在する場合は、`lang:python` や `lang:typescript` のように言語フィルタを用いて言語ごとの結果を取得してください。

## トラブルシューティング

### インデックス作成が遅い

- `--threads` で並列度を調整
- `.codannaignore` で大きなディレクトリを除外
- テストファイルが不要ならスキップ

### 検索結果が出ない

- ファイルにドキュメントコメントがあるか確認
- 対応言語（Rust, Python, TypeScript, Go, PHP, C, C++）であるか確認
- `.gitignore` や `.codannaignore` により除外されていないか確認

## 次のステップ

- [MCP Tools](../user-guide/mcp-tools.md) でインデックスを検索
- AI アシスタントとの [Integrations](../integrations/) を設定
- プロジェクト用に [settings.toml](../user-guide/configuration.md) を構成
```