# main.rs Review

## TL;DR

- CLIのエントリポイントとして、コードインテリジェンスの全機能（index/retrieve/serve/mcp/benchmark/parse/plugin/profile）を統合する中枢。主要公開インタフェースはCLIサブコマンド群。
- コアロジックは「設定ロード」「永続インデックスのロード/再構築」「プロバイダ（TypeScript）初期化」「インデックス同期」「MCPサーバ起動（stdio/http/https）」「retrieve/mcpツール呼び出し」に大別。
- 複雑箇所は main の分岐量（サーバモード選択、watcher起動、インデックス同期、retrieveマルチルート、MCPツール前処理/後処理）。Mermaid図で可視化。
- 重大リスクは「長大な main による保守性低下」「エラー処理の散在と std::process::exit 多用」「unsafe（ポインタ境界検査）」「外部ロック・非同期タスクの並行性」。
- Rust安全性: 所有権/借用は概ね安全（Arc中心）。unsafeはベンチでポインタ境界検査のみ。Send/Syncは外部型依存のため「不明」箇所あり。tokio::spawnを複数箇所で使用。
- セキュリティ: 権限/認可はMCP/HTTP/HTTPS実装に委譲（このファイルには現れない）。パス入力・ファイル監視・HTTPバインドの取り扱いに注意。
- テストは設定/インデックス種子化のユニットテストあり。統合テストは不足。追加のIntegration/E2Eテスト提案を記載。

## Overview & Purpose

このファイルは codanna のCLIエントリポイントであり、ユーザが実行するサブコマンド（init、index、retrieve、serve、mcp、benchmark、parse、plugin、profile など）を定義し、設定ロード、インデックス永続化、およびMCPサーバ起動までを一貫してオーケストレーションします。外部クレート codanna に存在するインデクサ、パーサ、MCPサーバ/クライアント、各種ウォッチャと連携し、対話/非対話両モードでコードインテリジェンスを提供します。

目的は以下です:
- 設定ファイル（settings.toml）の初期化/更新/表示
- コードベースのインデックス構築/再構築/同期（構成変更に追随）
- 取得系（retrieve）と直接MCPツール呼び出し（mcp）の両導線
- サーバ起動（stdio・HTTP・HTTPS）とファイル/設定監視によるホットリロード
- パーサ性能ベンチマークとAST出力（parse）
- プラグイン/プロファイル管理

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | Cli | private | 全体のCLI引数とグローバルオプション定義 | Low |
| Enum | Commands | private | サブコマンド（init/index/retrieve/serve/...）の定義 | High |
| Enum | PluginAction | private | pluginサブコマンドのサブアクション定義 | Med |
| Enum | RetrieveQuery | private | retrieveサブコマンドのクエリ種類 | Med |
| Struct | IndexInfo | private(Serialize) | MCPツール出力JSONの統計データ | Low |
| Struct | SymbolKindBreakdown | private(Serialize) | MCP統計のシンボル種別集計 | Low |
| Struct | SemanticSearchInfo | private(Serialize) | セマンティックサーチの状態・メタ情報 | Low |
| Fn | clap_cargo_style | private | clapのスタイル定義 | Low |
| Fn | create_custom_help | private | ユーザフレンドリなヘルプ文字列生成 | Low |
| Fn | create_provider_registry | private | プロバイダレジストリ構築（TypeScript） | Low |
| Fn | initialize_providers | private | 設定からプロバイダ初期化（検証/キャッシュ） | Med |
| Enum | SkipReason | private | 設定に追加しない理由の分類 | Low |
| Struct | SkippedPath | private | スキップされたパスの記録 | Low |
| Struct | SeedReport | private(Default) | インデクサへの初期seedの結果報告 | Low |
| Fn | seed_indexer_with_config_paths | private | インデクサへ設定ディレクトリ群を種まき | Med |
| Fn | add_paths_to_settings | private | 設定にパスを追加（重複/包含の扱い対応） | Med |
| Fn | main (async) | private | コマンド実行の中枢（設定・インデックス・サーバ） | High |
| Fn | run_parse_command | private | AST出力（parseコマンド） | Low |
| Fn | run_benchmark_command | private | ベンチマークオーケストレーション | Low |
| Fn | benchmark_rust_parser 等 | private | 各言語パーサのベンチ実行 | Low |
| Fn | benchmark_parser | private | パーサの共通ベンチ処理（unsafeあり） | Med |
| Fn | generate_*_benchmark_code | private | 疑似コード生成（各言語） | Low |

