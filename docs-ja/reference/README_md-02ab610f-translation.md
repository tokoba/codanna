```markdown
[Documentation](../README.md) / **Reference**

---

# リファレンス

Codanna のクイックリファレンスドキュメント。

## このセクションについて

- **[MCP Protocol](mcp-protocol.md)** - MCP ツール完全仕様
- **[Exit Codes](exit-codes.md)** - CLI 終了コードリファレンス

## クイックリファレンス

### 共通フラグ

- `--config`, `-c`: カスタム settings.toml ファイルへのパス
- `--force`, `-f`: 強制実行（上書き、再インデックスなど）
- `--progress`, `-p`: 操作中の進行状況を表示
- `--threads`, `-t`: 使用するスレッド数
- `--dry-run`: 実行せずに何が起こるかを表示
- `--json`: パイプ用の構造化出力（見つからない場合は終了コード 3）

### MCP ツールパラメータ

| ツール | パラメータ |
|------|------------|
| `find_symbol` | `name`（必須） |
| `search_symbols` | `query`, `limit`, `kind`, `module` |
| `semantic_search_docs` | `query`, `limit`, `threshold`, `lang` |
| `semantic_search_with_context` | `query`, `limit`, `threshold`, `lang` |
| `get_calls` | `function_name` または `symbol_id`（いずれか必須） |
| `find_callers` | `function_name` または `symbol_id`（いずれか必須） |
| `analyze_impact` | `symbol_name` または `symbol_id`（いずれか必須）, `max_depth` |
| `get_index_info` | なし |

**symbol_id の使用:**
- すべてのツールは一意の参照として `[symbol_id:123]` を返します
- 正確なクエリには名前の代わりに `symbol_id:ID` を使用してください
- 例: `codanna mcp get_calls symbol_id:1883`

### 言語フィルタリング

セマンティック検索ツールは、混在プロジェクトでノイズを減らすために言語フィルタリングをサポートします:

```bash
# Search only in Rust code
codanna mcp semantic_search_docs query:"authentication" lang:rust limit:5

# Search only in TypeScript code
codanna mcp semantic_search_with_context query:"parse config" lang:typescript limit:3
```

言語フィルタリングにより、複数言語で類似ドキュメントが存在する場合の重複結果を排除し、結果セットを最大 75% 削減しながら同一の類似度スコアを維持します。

## 現在の制限事項

- Rust、Python、TypeScript、Go、PHP、C、C++ をサポート（対応言語は今後追加予定）
- セマンティック検索は英語のドキュメント／コメントが必要
- Windows サポートは実験的

## 必要条件

- Rust 1.75+（開発用）
- モデル保存に約 150 MB（初回使用時にダウンロード）
- インデックス保存に数 MB（コードベースのサイズによる）

## 次のステップ

- [ユーザーガイド](../user-guide/) を参照
- [高度な機能](../advanced/) を探索
- 技術詳細は [アーキテクチャ](../architecture/) を参照

[ドキュメントに戻る](../README.md)
```