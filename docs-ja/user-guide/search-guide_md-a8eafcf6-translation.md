```markdown
# 検索ガイド

Codanna の検索機能で効果的なクエリを書き、最良の結果を得る方法。

## 検索タイプ

### 完全一致: `find_symbol`
正確な名前がわかっている場合:
```bash
codanna mcp find_symbol main
codanna mcp find_symbol SimpleIndexer
```

### あいまい検索: `search_symbols`
部分一致やタイプミスに対応:
```bash
codanna mcp search_symbols query:parse
codanna mcp search_symbols query:indx  # "index" 関数を見つける
```

### セマンティック検索: `semantic_search_docs`
自然言語クエリ向け:
```bash
codanna mcp semantic_search_docs query:"where do we handle errors"
codanna mcp semantic_search_docs query:"authentication logic"
```

### コンテキスト検索: `semantic_search_with_context`
関係性を理解するために:
```bash
codanna mcp semantic_search_with_context query:"file processing pipeline"
```

## より良いドキュメンテーションコメントを書く

セマンティック検索はドキュメンテーションコメントを理解して機能します:

### 良いドキュメント
```rust
/// Parse configuration from a TOML file and validate required fields
/// This handles missing files gracefully and provides helpful error messages
fn load_config(path: &Path) -> Result<Config, Error> {
    // implementation...
}
```

良いコメントがあると、セマンティック検索で次のようなプロンプトでもこの関数を見つけられます:
- "configuration validation"
- "handle missing config files"
- "TOML parsing with error handling"

### 不十分なドキュメント
```rust
// Load config
fn load_config(path: &Path) -> Result<Config, Error> {
    // implementation...
}
```

これではセマンティック検索で効果的に見つけられません。

## クエリ作成のヒント

### 具体的に書く
- **悪い例:** "error"
- **良い例:** "error handling in file operations"

### ドメイン用語を使う
- **悪い例:** "make things fast"
- **良い例:** "performance optimization for indexing"

### コンテキストを含める
- **悪い例:** "parse"
- **良い例:** "parse TypeScript import statements"

## 言語フィルタリング

複数言語のコードベースでは、言語フィルタを使用:

```bash
# Rust コードのみ検索
codanna mcp semantic_search_docs query:"memory management" lang:rust

# TypeScript のみ検索
codanna mcp semantic_search_docs query:"React components" lang:typescript
```

対応言語: rust, python, typescript, go, php, c, cpp

## スコアの理解

類似度スコアは 0〜1:
- **0.7 以上** - 非常に関連性が高い
- **0.5〜0.7** - 関連性が高い
- **0.3〜0.5** - ある程度関連
- **<0.3** - おそらく関連性が低い

しきい値を設定してフィルタ:
```bash
codanna mcp semantic_search_docs query:"authentication" threshold:0.5
```

## 検索ワークフロー

### 実装詳細を見つける
1. セマンティック検索を広めに行う  
2. 結果から symbol_id を取得  
3. ID を使って関係性を追跡  

```bash
# 認証関連を検索
codanna mcp semantic_search_docs query:"user authentication" limit:5
# 返却: authenticate_user [symbol_id:456]

# 明確な検索に symbol_id を使用
codanna mcp find_callers symbol_id:456

# あいまいさがなければ名前でも
codanna mcp find_symbol authenticate_user
```

### コードフローを理解する
1. エントリポイントを見つける  
2. symbol_id を使って呼び出しを追跡  
3. 影響を分析  

```bash
# メイン処理関数を見つける
codanna mcp semantic_search_with_context query:"main processing pipeline"
# 返却: process_file [symbol_id:789]

# その呼び出し元を追跡 (精度のため ID 使用)
codanna mcp get_calls symbol_id:789

# 影響を理解
codanna mcp analyze_impact symbol_id:789
```

### デバッグ
1. エラー関連コードを検索  
2. symbol_id を用いて呼び出し元を特定  
3. ソースへ遡る  

```bash
# エラー処理を検索
codanna mcp semantic_search_docs query:"error recovery retry logic"
# 返却: handle_error [symbol_id:234]

# エラーハンドラの呼び出し元を検索 (前結果の ID 使用)
codanna mcp find_callers symbol_id:234

# ソースへ遡る
codanna mcp analyze_impact symbol_id:234
```

## 高度なテクニック

### ツールの組み合わせ
```bash
# すべてのパーサとその呼び出し元を見つける
codanna mcp search_symbols query:parse kind:function --json | \
jq -r '.data[].name' | \
xargs -I {} codanna mcp find_callers {} --json | \
jq -r '.data[].name' | sort -u
```

### コンテキストの構築
```bash
# コンセプトの完全なコンテキストを取得
codanna mcp semantic_search_with_context query:"dependency injection" limit:1 --json | \
jq '.data[0]'
```

これにより次が返ります:
- シンボル自体 `[symbol_id:123]`
- それを呼び出すもの (各 symbol_id 付き)
- それが呼び出すもの (各 symbol_id 付き)
- 完全な影響分析

返却された symbol_id を使って正確なフォローアップクエリを行います。

## よくある問題

### 結果がない

**問題:** セマンティック検索で何も返らない  
**解決策:**  
- ドキュメントがあるか確認  
- 用語を広めにする  
- 技術用語を減らす  

### 結果が多すぎる

**問題:** 検索結果が多すぎる  
**解決策:**  
- 言語フィルタを追加: `lang:rust`  
- しきい値を上げる: `threshold:0.6`  
- limit を減らす: `limit:3`  
- クエリを具体的にする  

### 間違った言語の結果

**問題:** TypeScript が欲しいのに Python が出る  
**解決策:** 混在コードベースでは必ず言語フィルタを使用:
```bash
codanna mcp semantic_search_docs query:"components" lang:typescript
```

## ベストプラクティス

1. **まず semantic_search_with_context** - 最も包括的な情報を提供  
2. **フォローアップに symbol_id** - あいまいさを排除しクエリ数を節約  
3. **言語フィルタを使う** - 混在コードベースでノイズを最大 75% 削減  
4. **良いドキュメントを書く** - 良いドキュメント = 良い検索結果  
5. **検索を連鎖させる** - 一つの検索で得た symbol_id を次に活用  
6. **JSON 出力を使う** - 強力なパイプ処理とフィルタリングが可能  

**symbol_id を使ったワークフロー例:**
```bash
# Step 1: セマンティック検索で見つける
codanna mcp semantic_search_with_context query:"config parser" limit:1 --json
# 抽出: parse_config [symbol_id:567]

# Step 2: 直接フォローアップ (あいまいさ無し)
codanna mcp get_calls symbol_id:567
codanna mcp find_callers symbol_id:567
codanna mcp analyze_impact symbol_id:567
```

## パフォーマンスヒント

- 起動直後の最初の検索はやや遅い場合あり (キャッシュウォーミング)  
- その後の検索は通常 <10ms  
- 複雑なフィルタリングには `--json` と `jq` を使用し、検索回数を減らす  

## 関連リンク

- [MCP ツールリファレンス](mcp-tools.md) - 完全なツールドキュメント  
- [Unix パイプ](../advanced/unix-piping.md) - 高度な検索ワークフロー  
- [設定](configuration.md) - セマンティックモデルの設定  
```