### Dependencies & Interactions

- 内部依存
  - main -> Settings/IndexPersistence/SimpleIndexer（設定・永続化・インデックス）
  - main -> create_provider_registry/initialize_providers（Index前初期化）
  - main -> seed_indexer_with_config_paths/sync_with_config（インデックスルートの同期）
  - main -> retrieve::retrieve_*（codanna側の取得関数群へ委譲）
  - main -> mcp::CodeIntelligenceServer/MCP HTTP/HTTPS/stdioサーバ（サーバ起動とツール実行）
  - main -> FileSystemWatcher/ConfigFileWatcher/IndexWatcher（ウォッチャ起動）
  - main -> parse/benchmark/plugin/profile クレート領域へ委譲

- 外部依存（主要）
  | クレート/モジュール | 用途 |
  |------------------|------|
  | clap | CLI引数定義・スタイル |
  | serde/serde_json/toml | 設定・JSON出力 |
  | tokio | 非同期ランタイム・spawn |
  | rmcp | MCPプロトコルサーバ/クライアント |
  | console | ヘルプスタイル |
  | tempfile | テスト用一時ディレクトリ |
  | codanna::* | 設定/インデクサ/パーサ/MCP/ウォッチャ/IOユーティリティ 等（詳細はこのチャンクには現れない） |

- 被依存推定
  - バイナリコマンド「codanna」を直接起動するユーザ/CI/CD。
  - MCPクライアント/インスペクタ（例: @modelcontextprotocol/inspector）。
  - Claude Code等のIDE統合からのMCPアクセス。
  - プロジェクト内の .codanna/settings.toml を共有するスクリプト。

## API Surface (Public/Exported) and Data Contracts

このファイル自体から「pub」なAPIは輸出されません（バイナリ）。実質的な公開面は「CLIコマンド」および「MCPツール」インタフェースです。

### API一覧

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| codanna init | Commands::Init { force: bool } | 設定ファイル初期化 | O(1) | O(1) |
| codanna config | Commands::Config | 有効設定の表示 | O(n)（設定サイズ） | O(1) |
| codanna index | Commands::Index { paths, threads, force, progress, dry_run, max_files } | インデックス構築/再構築 | O(F + S) 参照 | O(I) 参照 |
| codanna add-dir | Commands::AddDir { path } | 設定にインデックス対象ディレクトリを追加 | O(p) | O(1) |
| codanna remove-dir | Commands::RemoveDir { path } | 設定から削除 | O(p) | O(1) |
| codanna list-dirs | Commands::ListDirs | 設定のindexed_paths表示 | O(n) | O(1) |
| codanna retrieve | Commands::Retrieve { query } | シンボル/関係/検索の取得 | O(q) 参照 | O(r) |
| codanna serve | Commands::Serve { watch, watch_interval, http, https, bind } | MCPサーバ起動 | O(1) 起動、ランタイム常駐 | O(常駐) |
| codanna mcp-test | Commands::McpTest { server_binary, tool, args, delay } | MCP接続テスト | O(1) | O(1) |
| codanna mcp | Commands::Mcp { tool, positional, args, json } | MCPツール直接呼び出し | O(q) 参照 | O(r) |
| codanna benchmark | Commands::Benchmark { language, file } | パーサ性能測定 | O(parse) | O(1) |
| codanna parse | Commands::Parse { file, output, max_depth, all_nodes } | ASTのJSONL出力 | O(file) | O(1) |

注: O(F + S) は「ファイル数 F + シンボル数 S」に依存。O(q)/O(r)は問い合わせ/結果に依存。外部 codanna の実装に依るため厳密値は「このチャンクには現れない」。

### Data Contracts

