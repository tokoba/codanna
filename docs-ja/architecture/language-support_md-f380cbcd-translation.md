Path: architecture\language-support.md

```markdown
# 言語サポート

パーサーのアーキテクチャと対応言語。

## 対応言語

| 言語 | パーサー | ステータス |
|------|----------|------------|
| Rust | tree-sitter-rust | Production |
| Python | tree-sitter-python | Production |
| TypeScript | tree-sitter-typescript | Production |
| Go | tree-sitter-go | Production |
| PHP | tree-sitter-php | Production |
| C | tree-sitter-c | Production |
| C++ | tree-sitter-cpp | Production |

## パーサーテクノロジー

Codanna は AST 解析に tree-sitter を使用しています。これは GitHub のコードナビゲータでも採用されている技術です。

### tree-sitter を採用する理由

- 言語に依存しない
- 高速なインクリメンタルパース
- エラーに寛容
- 実績豊富
- 活発なエコシステム

## 抽出対象

対応言語ごとに抽出される要素:

- 関数とメソッド
- クラス、構造体、トレイト
- 型定義
- インポートとインクルード
- 呼び出し関係
- 型の関係
- ドキュメンテーションコメント

## パフォーマンス

最新のベンチマークについては [パフォーマンスドキュメント](../advanced/performance.md) を参照してください。

## 新しい言語の追加

言語サポート追加の詳細な手順については、リポジトリの Contributing ドキュメントを参照してください。

## 参照

- [仕組み](how-it-works.md) - 全体アーキテクチャ
- [パフォーマンス](../advanced/performance.md) - パーサーベンチマーク
- [コントリビューション](../contributing/) - 開発ガイドライン
```