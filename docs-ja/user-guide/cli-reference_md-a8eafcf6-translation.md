Path: user-guide\cli-reference.md

```markdown
# CLI リファレンス

Codanna のすべてのコマンドとオプションをまとめた完全な一覧です。

## グローバルオプション

すべてのコマンドで利用可能:
- `-c, --config <CONFIG>` - カスタム settings.toml ファイルへのパス
- `--info` - 詳細な読み込み情報を表示
- `-h, --help` - ヘルプを表示
- `-V, --version` - バージョンを表示

## トップレベルコマンド

| Command | 説明 |
|---------|-------------|
| `codanna init` | 既定の設定で .codanna ディレクトリをセットアップ |
| `codanna index` | コードベースから検索用インデックスを構築 |
| `codanna add-dir` | インデックスに追加するフォルダを登録 |
| `codanna remove-dir` | インデックス対象フォルダを解除 |
| `codanna list-dirs` | インデックス対象フォルダを一覧表示 |
| `codanna retrieve` | シンボル、リレーション、依存関係を検索 |
| `codanna serve` | MCP サーバーを起動 |
| `codanna config` | 有効な設定を表示 |
| `codanna mcp-test` | MCP 接続をテスト |
| `codanna mcp` | MCP ツールを直接実行 |
| `codanna benchmark` | パーサの性能をベンチマーク |
| `codanna parse` | AST ノードを JSONL 形式で出力 |
| `codanna plugin` | Claude Code プラグインを管理 |
| `codanna profile` | ワークスペースのプロファイルとプロバイダーを管理 |

## コマンド詳細

`codanna init`  
既定の設定で .codanna ディレクトリをセットアップ

**オプション:**
- `-f, --force` - 既存設定を強制的に上書き

`codanna index [PATHS...]`  
コードベースから検索用インデックスを構築

**引数:**
- `[PATHS...]` - インデックス対象のファイルまたはディレクトリのパス（複数可）
- 引数を省略すると設定の `indexed_paths` を使用（`add-dir` で設定）

**オプション:**
- `-t, --threads <THREADS>` - 使用スレッド数（設定を上書き）
- `-f, --force` - インデックスが存在していても再構築
- `-p, --progress` - インデックス作成中の進捗を表示
- `--dry-run` - 実際にはインデックスせず対象を表示
- `--max-files <MAX_FILES>` - インデックスする最大ファイル数

**例:**
```bash
# 1 つのディレクトリをインデックス
codanna index src --progress

# 複数ディレクトリを一度にインデックス
codanna index src lib tests --progress

# 設定済みの indexed_paths を使用
codanna index --progress
```

**動作:**
- 単一コマンドで複数パスのインデックスに対応
- 引数なしの場合、設定ファイルの `indexed_paths` フォルダを使用
- キャッシュを再利用し、変更がなければ `Index already up to date (no changes detected).` を表示
- 設定経由で削除されたフォルダのシンボルを自動クリーンアップ
- CLI からのパス追加は冪等で、親ディレクトリがすでに追跡されている場合 `Skipping <path> (already covered by <parent>)` と表示
- `--force` 実行時は、ネストしたサブディレクトリ指定でもまず全ルートを再構築
- 単一ファイル指定はアドホックでインデックスされ、`Skipping <file> (indexed file is tracked ad-hoc and not stored in settings)` を表示（`indexed_paths` には追加しない）
- 旧来の単一パス操作とも互換

`codanna add-dir <PATH>`  
settings.toml の indexed paths にフォルダを追加

**引数:**
- `<PATH>` - フォルダのパス（絶対パスに正規化）

**例:**
```bash
codanna add-dir /path/to/project
codanna add-dir src
```

**動作:**
- settings.toml を更新（信頼できる情報源）
- 重複エントリを防止
- 次回コマンド実行時に自動でインデックス

`codanna remove-dir <PATH>`  
settings.toml からフォルダを削除

**引数:**
- `<PATH>` - 設定に登録済みのフォルダパス

**例:**
```bash
codanna remove-dir /path/to/old-project
codanna remove-dir tests
```

**動作:**
- settings.toml を更新（信頼できる情報源）
- 次回コマンド実行時にシンボル・埋め込み・メタデータを自動削除

`codanna list-dirs`  
settings.toml に設定されたインデックス対象ディレクトリを表示

**例:**
```bash
codanna list-dirs
```

## 自動同期メカニズム

すべてのコマンドは settings.toml（信頼できる情報源）とインデックスメタデータを比較:
- 設定に追加された新しいパス → 自動でインデックス
- 設定から削除されたパス → シンボル・埋め込み・メタデータをクリーン

**例:**
```bash
codanna add-dir examples/typescript
codanna retrieve symbol Button
# ✓ Added 1 new directories (5 files, 127 symbols)