- IndexInfo
  ```rust
  #[derive(Debug, Serialize)]
  struct IndexInfo {
      symbol_count: usize,
      file_count: usize,
      relationship_count: usize,
      symbol_kinds: SymbolKindBreakdown,
      semantic_search: SemanticSearchInfo,
  }
  ```
- SymbolKindBreakdown
  ```rust
  #[derive(Debug, Serialize)]
  struct SymbolKindBreakdown {
      functions: usize,
      methods: usize,
      structs: usize,
      traits: usize,
  }
  ```
- SemanticSearchInfo
  ```rust
  #[derive(Debug, Serialize)]
  struct SemanticSearchInfo {
      enabled: bool,
      model_name: Option<String>,
      embeddings: Option<usize>,
      dimensions: Option<usize>,
      created: Option<String>,
      updated: Option<String>,
  }
  ```

例: get_index_info のJSON応答（簡略）
```json
{
  "data": {
    "symbol_count": 12345,
    "file_count": 321,
    "relationship_count": 45678,
    "symbol_kinds": {
      "functions": 8000,
      "methods": 2000,
      "structs": 200,
      "traits": 100
    },
    "semantic_search": {
      "enabled": true,
      "model_name": "text-embedding-3",
      "embeddings": 9876,
      "dimensions": 1536,
      "created": "2 hours ago",
      "updated": "5 minutes ago"
    }
  }
}
```

### 代表的APIの詳細

1) add_paths_to_settings
- 目的と責務
  - 設定ファイルから Settings をロードし、与えられたパス群を indexed_paths に追加、保存。ディレクトリのみ永続化。包含関係や既存重複を説明可能にスキップ記録。
- アルゴリズム（簡略）
  1. 設定ロード
  2. 各パスについて:
     - ファイルは strict=true なら Err、strict=false なら Skipped(FileNotPersisted)
     - Settings::add_indexed_path 実行
       - 「already indexed」メッセージに基づき包含関係を判定して SkipReason を付与
  3. 追加があれば設定保存
- 引数
  | 引数 | 型 | 説明 |
  |------|----|------|
  | paths | &[PathBuf] | 追加候補のパス群 |
  | config_path | &Path | 設定ファイルの位置 |
  | strict | bool | add-dir用途ではtrue（重複はエラー）、index用途ではfalse（冪等） |
- 戻り値
  | 戻り値 | 型 | 説明 |
  |--------|----|------|
  | Ok | (Settings, Vec<PathBuf>, Vec<SkippedPath>) | 更新後設定・追加されたパス・スキップ記録 |
  | Err | String | 失敗の理由 |
- 使用例
  ```rust
  let (settings, added, skipped) =
      add_paths_to_settings(&[PathBuf::from("src")], Path::new(".codanna/settings.toml"), false)
      .expect("update failed");
  ```
- エッジケース
  - パスがファイル: strict=trueでエラー、falseでスキップ
  - 既に親ディレクトリ登録済み: CoveredBy でスキップ
  - 設定保存失敗: Err

2) seed_indexer_with_config_paths
- 目的
  - 設定の indexed_paths を SimpleIndexer に同期（ディレクトリのみ、存在チェック、重複排除）
- アルゴリズム
  1. indexer.get_indexed_paths をHashSet化
  2. 各パスについて存在/ディレクトリ判定、未登録なら add_indexed_path
  3. 追加/欠落の報告を返す
- 引数/戻り値
  | 引数 | 型 | 説明 |
  |------|----|------|
  | indexer | &mut SimpleIndexer | インデクサ |
  | config_paths | &[PathBuf] | 設定登録済みパス群 |
  | debug | bool | デバッグ出力フラグ |
  | 戻り値 | SeedReport | 追加と欠落の結果 |
- 使用例
  ```rust
  let report = seed_indexer_with_config_paths(&mut indexer, &settings.indexing.indexed_paths, true);
  ```
- エッジケース
  - パスが存在しない: missing_paths に記録
  - ファイル/非ディレクトリ: スキップ

3) initialize_providers
- 目的
  - Settings に記載された言語ごとの config_files を検証し、プロバイダキャッシュを再構築
