[Documentation](../README.md) / **ユーザーガイド**

---

# ユーザーガイド

Codanna を効果的に使用するための完全なドキュメント。

## ドキュメント

- **[CLI リファレンス](cli-reference.md)** - すべてのコマンドとフラグ
- **[MCP ツール](mcp-tools.md)** - MCP サーバーで利用可能なツール
- **[設定](configuration.md)** - `.codanna/settings.toml` に格納
- **[検索ガイド](search-guide.md)** - セマンティック検索のベストプラクティス

## コアコマンド

| コマンド | 説明 | 例 |
|---------|-------------|---------|
| `codanna init` | 既定の設定で .codanna ディレクトリをセットアップ | `codanna init --force` |
| `codanna index <PATH>` | コードベースから検索可能なインデックスを構築 | `codanna index src --progress` |
| `codanna config` | 有効な設定を表示 | `codanna config` |
| `codanna serve` | AI アシスタント用の MCP サーバーを起動 | `codanna serve --watch` |

## MCP ツール プレビュー

### シンプルツール（位置引数）
| ツール | 説明 | 例 |
|------|-------------|---------|
| `find_symbol` | 正確な名前でシンボルを検索 | `codanna mcp find_symbol main` |
| `get_calls` | 指定した関数が呼び出す関数を表示 | `codanna mcp get_calls process_file`<br>`codanna mcp get_calls symbol_id:1883` |
| `find_callers` | 指定した関数を呼び出す関数を表示 | `codanna mcp find_callers init`<br>`codanna mcp find_callers symbol_id:1883` |
| `analyze_impact` | シンボル変更の影響範囲を分析 | `codanna mcp analyze_impact Parser`<br>`codanna mcp analyze_impact symbol_id:1883` |

### 複合ツール（キー:値 引数）
| ツール | 説明 | 例 |
|------|-------------|---------|
| `search_symbols` | 全文ファジーマッチでシンボルを検索 | `codanna mcp search_symbols query:parse kind:function limit:10` |
| `semantic_search_docs` | 自然言語クエリで検索 | `codanna mcp semantic_search_docs query:"error handling" limit:5` |

**ヒント:** すべてのツールは結果に `[symbol_id:123]` を返します。シンボル名の代わりに `symbol_id:ID` を使用すると後続のクエリが明確になります。

## 次のステップ

- AI アシスタントとの [インテグレーション](../integrations/) を設定
- [高度な](../advanced/) Unix ネイティブ機能を探求
- 内部構造については [アーキテクチャ](../architecture/) を学習

[ドキュメントへ戻る](../README.md)