```markdown
# Embedding Model

Codanna がコード検索のためにセマンティック埋め込みを生成・利用する方法。

## Supported Models

| Model | Dimensions | Languages | Use Case |
|-------|------------|-----------|----------|
| `AllMiniLML6V2` | 384 | English | 既定、速い、英語コードベース |
| `MultilingualE5Small` | 384 | 94 | 多言語、同等の性能 |
| `MultilingualE5Base` | 768 | 94 | より高品質、やや遅い |
| `MultilingualE5Large` | 1024 | 94 | 最高品質、最も遅い |

## Model Selection

`.codanna/settings.toml` で設定:

```toml
[semantic]
model = "AllMiniLML6V2"  # 既定
# model = "MultilingualE5Small"  # 多言語チーム向け
```

**注意:** モデルを変更した場合は再インデックスが必要です:
```bash
codanna index . --force --progress
```

## Embedding Generation

### Input: Documentation Comments

```rust
/// Parse configuration from a TOML file and validate required fields
/// This handles missing files gracefully and provides helpful error messages
fn load_config(path: &Path) -> Result<Config, Error>
```

### Process

1. **抽出**: ドキュメントコメントのテキスト
2. **トークン化**: トークンに分割
3. **埋め込み**: fastembed モデルでベクトル化
4. **正規化**: コサイン類似度向けに L2 正規化
5. **保存**: メモリマップドベクターキャッシュへ格納

### Output: Dense Vector

```
[0.123, -0.456, 0.789, ..., 0.321]  // 384/768/1024 個の float
```

## Semantic Understanding

埋め込みが捉えるもの:
- **概念的意味** - 単なるキーワードではない
- **コンテキスト** - 関連用語が近くにクラスタリング
- **意図** - 「error handling」が「graceful failure recovery」と一致

### Example

クエリ: "authentication logic"

マッチ:
- "user authentication and session management"
- "verify credentials and create tokens"
- "login flow with password hashing"

マッチしない:
- "configuration parser" (異なる概念)
- "file system operations" (無関係)

## Similarity Computation

ベクトル比較にはコサイン類似度を使用:

```
similarity = dot(v1, v2) / (||v1|| × ||v2||)
```

スコア範囲は 0 〜 1:
- **0.7+** - 非常に類似
- **0.5-0.7** - 関連あり
- **0.3-0.5** - やや関連
- **<0.3** - 異なる概念

## Language-Aware Embeddings

各埋め込みはソース言語を保持:

```rust
struct EmbeddedSymbol {
    symbol_id: SymbolId,
    vector: Vec<f32>,
    language: LanguageId,  // rust, python, typescript など
}
```

### Language Filtering

類似度計算 **前** にフィルタリング:

```bash
# Rust コードのみ検索
codanna mcp semantic_search_docs query:"error handling" lang:rust
```

**性能向上**: 混在コードベースで検索対象を最大 75% 削減。

**精度**: 異なる言語でも同一ドキュメントの場合スコアは同一。

## IVFFlat Index

高速検索のため Inverted File + Flat ベクトルを使用:

### K-means Clustering

1. **クラスタリング**: 類似ベクトルをまとめる
2. **セントロイド**: 各クラスタを代表
3. **検索**: 近傍クラスタを優先的に探索

### Search Algorithm

```
1. クエリベクトル → 最も近いセントロイドを探索
2. そのクラスタ内のベクトルを検索
3. 必要に応じ近隣クラスタも検索
4. 上位 k 件を返す
```

**速度向上**: 比較回数 O(N) → O(√N)。

## Model Characteristics

### AllMiniLML6V2
- **サイズ**: 約 25MB
- **速度**: 推論高速
- **品質**: 英語で良好
- **用途**: 既定選択

### MultilingualE5Small
- **サイズ**: 約 118MB
- **速度**: AllMiniLM と同程度
- **品質**: 94 言語対応
- **用途**: 多言語チーム

### MultilingualE5Base
- **サイズ**: 約 278MB
- **速度**: 推論やや遅い
- **品質**: より高精度
- **用途**: 品質重視

### MultilingualE5Large
- **サイズ**: 約 560MB
- **速度**: 最も遅い
- **品質**: 最高精度
- **用途**: 最大品質が必要な場合

## Performance Characteristics

### 初回利用
- モデルダウンロード: 1 回のみ (約 25–560MB)
- 保存先: `~/.cache/fastembed/`
- 2 回目以降: モデル即時ロード

### 埋め込み生成
- シンボルあたり: 約 10ms
- 100 件バッチ: 約 100ms
- 並列化: CPU コア数に比例

### 検索
- IVFFlat 使用: 100k ベクトルで <10ms
- クラスタリングなし: 約 1s

## Optimization

### Batch Processing
インデックス時にバッチ生成:
- GPU/CPU 使用効率向上
- モデル初期化コストを均等化
- スループット向上

### Caching
- 埋め込みはメモリマップドファイルに永続化
- コード変更が無い限り再生成不要
- シンボル単位で変更検知

### Incremental Updates
変更シンボルのみ再埋め込み:
```rust
if symbol.doc_comment != old_symbol.doc_comment {
    regenerate_embedding(symbol);
}
```

## Troubleshooting

### 検索結果が悪い
1. ドキュメント品質を確認
2. 別モデルを試す (多言語など)
3. 閾値パラメータ調整
4. 言語フィルタリングを使用

### 埋め込み生成が遅い
1. 初回はモデルダウンロード (一度きり)
2. 大規模コードベースは初期に時間がかかる
3. 増分更新は高速
4. `--threads` で並列化

### モデルが見つからない
- 初回はインターネット接続を確認
- `~/.cache/fastembed/` の権限確認
- `rm -rf ~/.cache/fastembed/` で再ダウンロード

## Storage Requirements

シンボル 100,000 件の場合:

**AllMiniLML6V2 (384 次元):**
- 100k × 384 × 4 バイト = 153.6 MB

**MultilingualE5Base (768 次元):**
- 100k × 768 × 4 バイト = 307.2 MB

**MultilingualE5Large (1024 次元):**
- 100k × 1024 × 4 バイト = 409.6 MB

## See Also

- [How It Works](how-it-works.md) - システム概要
- [Memory Mapping](memory-mapping.md) - ベクターストレージ詳細
- [Search Guide](../user-guide/search-guide.md) - 効率的なクエリの書き方
- [Configuration](../user-guide/configuration.md) - モデル選択
```