- ステップ
  1. registry.active_providers(settings) を列挙
  2. config_paths を存在チェックし、欠落があれば IndexError::ConfigError で詳細理由を返す
  3. rebuild_cache(settings) を試行（失敗は警告で継続）
- 引数/戻り値
  | 引数 | 型 |
  |------|----|
  | registry | &SimpleProviderRegistry |
  | settings | &Settings |
  | 戻り値 | Result<(), IndexError> |
- エッジケース
  - 設定にconfig_filesがない: スキップ
  - 複数欠落ファイル: 詳細メッセージを構築して返す（提案含む）

4) main（要約）
- 目的
  - CLIパーサからコマンド分岐、設定/インデックスのロード/再構築、MCPサーバ起動や各機能の呼び出しを統括
- ステップ（主要）
  - auto-init 設定（indexコマンドで未初期化なら作成）
  - 設定ロード（--config優先）
  - parseコマンドは早期実行/終了
  - インデックス永続層（IndexPersistence）準備、必要に応じてロード or 新規作成、traitリゾルバの遅延初期化
  - 設定の indexed_paths をインデクサへ種まき、セマンティック検索有効化、メタデータとの同期
  - サブコマンド分岐（serve/index/retrieve/mcp/etc）、それぞれの処理とエラー終了管理

5) benchmark_parser
- 目的
  - 指定言語パーサの平均処理時間・シンボルレート測定、ポインタ境界チェックで追加メモリ割当の兆候を検知
- ステップ
  1. Warm-up 呼び出し
  2. 3回のパース時間計測、平均算出
  3. find_callsの結果の先頭要素で &str の範囲が元コードスライス内か unsafe なポインタ比較で検査
- unsafeの不変条件
  - code.as_ptr() に対する add(code.len()) はコードバッファの終端アドレスを意味し、比較に使用するのみ
  - within_bounds 判定は解放済みメモリに触れない（読み書きなし）
- エッジケース
  - find_calls が空: 境界検査スキップ

## Walkthrough & Data Flow

### main の主要分岐とデータフロー

```mermaid
flowchart TD
    A[Cli::parse] --> B{--config ?}
    B -->|Yes| C[Settings::load_from(path)]
    B -->|No| D[Settings::load or default]
    D --> E{Command == Parse?}
    C --> E
    E -->|Yes| F[run_parse_command -> exit]
    E -->|No| G[IndexPersistence::new(index_path)]
    G --> H{skip_index_load?}
    H -->|Yes| I[SimpleIndexer::with_settings]
    H -->|No| J{persistence.exists && !force?}
    J -->|Yes| K[persistence.load_with_settings_lazy(settings, info, skip_trait)]
    J -->|No| L[clear if force; SimpleIndexer::with_settings_lazy]
    K --> M[seed_indexer_with_config_paths]
    L --> M
    M --> N{semantic_search enabled?}
    N -->|Yes| O[indexer.enable_semantic_search()]
    N -->|No| P[continue]
    O --> Q{persistence.exists && !force}
    P --> Q
    Q -->|Yes| R[IndexMetadata::load -> indexer.sync_with_config]
    Q -->|No| S[skip sync]
    R --> T{Serve?}
    S --> T
    T -->|Yes| U{https/http/stdio}
    U -->|https| V[serve_https (feature-gated)]
    U -->|http| W[serve_http]
    U -->|stdio| X[CodeIntelligenceServer::new(indexer)]
    X --> Y[IndexWatcher/FileSystemWatcher/ConfigFileWatcher spawn]
    Y --> Z[server.serve(stdio).await; waiting().await]
    T -->|No| AA{Index/Retrieve/Mcp/...}
    AA --> AB[index / retrieve / mcp 実行と終了コード処理]
```

上記の図は main 関数の主要分岐の概略を示します（行番号: 不明）。

代表的抜粋（モード選択）:
```rust
match server_mode {
    "https" => { /* ... 省略 ... */ }
    "http" => { /* ... 省略 ... */ }
    _ => { /* stdio ... watchers spawn ... serve(stdio) ... waiting() ... */ }
}
```

### MCPツール呼び出し（embeddedモード）

