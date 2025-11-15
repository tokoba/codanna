```markdown
[Documentation](../README.md) / **Architecture**

---

# アーキテクチャ

Rust 製の高性能コードインテリジェンスシステム。コードをインデックスし、関係を追跡し、MCP で提供します。

## 仕組み

1. **高速パース** - Rust、Python、TypeScript、Go、PHP（さらに追加予定）向けに Tree-sitter AST を使用（GitHub Code Navigator と同じ）
2. **実データ抽出** - 関数、トレイト、型の関係、呼び出しグラフを抽出
3. **埋め込み** - ドキュメントコメントからセマンティックベクトルを生成
4. **インデックス** - Tantivy + メモリマップされたシンボルキャッシュで <10ms ルックアップ
5. **提供** - AI アシスタント向け MCP プロトコル、HTTP/HTTPS で約 300ms、stdio は 0.16s

## このセクションに含まれるもの

- **[How It Works](how-it-works.md)** - 詳細なシステムアーキテクチャ
- **[Memory Mapping](memory-mapping.md)** - キャッシュとストレージ設計
- **[Embedding Model](embedding-model.md)** - セマンティック検索の実装
- **[Language Support](language-support.md)** - パーサーシステムと新言語の追加方法

## アーキテクチャのハイライト

**メモリマップドストレージ**: アクセスパターンに応じた 2 種類のキャッシュ  
- `symbol_cache.bin` - FNV-1a ハッシュによるシンボルルックアップ、<10ms 応答  
- `segment_0.vec` - 384 次元ベクトル、OS ページキャッシュが温まった後 <1μs アクセス

**埋め込みのライフサイクル管理**: ファイルを再インデックスすると古い埋め込みを削除し、蓄積を防止。

**ロックフリー並行性**: DashMap でシンボルを並列読み込み、単一ライターロックで書き込みを調整。

**シングルパスインデックス**: AST を 1 回走査するだけでシンボル、関係、埋め込みを抽出。

**言語対応セマンティック検索**: 埋め込みにソース言語を保持し、類似度計算前にフィルタリング可能。同一ドキュメントならフィルタリング有無にかかわらずスコアは同一。

**ホットリロード**: 500ms デバウンス付きファイルウォッチャーで変更ファイルのみ再インデックス。

## パフォーマンス

750 シンボルのテストファイルでのパーサーベンチマーク:

| 言語 | パース速度 | vs. 目標 (10k/s) | ステータス |
|------|-------------|------------------|------------|
| **Rust** | 91,318 symbols/sec | 9.1 倍速 ✓ | 運用 |
| **Python** | 75,047 symbols/sec | 7.5 倍速 ✓ | 運用 |
| **TypeScript** | 82,156 symbols/sec | 8.2 倍速 ✓ | 運用 |
| **PHP** | 68,432 symbols/sec | 6.8 倍速 ✓ | 運用 |
| **Go** | 74,655 symbols/second | 7.5 倍速 ✓ | 運用 |

パフォーマンスベンチマークの実行:
```bash
codanna benchmark all          # すべてのパーサーをテスト
codanna benchmark python       # 特定言語をテスト
```

## 次のステップ

- [User Guide](../user-guide/) で使い方を学ぶ
- [Advanced](../advanced/) 機能を探索
- [Contributing](../contributing/) を読んで機能追加に参加

[Back to Documentation](../README.md)
```