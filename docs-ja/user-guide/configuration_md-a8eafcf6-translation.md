```markdown
# Configuration Guide

Codanna の設定は `.codanna/settings.toml` に保存されています。

## Configuration File Location

```bash
.codanna/
├── plugins/          # プラグインのロックファイル
├── index/            # インデックスの保存場所
├── .project-id       # グローバル設定を管理するための一意なプロジェクト ID（~/.codanna に保存）
└── settings.toml     # メイン設定ファイル
```

## Basic Configuration

```toml
# .codanna/settings.toml

# セマンティック検索モデルの設定
[semantic]
# 埋め込みに使用するモデル
# - AllMiniLML6V2: 英語専用、384 次元（デフォルト）
# - MultilingualE5Small: 94 言語対応、384 次元（多言語推奨）
# - MultilingualE5Base: 94 言語対応、768 次元（より高品質）
# - MultilingualE5Large: 94 言語対応、1024 次元（最高品質）
# - BGESmallZHV15: 中国語特化、512 次元
# - 利用可能なモデルの一覧はドキュメントを参照
model = "AllMiniLML6V2"
```

[Read more about embedding models](../architecture/embedding-model.md)

```toml
# エージェントガイダンスの設定
[guidance]
enabled = true
```
[Learn more about agent guidance](../integrations/agent-guidance.md)

## Language Configuration

### TypeScript

`tsconfig.json` を読み込み、パスエイリアスを解決します:

```toml
[languages.typescript]
enabled = true
config_files = [
    "tsconfig.json",
    "packages/web/tsconfig.json"  # モノレポ用
]
```

TypeScript コードで `@app/utils` をインポートすると、Codanna は `tsconfig.json` のパスマッピングを利用して実際のファイル位置（`src/app/utils`）に解決します。これはモノレポを含むモジュール間で機能します。

### Other Languages

近日対応予定: Python（`pyproject.toml`）、Go（`go.mod`）、その他プロジェクト固有のインポート解決を必要とする言語。

## Semantic Search Models

### Available Models

| Model | Description | Use Case |
|-------|-------------|----------|
| `AllMiniLML6V2` | 高速・英語最適化（デフォルト） | 英語コードベース |
| `MultilingualE5Small` | 非英語に強い | 多言語チーム |
| `ParaphraseMultilingualMiniLML12V2` | 最も高品質な多言語対応 | 国際プロジェクト |

### Switching Models

```toml
[semantic]
model = "MultilingualE5Small"
```

**Note:** モデルを変更した場合は再インデックスが必要です:
```bash
codanna index . --force --progress
```

## Agent Guidance Templates

AI アシスタントのガイダンス方法を設定します:

```toml
[guidance]
enabled = true

[guidance.templates.find_callers]
no_results = "No callers found. Might be an entry point or dynamic dispatch."
single_result = "Found 1 caller. Use 'find_symbol' to inspect usage."
multiple_results = "Found {result_count} callers. Try 'analyze_impact' for the full graph."

[guidance.templates.analyze_impact]
no_results = "No impact detected. Likely isolated."
single_result = "Minimal impact radius."
multiple_results = "Impact touches {result_count} symbols. Focus critical paths."

[[guidance.templates.analyze_impact.custom]]
min = 20
template = "Significant impact with {result_count} symbols. Break the change into smaller parts."
```

## Indexing Configuration

```toml
[indexing]
threads = 8  # 並列インデックス処理に使用するスレッド数
max_file_size_mb = 10  # このサイズを超えるファイルはスキップ
```

## Multi-Directory Indexing

永続的な設定で複数ディレクトリを同時にインデックスできます。

### Configuration

```toml
[indexing]
indexed_paths = [
    "/absolute/path/to/project1",
    "/absolute/path/to/project2",
    "/absolute/path/to/project3"
]
```

### Managing Indexed Directories

```bash
codanna add-dir /path/to/project
codanna list-dirs
codanna remove-dir /path/to/project
```

**Automatic Sync:**
- コマンドは settings.toml（唯一の情報源）を更新
- 次回コマンド実行時にインデックスを自動同期
- 追加されたパス → インデックス対象
- 削除されたパス → シンボル・埋め込み・メタデータをクリーンアップ

### Use Cases

**マルチプロジェクト ワークスペース** - 複数の関連プロジェクトをまとめてインデックスし、プロジェクト間のシンボル解決を実現

**モノレポ対応** - 各コンポーネントを個別にインデックスしつつクロスリファレンスを維持

**選択的インデックス** - 大規模コードベースの特定ディレクトリのみインデックス

**動的ワークフロー** - プロジェクト構成の変化に合わせてフォルダを追加・削除

## Ignore Patterns

Codanna は `.gitignore` を尊重し、独自に `.codannaignore` も追加します:

```bash
# .codannaignore
.codanna/       # 自身のデータは除外
target/         # ビルド成果物をスキップ
node_modules/   # 依存パッケージをスキップ
*_test.rs       # 必要に応じてテストをスキップ
```

## HTTP/HTTPS Server Configuration

サーバーモードの設定例:

```toml
[server]
bind = "127.0.0.1:8080"
watch_interval = 5  # インデックスチェック間隔（秒）
```

## Performance Tuning

```toml
[performance]
cache_size_mb = 100  # メモリキャッシュサイズ
vector_cache_size = 10000  # メモリに保持するベクトル数
```

## Command-Line Overrides

ほとんどの設定はコマンドラインで上書き可能です:

```bash
# 設定ファイルを指定して上書き
codanna --config /path/to/custom.toml index .

# スレッド数を上書き
codanna index . --threads 16

# 特定の設定を強制
codanna serve --watch --watch-interval 10
```

## Viewing Configuration

```bash
# 有効な設定を表示
codanna config

# カスタムファイルで設定を表示
codanna --config custom.toml config
```

## Configuration Precedence

1. コマンドラインフラグ（最優先）
2. カスタム設定ファイル（`--config` 指定）
3. プロジェクト `.codanna/settings.toml`
4. 組み込みデフォルト（最低優先）

## Project-Specific Path Resolution

### How It Works

1. Codanna はプロジェクト設定ファイル（`tsconfig.json` など）を読み込み
2. パスエイリアス、baseUrl、その他の解決ルールを抽出
3. それらを `.codanna/index/resolvers/` に保存
4. インデックス時にこれらのルールを使用しインポートを正確に解決

### Benefits

- 正確なインポート解決
- モノレポ内のモジュール間ナビゲーション
- パスエイリアス（`@app/*`, `~/utils/*`）のサポート
- 手動設定不要

## Troubleshooting

### Index Not Updating

ウォッチ間隔を確認:
```toml
[server]
watch_interval = 5  # チェック頻度を上げる
```

### Semantic Search Not Working

1. ドキュメントコメントが存在するか確認
2. モデルが言語に合っているか確認
3. 設定変更後に再インデックス

### Path Resolution Issues

設定ファイルがリストに含まれているか確認:
```toml
[languages.typescript]
config_files = ["tsconfig.json"]
```

## See Also

- [First Index](../getting-started/first-index.md) - 初めてのインデックス作成
- [Agent Guidance](../integrations/agent-guidance.md) - AI アシスタント動作の設定
- [CLI Reference](cli-reference.md) - コマンドラインオプション
```