```mermaid
flowchart TD
    A[parse_positional_args + --args(JSON)] --> B[arguments(Map)構築]
    B --> C{json && tool == pre-collect対象?}
    C -->|Yes| D[インデクサから必要データ先取り]
    C -->|No| E[skip]
    D --> F[guidance_config 取得]
    B --> G[CodeIntelligenceServer::new(indexer)]
    G --> H[server.tool(Parameters(..)).await]
    H --> I{json?}
    I -->|Yes| J[pre-collectedデータを整形・JsonResponse]
    I -->|No| K[rmcp Text content をprintln]
```

上記の図は Commands::Mcp 分岐の主要フロー（行番号: 不明）。

## Complexity & Performance

- add_paths_to_settings
  - 時間: O(p + m) 近似（p=引数パス数、m=既存 indexed_paths の線形検査）
  - 空間: O(1)（一時ベクトル/エラー文字列）
- seed_indexer_with_config_paths
  - 時間: O(k)（k=config_paths数）＋HashSet生成 O(t)（t=既存追跡パス数）
  - 空間: O(t)
- initialize_providers
  - 時間: O(n)（n=有効プロバイダ数）＋各プロバイダのキャッシュ構築時間（このチャンクには現れない）
- main
  - 起動時の支配項は「インデックスロード/同期/検索/サーバ待ち」。
  - インデクサの search/index/semantic の複雑度は外部実装に依存（不明）。
- benchmark_parser
  - 時間: O(parse)×3回、find_calls のコストはパーサに依存（不明）
- スケール限界/ボトルネック
  - 大規模プロジェクトではインデックスロード/同期がI/OとCPUに支配される
  - semantic_search 有効化時の初期コスト（モデル/ベクトルロード）
  - rmcpサーバのstdioモードは単一プロセスでの処理キューに依存

実運用負荷要因:
- ファイル数、シンボル数、関係数の増加
- ファイル/設定ウォッチャに伴うイベント頻度（デバウンス設定）
- HTTP/HTTPSのネットワークI/O（このチャンクには詳細実装なし）
- Tantivy（検索エンジン）のインデックスI/O（外部）

## Edge Cases, Bugs, and Security

### エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空のretrieve引数 | codanna retrieve symbol | Usage表示→Err | 明示的チェック | OK |
| indexで設定にパスなし | codanna index | エラー文言/ガイダンス表示 | 実装済み | OK |
| add-dirにファイルを渡す | codanna add-dir file.rs | エラー（strict） | 実装済み | OK |
| indexにファイルを渡す | codanna index file.rs | ファイルをインデックス（非永続） | 実装済み | OK |
| 設定ファイル欠落 | 任意 | 既定設定/警告 | 実装済み | OK |
| providerのconfig_files欠落 | settings.tomlが不正 | 詳細なIndexError::ConfigError | 実装済み | OK |
| 既存メタデータ不整合 | metadataロード失敗 | 回復手順表示/スキップ | 実装済み | OK |
| semantic未有効でsemantic_search呼び出し | mcpツール | エラーJSONと提案 | 実装済み | OK |
| httpsサーバ機能未コンパイル | --https | エラーメッセージ・終了 | 実装済み | OK |

### バグ/懸念

- main の分岐が非常に大きく、危険な複合条件の取り違い/回帰を招きやすい（保守性低下）。
- std::process::exit の多用により、テスト/組込環境での扱いが難しく、リソースクリーンアップ機会を失う。
- retrieve/mcp の引数パースと JSONレスポンス構築の重複ロジックが散在（DRY違反）し、仕様拡張時の不整合リスク。
- benchmark_parser の unsafe 境界チェックは注意が必要（ただし内容は読み取りしない比較のみで低リスク）。

### セキュリティチェックリスト

- メモリ安全性
  - Buffer overflow: String生成/フォーマットのみで低リスク。unsafe は境界比較のみ。
  - Use-after-free: なし（安全な所有権/借用モデル）。
  - Integer overflow: JSON->number変換時に i64/f64 を使用、極端値は不明（このチャンクには現れない）。閾値/limitはデフォルトを持ち、過大値の場合のガードを追加推奨。