codanna remove-dir examples/typescript
codanna retrieve symbol Button
# ✓ Removed 1 directories from index
```

settings.toml を手動編集しても、次回コマンド実行時に変更が検出されます。

`codanna retrieve <SUBCOMMAND>`  
インデックスされたシンボル、リレーション、依存関係を検索

**サブコマンド:**
| Subcommand | 説明 |
|------------|-------------|
| `retrieve symbol` | 名前または `symbol_id:ID` でシンボルを検索 |
| `retrieve calls` | 関数が呼び出す関数を表示（`<name>` または `symbol_id:ID` を受け付ける） |
| `retrieve callers` | 関数を呼び出す関数を表示（`<name>` または `symbol_id:ID` を受け付ける） |
| `retrieve implementations` | 型が実装しているトレイトを表示 |
| `retrieve search` | フリーテキストでシンボルを検索 |
| `retrieve describe` | シンボルの情報を表示（`<name>` または `symbol_id:ID` を受け付ける） |

**すべての retrieve サブコマンド共通:**
- `--json` - JSON 形式で出力

**symbol_id の使用例:**
```bash
# 名前で検索（曖昧な場合あり）
codanna retrieve calls process_file

# ID で検索（常に一意）
codanna retrieve calls symbol_id:1883

# calls, callers, describe で利用可能
```

`codanna serve`  
MCP サーバーを起動し、HTTP/HTTPS モードも選択可能

**オプション:**
- `--watch` - インデックス変更時にホットリロード
- `--watch-interval <WATCH_INTERVAL>` - インデックス変更チェック間隔（デフォルト: 5）
- `--http` - stdio トランスポートの代わりに HTTP サーバーとして実行
- `--https` - TLS 対応の HTTPS サーバーとして実行
- `--bind <BIND>` - HTTP/HTTPS サーバーのバインドアドレス（デフォルト: 127.0.0.1:8080）

`codanna config`  
有効な設定を表示

`codanna mcp-test`  
MCP 接続をテストし、利用可能なツールを一覧表示

`codanna mcp <TOOL> [POSITIONAL]...`  
サーバーを起動せず MCP ツールを直接実行

**引数:**
- `<TOOL>` - 呼び出すツール名
- `[POSITIONAL]...` - 位置引数（単純値または key:value ペア）

**オプション:**
- `--args <ARGS>` - ツール引数を JSON で指定（後方互換・複雑ケース用）
- `--json` - JSON 形式で出力

**利用可能ツール:**
| Tool | 説明 |
|------|-------------|
| `find_symbol` | 完全一致でシンボルを検索 |
| `search_symbols` | ファジーマッチを含む全文検索 |
| `semantic_search_docs` | 自然言語検索 |
| `semantic_search_with_context` | リレーションを含む自然言語検索 |
| `get_calls` | 関数が呼び出す関数（`function_name:<name>` または `symbol_id:ID`） |
| `find_callers` | 関数を呼び出す関数（`function_name:<name>` または `symbol_id:ID`） |
| `analyze_impact` | シンボル変更の影響範囲（`symbol_name:<name>` または `symbol_id:ID`） |
| `get_index_info` | インデックス統計情報 |

> ヒント: シンボル ID を受け付けるツールでは、プレーン名（`process_file`）または `symbol_id:1234` の完全修飾参照のどちらも使用できます。

`codanna benchmark [LANGUAGE]`  
パーサの性能をベンチマーク

**引数:**
- `[LANGUAGE]` - 対象言語 (rust, python, typescript, go, php, c, cpp, all) [デフォルト: all]

**オプション:**
- `-f, --file <FILE>` - カスタムファイルをベンチマーク

`codanna parse <FILE>`  
ファイルを解析し AST を JSON Lines で出力

**引数:**
- `<FILE>` - 解析するファイル

**オプション:**
- `-o, --output <OUTPUT>` - 出力ファイル（デフォルト: stdout）
- `-d, --max-depth <MAX_DEPTH>` - 走査する最大深さ
- `-a, --all-nodes` - すべてのノードを含める（デフォルトは名前付きノードのみ）

`codanna plugin <SUBCOMMAND>`  
Git ベースマーケットプレイスから Claude Code プラグインを管理

> **詳細ドキュメント:** 詳細な使用方法、プラグイン作成、マーケットプレイス構造については [Plugin System Documentation](../plugins/) を参照してください。

**サブコマンド:**
| Subcommand | 説明 |
|------------|-------------|
| `plugin add` | マーケットプレイスリポジトリからプラグインをインストール |
| `plugin remove` | インストール済みプラグインを削除しファイルをクリーンアップ |
| `plugin update` | プラグインを新しいバージョンに更新 |
| `plugin list` | インストール済みプラグインとそのバージョンを一覧 |
| `plugin verify` | プラグインファイルが想定チェックサムと一致するか検証 |

`plugin add <MARKETPLACE> <PLUGIN_NAME>`  
マーケットプレイスリポジトリからプラグインをインストール

**引数:**
- `<MARKETPLACE>` - マーケットプレイスリポジトリ URL またはローカルパス
- `<PLUGIN_NAME>` - インストールするプラグイン名

**オプション:**
- `--ref <REF>` - Git リファレンス（ブランチ、タグ、コミット SHA）
- `-f, --force` - 競合があっても強制インストール
- `--dry-run` - 変更を加えず実行内容を表示

#`plugin remove <PLUGIN_NAME>`  
インストール済みプラグインを削除しファイルをクリーンアップ

**引数:**
- `<PLUGIN_NAME>` - 削除するプラグイン名

**オプション:**
- `-f, --force` - 他プラグインが依存していても強制削除
- `--dry-run` - 変更を加えず実行内容を表示

`plugin update <PLUGIN_NAME>`  
プラグインを新しいバージョンに更新

**引数:**
- `<PLUGIN_NAME>` - 更新するプラグイン名

**オプション:**
- `--ref <REF>` - 指定 Git リファレンスへ更新
- `--dry-run` - 変更を加えず実行内容を表示

`plugin list`  
インストール済みプラグインとそのバージョンを一覧表示

`plugin verify <PLUGIN_NAME>`  
プラグインファイルが想定チェックサムと一致するか検証

**引数:**
- `<PLUGIN_NAME>` - 検証するプラグイン名

## ヘルプの取得

任意のコマンドまたはサブコマンドの詳細ヘルプを表示:

```bash
# トップレベルコマンドのヘルプ
codanna help <command>
codanna <command> --help

