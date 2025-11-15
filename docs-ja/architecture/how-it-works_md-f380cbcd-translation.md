```markdown
# 仕組み

Codanna による高速かつ高精度なコードインテリジェンスのアーキテクチャ。

## システム概要

1. **高速パース** - Rust、Python、TypeScript、Go、PHP（さらに拡大予定）向けに Tree-sitter の AST 解析（GitHub Code Navigator と同様）
2. **実体抽出** - 関数、トレイト、型の関係、呼び出しグラフの抽出
3. **埋め込み生成** - ドキュメントコメントからセマンティックベクトルを生成
4. **インデックス** - Tantivy + メモリマップ型シンボルキャッシュで <10ms ルックアップ
5. **提供** - AI アシスタント向け MCP プロトコル、HTTP/HTTPS ~300ms 応答、標準入出力組み込み（0.16s）

## 技術スタック

- **tree-sitter**: 多言語パーシング
- **tantivy**: ベクトル機能統合型全文検索
- **fastembed**: 高速埋め込み生成
- **linfa**: IVFFlat ベクトル索引用 K-means クラスタリング
- **memmap2**: ベクトルデータのメモリマップストレージ
- **bincode**: 高効率シリアライズ
- **rkyv**: ゼロコピーシリアライズで高速化
- **DashMap**: ロックフリーの並列データ構造
- **tokio**: 非同期ランタイム
- **thiserror**: 構造化エラー処理

## データフロー

### インデックス作成パイプライン

```
Source Files
    ↓
Tree-sitter Parser
    ↓
Symbol Extraction
    ↓
Relationship Analysis
    ↓
Doc Comment Embedding
    ↓
Tantivy Index + Vector Store
```

### クエリパイプライン

```
User Query
    ↓
MCP Protocol
    ↓
Query Router
    ├→ Exact Match (find_symbol)
    ├→ Full-Text Search (search_symbols)
    ├→ Semantic Search (semantic_search_docs)
    └→ Relationship Queries (get_calls, find_callers)
    ↓
Index Lookup
    ↓
Result Formatting
    ↓
Response (JSON/Text)
```

## コアコンポーネント

### パーサーシステム

- 言語非依存のパーサートレイト
- Tree-sitter ベースの実装
- AST からのシンボル抽出
- 関係トラッキング（呼び出し、使用、実装）
- 解決コンテキスト管理

### インデックスシステム

**テキストインデックス (Tantivy):**
- 全文検索機能
- シンボルメタデータ保存
- ファジーマッチ対応

**ベクトルインデックス (カスタム):**
- メモリマップ型ベクトルストレージ
- 高速ルックアップのための IVFFlat クラスタリング
- 埋め込み次元数を設定可能 (384/768/1024)
- K-means による構成

### MCP サーバー

- 標準入出力トランスポート（デフォルト）
- HTTP/HTTPS トランスポート（オプション）
- ファイル監視によるホットリロード
- OAuth 認証（HTTP）
- TLS 暗号化（HTTPS）

## パフォーマンスアーキテクチャ

### シンボルキャッシュ
- FNV-1a ハッシュによるルックアップ
- メモリマップで瞬時ロード
- <10ms 応答時間
- シンボルあたり約 100 バイト

### ベクトルキャッシュ
- モデルに応じた次元 (384/768/1024) 設定
- OS ページキャッシュウォームアップ後 <1μs アクセス
- スケーラビリティ対応のセグメント化ストレージ

### 並行モデル
- DashMap によるロックフリー読み取り
- 単一ライター協調
- ワークスチーリングによる並列インデックス作成
- スレッドローカルのパーサープール

## ストレージレイアウト

```
.codanna/
├── settings.toml           # 設定
├── index/
│   ├── tantivy/           # 全文検索インデックス
│   ├── vectors/           # メモリマップ型ベクトルストレージ
│   │   ├── segment_0.vec  # ベクトルデータ
│   │   └── metadata.bin   # ベクトルメタデータ
│   ├── resolvers/         # パス解決ルール
│   └── symbol_cache.bin   # FNV-1a ハッシュ済みシンボル
└── plugins/
    └── lockfile.json      # プラグインインストール管理
```

## 埋め込みライフサイクル

1. **生成**: ドキュメントコメント → fastembed → ベクトル（モデルに応じ 384/768/1024 次元）
2. **保存**: ベクトルをメモリマップファイルへ格納
3. **クラスタリング**: IVFFlat 構成のため K-means 実行
4. **クリーンアップ**: 再インデックス時に古い埋め込みを削除

## 言語認識検索

埋め込みはソース言語を追跡し、類似度計算前にフィルタリングを可能にします。スコアの再配分は行わず、同一のドキュメントはフィルタリング有無にかかわらず同一スコアを保持します。

## ホットリロード

500ms デバウンス付きのファイルウォッチャーが変更済みファイルのみを再インデックスします。検出方法:
- ファイル更新タイムスタンプ
- コンテンツハッシュ
- シンボルレベルの変更検出

## 参考

- [Memory Mapping](memory-mapping.md) - キャッシュとストレージの詳細
- [Embedding Model](embedding-model.md) - セマンティック検索の内部
- [Language Support](language-support.md) - パーサーアーキテクチャ
```