- インジェクション
  - SQL/Command: 直接なし。外部MCP/HTTPへ渡す引数はパラメータ化される想定（実装は外部）。
  - Path traversal: ユーザ指定パスを素朴に扱うため、シンボリックリンク/親ディレクトリ参照は設定の責務。canonicalize の活用あり（包含判定）。さらにファイル監視での不正リンクに対するガードが外部に必要。
- 認証・認可
  - HTTP/HTTPSの認証はこのファイルでは設定のみ。OAuth/TLSは外部実装に依存（不明）。
- 秘密情報
  - ハードコード秘密はなし。設定内容の表示時、機微情報のマスク機構は不明（このチャンクには現れない）。
- 並行性
  - Race condition: tokio::spawnでWatcher/Serverを複数起動。共有リソース（indexer Arc）への同時読み書きは codanna の内部同期に依存。
  - Deadlock: parsing::get_registry().lock() を使用（Registryのロック）。長時間ロックしないが、潜在的な再入リスクは外部実装依存。
  - キャンセル: spawnタスクのキャンセル制御が明示的にはない。終了時のクリーンアップは課題。

### Rust特有の観点

- 所有権/借用
  - Arc<Settings>/Arc<Indexer> を共有し複数タスクから参照。ミュータブル操作は codanna 内側の同期に依存（行番号: 不明）。
- ライフタイム
  - 明示的ライフタイムは不要。関数境界で所有権が完結。
- unsafe境界
  - 使用箇所: benchmark_parser 内のポインタ境界チェック（行番号: 不明）
  - 保証すべき条件: code の寿命中のみ比較、書き込みなし、パース後にコードスライス解放前に比較。
  - 安全性根拠: within_bounds は比較のみで未定義動作を引き起こす操作なし。
- 並行性・非同期
  - Send/Sync: CodeIntelligenceServer/Indexer が Send/Sync を満たすかは外部型（不明）。spawn 渡しはArcでラップ済み。
  - データ競合: Indexer共有。更新系操作は外部同期に依存。
  - await境界: サーバ起動/HTTP/HTTPS/stdio serve、MCP呼び出しが await。UI/出力直列化。
  - キャンセル: 明示制御なし。プロセス終了で強制停止。
- エラー設計
  - Result vs Option: 収集フェーズで Option を多用（関数名/IDの有無）。外部関数は Result返し。
  - panic箇所: expect/unwrap_or_else の利用あり（ベンチ、設定ロード失敗時にexit）。本番経路での expect は限定的。
  - エラー変換: Stringで単純化（add_paths_to_settings）と codanna::io::ExitCode で分類。

## Design & Architecture Suggestions

- main の肥大化を解消
  - serve/index/retrieve/mcp 毎にモジュール分割し、Commandハンドラを独立ファイルへ。コマンドごとに「入力検証→ビジネスロジック→出力整形」を三層化。
- エラー処理の一元化
  - 独自の Error 型（enum）と anyhow/thiserror を利用し、std::process::exit を最終レイヤのみで使用。JSON/textのレスポンス組み立ては ResponseBuilder で共通化。
- 引数パースの統一
  - retrieve/mcp で parse_positional_args の結果を型安全な構造体へデコードするヘルパを導入。kind/module/lang など定型のキーは Clap の value_parser でバリデーション。
- ロギング/トレースの導入
  - eprintln から tracing/log へ移行。レベル、ターゲット、span を付けてサーバ・ウォッチャイベントを可視化。HTTP/HTTPSはリクエストIDをspan化。
- 非同期タスク管理
  - 監視タスクのキャンセル/終了協調（signal handling）。tokio::select! でサーバ終了とタスクキャンセル。
- JSONレスポンスのテンプレート化
  - JsonResponse 構築の重複を排除し、ツールごとに Result<DomainModel, ToolError> を serialize する共通パス。
- 型付けの強化
  - MCPツール引数を strongly-typed リクエスト構造体へマッピングし、serdeで both JSON/positional を取り込む。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト（既存）
  - add_paths_to_settings のスキップ記録/包含判定
  - seed_indexer_with_config_paths の追跡整合性
  - ファイルパスの非永続化
  - CLI構成の検証（Clapのdebug_assert）