# サブコマンドのヘルプ
codanna help retrieve <subcommand>
codanna retrieve <subcommand> --help
codanna help plugin <subcommand>
codanna plugin <subcommand> --help
```

---

## プロファイルシステム

プロファイルは再利用可能なフック、コマンド、設定をパッケージ化します。プロバイダー（Git リポジトリまたはローカルフォルダ）はプロファイルを配布し、グローバルに登録されます。インストールはワークスペースごとに管理されます。

> **完全ガイド:** ワークフロー、保存場所、構造については [Profile System Documentation](../profiles/README.md) を参照してください。

| Command | 説明 |
|---------|-------------|
| `codanna profile provider add <source>` | プロバイダーを登録（GitHub 省略形、git URL、ローカルパス） |
| `codanna profile list [--verbose] [--json]` | 登録済みプロバイダーが提供するプロファイルを確認 |
| `codanna profile install <name> [--force]` | プロファイルを現在のワークスペースにインストール |
| `codanna profile status [--verbose]` | インストール済みプロファイルを表示 |
| `codanna profile sync [--force]` | ワークスペース lockfile に基づきプロファイルをインストール |
| `codanna profile update <name> [--force]` | インストール済みプロファイルを最新に更新 |
| `codanna profile verify [<name>] [--all] [--verbose]` | インストール済みプロファイルの整合性を検証 |
| `codanna profile remove <name> [--verbose]` | ワークスペースからプロファイルを削除 |

プロファイルは `~/.codanna` にキャッシュされ、ワークスペースへのインストールは `.codanna/profiles.lock.json` で管理されます。

---

## 終了コード

- `0` - 正常終了
- `1` - 一般エラー
- `3` - 見つからない（retrieve コマンドで使用）

## 注意

- すべての retrieve コマンドは `--json` フラグで構造化出力に対応
- MCP ツールは位置引数と key:value 引数の両方をサポート
- plugin コマンドは Codanna の拡張機能を管理
- profile コマンドはワークスペース設定とプロバイダー登録を管理
- index、plugin add、plugin remove では `--dry-run` で変更前プレビューが可能
- セマンティック検索で言語フィルタリング可能: `lang:rust`, `lang:typescript` など
- プロファイルはグローバル (`~/.codanna/providers.json`) に保存され、ワークスペース単位でインストール (`.codanna/profiles.lock.json`) される
```