- 追加のユニットテスト
  - initialize_providers: 欠落ファイル時の詳細メッセージと提案の確認
  - MCPツール前処理: json=true 時の pre-collect の整合性（例: find_symbol, get_calls）
  - semantic_search の有無でのエラーレスポンス分岐

- 統合テスト（提案）
  - 「index → retrieve → serve(stdio) → mcp ツール」までの一連動作を TempDir で再現
  - metadata 同期: 既存メタデータの差分検出・追加/削除の確認
  - ファイル監視: 変更トリガで通知/再読み込みが発火するか（外部モジュールに依存）

- 例: initialize_providers のテストスケルトン
```rust
#[test]
fn ts_provider_missing_config_files_returns_config_error() {
    // このチャンクには TypeScriptProvider の詳細実装が現れないため擬似設定を用意
    let registry = create_provider_registry();
    let mut settings = Settings::default();
    // 仮: settings に存在しないパスを登録
    settings.languages.typescript.config_files = vec![PathBuf::from("missing/tsconfig.json")];
    let result = initialize_providers(&registry, &settings);
    assert!(result.is_err());
    // IndexError::ConfigError の reason に含まれる提案文言を確認（このチャンクには詳細不明なら文字列包含テスト）
}
```

- 例: MCPのpre-collect結果のJSON整形テスト
```rust
#[test]
fn mcp_find_symbol_json_empty_results_not_found_response() {
    // arguments Mapで name を指定し、indexer を空にした場合の pre-collect が空配列になること
    // その場合の JsonResponse::not_found が期待どおりか確認
    // このチャンクでは外部型のモックが必要 → 不明
}
```

## Refactoring Plan & Best Practices

- ステップ1: コマンド別モジュール切り出し（serve.rs/index.rs/retrieve.rs/mcp.rs）
- ステップ2: エラー/終了コードのポリシー策定。Error→ExitCodeの集中変換レイヤを用意。
- ステップ3: 引数→型変換の共通ヘルパ導入（positionalと--args JSONの統合）
- ステップ4: JsonResponse ビルダ/ガイダンス追加の共通化
- ステップ5: tracing を導入し、デバッグ出力（DEBUG: ...）をレベル付きログへ集約
- ステップ6: タスクのキャンセレーションを導入し、serve終了時にwatcherを停止
- ステップ7: unsafe 検査のユーティリティ化（テスト可能な境界チェッカ）

ベストプラクティス:
- 「出口（exit）」は最上位コマンドハンドラのみ
- 「外部呼び出し（codanna::*）」はエラーを詳細化し、ユーザ向けの回復策を常に提示
- 「設定変更（indexed_paths）」は読み/書きを厳格に分離し、ロック順序を規定

## Observability (Logging, Metrics, Tracing)

- ログ
  - tracing を導入し、span: serve_mode、watchers、index_sync、retrieve、mcp_call を定義
  - 構造化フィールド（bind, watch_interval, added/removed, symbols_count, file_count）
- メトリクス
  - Prometheus互換のメトリクス（index_size、search_latency、semantic_enabled、watch_events）
  - MCPツールごとの呼び出し回数/失敗率
- トレーシング
  - MCPリクエストIDをspanに紐付け、retrieve/search/semantic の上下流（indexer操作）を視覚化
- 例（tracing）の適用例
```rust
tracing::info!(mode = server_mode, bind = %bind_address, "Starting MCP server");
```

## Risks & Unknowns

- codanna::SimpleIndexer/IndexPersistence/MCP/Watcher の内部仕様はこのチャンクには現れないため、Send/Sync/ロック戦略/エラー分類の詳細は不明。
- HTTP/HTTPSモードの認証/認可/OAuth/TLS管理は外部実装に依存し、ここではモード選択のみ。
- semantic_search のモデル/閾値/埋め込み管理の詳細は不明。大規模インデックスでの性能とメモリフットプリントに影響。
- rmcp サービスの待機/エラーの再試行戦略、致命的障害時の復旧手順は不明。
- Windowsのファイルロック問題に対する対処（clear_symbol_cache）が言及されるが、根本条件は外部に依存。