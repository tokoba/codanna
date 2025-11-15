# parsers\typescript\test_alias_resolution.rs Review

## TL;DR

- 目的: **TypeScriptのパスエイリアス解決**が、索引なしの環境でも期待通りに強化（enhance）されるかを検証するテスト群。
- 主要利用API（本ファイルでのコアロジック）: **TypeScriptProjectEnhancer::enhance_import_path**、**TypeScriptBehavior::add_import / get_imports_for_file**、**ResolutionPersistence::load**、**TypeScriptProvider::rebuild_cache**。
- 複雑箇所: テスト内での**ファイルシステム操作**（.codannaディレクトリ作成・移動）、**カレントディレクトリの変更**、**外部設定ロード**に依存する部分。
- 重大リスク: テストが**グローバル環境に副作用**（cwd変更、ディレクトリ移動）を与え、並列実行時に**レース**や**フレーク**の原因となりうる。
- 安全性: Rustの**unsafe**は未使用、メモリ安全性は高いが、`unwrap`による**パニック**可能性あり（テスト用途としては許容範囲）。
- パフォーマンス: 計算は軽微（O(n)のケース走査）、ただし**I/O**や**キャッシュ再構築**があるテストは遅くなり得る。
- 不明点: codannaライブラリ内部の詳細（解決アルゴリズム、キャッシュ仕様）はこのチャンクには現れない。

## Overview & Purpose

このファイルは、TypeScriptのパスエイリアス（例: "@/components/*" -> "./src/components/*"）の解決・強化が、プロジェクトのルール（tsconfig相当）を用いて正しく行われるかを検証する**テスト**です。主に以下を確認します。

- エイリアスの各種パターンが**適切に強化**されること（`test_import_enhancement_with_aliases`）。
- 強化済みのパスが**モジュールパス**（ドット区切りの名前空間）へ正しく変換されること（`test_module_path_computation`）。
- `TypeScriptBehavior`が**プロジェクトルールを考慮**して`add_import`時にパスを強化すること（`test_typescript_behavior_add_import`）。
- 実際の**プロジェクト設定（Settings）とプロバイダー**を通じて、永続化済みのルールから**強化が成功**すること（`test_resolution_with_project_rules`）。

これらはcodannaのTypeScriptパーサ/リゾルバの正当性と、設定連携の健全性を担保するための重要な回帰テストです。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_import_enhancement_with_aliases | private | エイリアスマッピングによるパス強化の単体検証 | Low |
| Function | test_module_path_computation | private | 強化済みパスからモジュールパスへ変換の検証 | Low |
| Function | test_typescript_behavior_add_import | private | `.codanna`テスト環境を構築し、Behavior経由でインポート強化を検証 | Med |
| Function | test_resolution_with_project_rules | private | Settings/Provider/Persistence経由でプロジェクトルールをロードし、強化の動作確認 | Med |

### Dependencies & Interactions

- 内部依存（このファイル内の呼び出し関係）
  - 各テスト関数は**独立**しており、相互呼び出しはありません。

- 外部依存（使用クレート・モジュール）
  | ライブラリ/モジュール | 使用シンボル | 目的 |
  |-----------------------|--------------|------|
  | codanna::FileId | FileId::new | テスト用のファイル識別子生成 |
  | codanna::config::Settings | Settings::load | 設定ファイル（言語設定）のロード |
  | codanna::parsing::resolution | ProjectResolutionEnhancer | 型参照（本テストで直接未使用） |
  | codanna::parsing::typescript::behavior | TypeScriptBehavior | TypeScriptのインポート管理/強化 |
  | codanna::parsing::typescript::resolution | TypeScriptProjectEnhancer | エイリアス解決のコア強化機構 |
  | codanna::parsing | Import, LanguageBehavior | インポート表現および行動トレイト |
  | codanna::project_resolver::persist | ResolutionPersistence, ResolutionRules | ルール永続化と読み出し |
  | codanna::project_resolver::providers::typescript | TypeScriptProvider | TypeScript設定からルールをキャッシュ再構築 |
  | std::fs, std::env, std::path | fs操作、環境操作、パス操作 | テスト用の環境構築・後片付け |

- 被依存推定
  - このモジュールは**テスト専用**であり、他モジュールからの依存は想定されません。`cargo test`実行時に使用されます。

## API Surface (Public/Exported) and Data Contracts

公開APIはありません（テスト関数のみ）。以下はテスト関数の一覧と計算量の目安です。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| test_import_enhancement_with_aliases | `fn test_import_enhancement_with_aliases()` | エイリアス強化動作の検証 | O(k)（k=ケース数） | O(1) |
| test_module_path_computation | `fn test_module_path_computation()` | 強化パスをモジュール表現へ変換の検証 | O(k) + O(n)（文字列長） | O(1) |
| test_typescript_behavior_add_import | `fn test_typescript_behavior_add_import()` | ルール永続化を模した環境でBehaviorの強化を検証 | O(1)計算＋I/O依存 | O(size of files) |
| test_resolution_with_project_rules | `fn test_resolution_with_project_rules()` | Settings/Provider/Persistence連携による強化検証 | I/O依存 | O(cache + index) |

各APIの詳細:

1) test_import_enhancement_with_aliases
- 目的と責務
  - 与えた`ResolutionRules`に基づき、`TypeScriptProjectEnhancer::enhance_import_path`が期待通りに**パス強化**するかを検証。
- アルゴリズム（ステップ）
  - ルール構築 → Enhancer生成 → テストケースのループ → 強化結果の一致/不一致をアサート。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | なし | - | テスト関数 |
- 戻り値
  | 型 | 意味 |
  |----|------|
  | なし | パニックで失敗を表現 |
- 使用例
  ```rust
  // 重要部のみ（行番号: 不明）
  let rules = ResolutionRules {
      base_url: None,
      paths: vec![
          ("@/components/*".to_string(), vec!["./src/components/*".to_string()]),
          ("@/utils/*".to_string(), vec!["./src/utils/*".to_string()]),
          ("@/*".to_string(), vec!["./src/*".to_string()]),
      ].into_iter().collect(),
  };
  let enhancer = TypeScriptProjectEnhancer::new(rules);
  let file_id = FileId::new(1).unwrap();
  let result = enhancer.enhance_import_path("@/components/Button", file_id);
  assert_eq!(result.as_deref(), Some("./src/components/Button"));
  ```
- エッジケース
  - "@/components/ui/dialog"などの**ネスト**パス
  - "@/lib/api"のように**共通`@/*`ルール**にフォールバックするケース
  - "./relative/path"や"../parent/path"などの**相対/親参照**は強化しない
  - "react"のような**外部パッケージ**は強化しない

2) test_module_path_computation
- 目的と責務
  - 強化済みの相対パス（例: `./src/...`）を、プロジェクトプレフィックスを保持した**モジュールパス**に変換できるか検証。
- アルゴリズム
  - インポート元モジュール文字列からプロジェクトプレフィックス抽出 → `"./"`と`"/"`を除去 → `'/'`を`.`に変換 → プレフィックス付与。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | なし | - | テスト関数 |
- 戻り値
  | 型 | 意味 |
  |----|------|
  | なし | パニックで失敗を表現 |
- 使用例
  ```rust
  // 重要部のみ（行番号: 不明）
  let enhanced_path = "./src/components/Button";
  let importing_module = "examples.typescript.react.src.app";
  let project_prefix = "examples.typescript.react";
  let cleaned_path = enhanced_path.trim_start_matches("./")
                                  .trim_start_matches("/")
                                  .replace('/', ".");
  let target_module = format!("{project_prefix}.{cleaned_path}");
  assert_eq!(target_module, "examples.typescript.react.src.components.Button");
  ```
- エッジケース
  - プロジェクトプレフィックスが**存在しない**場合
  - インポート元に`.src.`が**含まれない**場合のフォールバック
  - パスに**余分な`/`**がある場合のトリム

3) test_typescript_behavior_add_import
- 目的と責務
  - `.codanna`配下にTypeScriptルールの**永続ファイル**を作成し、`TypeScriptBehavior`経由で`add_import`が**強化**を適用することを検証。
- アルゴリズム
  - テスト用`.codanna`と`typescript_resolution.json`作成 → テスト用ワークスペースへ移動 → `TypeScriptBehavior`生成 → `add_import` → `get_imports_for_file` → クリーンアップ。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | なし | - | テスト関数 |
- 戻り値
  | 型 | 意味 |
  |----|------|
  | なし | パニックで失敗を表現 |
- 使用例
  ```rust
  // 重要部のみ（>20行のため抜粋、行番号: 不明）
  let behavior = TypeScriptBehavior::new();
  let file_id = FileId::new(1).unwrap();
  let import = Import {
      path: "@/components/Button".to_string(),
      alias: Some("Button".to_string()),
      is_glob: false,
      is_type_only: false,
      file_id,
  };
  behavior.add_import(import.clone());
  let imports = behavior.get_imports_for_file(file_id);
  assert_eq!(imports.len(), 1);
  assert!(imports[0].path == "./src/components/Button"
          || imports[0].path == "@/components/Button");
  ```
- エッジケース
  - ルールが**ロードされない**場合は強化されず、**元パス**のまま
  - `.codanna`ディレクトリの**存在/権限**問題
  - Windows/Unixでの**パス表現差**によるJSONルールの不一致

4) test_resolution_with_project_rules
- 目的と責務
  - 実際の`Settings`/`TypeScriptProvider`/`ResolutionPersistence`を用いて、プロジェクトルールを**ビルド→ロード**し、強化が成功することを観察（成功しない場合もログにより情報収集）。
- アルゴリズム
  - Settingsロード → TypeScriptProviderで**キャッシュ再構築** → Persistenceから**ルール読み出し** → Enhancerで強化を試行。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | なし | - | テスト関数 |
- 戻り値
  | 型 | 意味 |
  |----|------|
  | なし | パニックではなく、失敗時は**println**で通知して早期returnもあり |
- 使用例
  ```rust
  // 重要部のみ（行番号: 不明）
  if let Ok(settings) = Settings::load() {
      let provider = TypeScriptProvider::new();
      if let Err(e) = provider.rebuild_cache(&settings) {
          println!("Warning: Could not build cache: {e}");
          return;
      }
      let persistence = ResolutionPersistence::new(Path::new(".codanna"));
      if let Ok(index) = persistence.load("typescript") {
          if let Some(rules) = index.rules.values().next() {
              let enhancer = TypeScriptProjectEnhancer::new(rules.clone());
              let file_id = FileId::new(1).unwrap();
              if let Some(enhanced) = enhancer.enhance_import_path("@/components/Button", file_id) {
                  println!("Successfully enhanced: @/components/Button -> {enhanced}");
              }
          }
      }
  }
  ```
- エッジケース
  - SettingsにTypeScriptが**未設定**、config_filesが**空**の場合
  - キャッシュ再構築が**失敗**する場合
  - `.codanna`に**永続ルールがない**場合

## Walkthrough & Data Flow

- test_import_enhancement_with_aliases
  - 入力: `ResolutionRules`にエイリアスパターン登録。
  - 処理: `TypeScriptProjectEnhancer::enhance_import_path(import_path, file_id)`をケースごとに呼び出し。
  - 出力: Some(強化パス)またはNone。アサーションで検証。
  - 根拠: 関数名のみ（行番号: 不明）。

- test_module_path_computation
  - 入力: 強化済みパス（`./src/...`）とインポート元モジュール文字列。
  - 処理:
    - プロジェクトプレフィックス抽出（特定の文字列が含まれるかの判定）。
    - 強化パスの整形（`./`と`/`を除去、`/`→`.`）。
    - プレフィックス付与。
  - 出力: 期待されるドット区切り表現と一致するかアサート。
  - 根拠: 関数名のみ（行番号: 不明）。

- test_typescript_behavior_add_import
  - 入力: `.codanna`相当のルールJSON、`Import`（エイリアス付）。
  - 処理:
    - テスト用ディレクトリ作成、ルールJSON配置、cwd変更。
    - `TypeScriptBehavior::new()`→`add_import`→`get_imports_for_file`。
    - cwd復元、ディレクトリ削除。
  - 出力: インポートが1件保存され、パスが強化されているか（または未強化だが保存）をアサート。
  - 根拠: 関数名のみ（行番号: 不明）。

- test_resolution_with_project_rules
  - 入力: 実環境の`.codanna`やSettingsに依存。
  - 処理:
    - Settingsロード→Providerキャッシュ再構築→Persistenceからルール読み込み→Enhancerで強化試行。
    - 失敗時はprintlnで情報出力。
  - 出力: 強化成功時のログ出力。
  - 根拠: 関数名のみ（行番号: 不明）。

（Mermaid図は条件分岐が4つ以上の複雑なフローがなく、このチャンクでは基準に該当しないため作成しません。）

## Complexity & Performance

- 計算量
  - `test_import_enhancement_with_aliases`: O(k)（テストケース数）/ 空間O(1)。
  - `test_module_path_computation`: O(k + n)（k=ケース数、n=文字列操作）/ 空間O(1)。
  - `test_typescript_behavior_add_import`: 計算は軽微だが、**ファイルI/O**と**cwd変更**が支配的。
  - `test_resolution_with_project_rules`: **キャッシュ再構築**・**永続ルールロード**でI/Oコスト。
- ボトルネック
  - `.codanna`ディレクトリ操作、`fs::write`/`fs::rename`/`fs::remove_dir_all`、`Settings::load`、`provider.rebuild_cache`。
- スケール限界
  - テストのため問題なし。大量のルール・巨大なプロジェクトではI/O時間が増加。
- 実運用負荷要因
  - ルール読み込み時のディスクI/O、キャッシュ作成時のCPU/ディスク使用。

## Edge Cases, Bugs, and Security

- エッジケース詳細化

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| エイリアス一致（深いパス） | "@/components/ui/dialog" | "./src/components/ui/dialog"に強化 | enhance_import_pathで対応 | OK |
| エイリアス一致（共通フォールバック） | "@/lib/api" | "./src/lib/api"に強化（"@/*"） | enhance_import_pathで対応 | OK |
| 相対パス | "./relative/path" | 強化しない（None） | enhance_import_pathがNone | OK |
| 親参照 | "../parent/path" | 強化しない（None） | enhance_import_pathがNone | OK |
| 外部パッケージ | "react" | 強化しない（None） | enhance_import_pathがNone | OK |
| ルール未ロード | "@/components/Button" | 未強化を許容（テストで二択） | behaviorテストで分岐アサート | OK（非決定的許容） |
| Settings未設定 | 設定ファイルなし | キャッシュ再構築しない/スキップ | printlnで通知してreturn | OK（情報のみ） |
| Windows/Unix差 | パス区切りの違い | ルールJSONのパス表現に依存 | display()使用 | 要注意 |

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし。このチャンクに`unsafe`はなく、標準APIのみ。
  - `unwrap`の使用: `FileId::new(1).unwrap()`は失敗時にpanic。テスト文脈では許容されるが、実運用コードでは避けるべき。
- インジェクション
  - SQL/Command/Path traversal: 外部入力を用いたコマンド実行はなし。`canonicalize`や固定パスで運用、Path traversalのリスクは限定的。
- 認証・認可
  - 該当なし。テストコード。
- 秘密情報
  - Hard-coded secrets: なし。テスト用JSONに秘密は含まれない。
  - Log leakage: `println!`のみ。機密情報の出力なし。
- 並行性
  - Race condition / Deadlock: `env::set_current_dir`とディレクトリ移動/削除は**プロセスグローバル**に影響。並列テスト時に**レース**が発生し得るため、直列実行やロックが望ましい。

Rust特有の観点（詳細チェックリスト）
- 所有権
  - `import.clone()`で明示的クローン（`test_typescript_behavior_add_import`）。借用ミスはない（行番号: 不明）。
- 借用
  - 可変借用は使用していない。ループ内の参照は短期借用で安全（行番号: 不明）。
- ライフタイム
  - 明示的ライフタイムは不要。`String`/`PathBuf`など所有型中心。
- unsafe境界
  - `unsafe`未使用（行番号: 不明）。
- Send/Sync
  - 並行実行を意図していない。グローバル環境変更のため、**並列不適**。
- データ競合
  - 共有状態は`cwd`とファイルシステム。テスト間で競合する可能性あり。
- await境界 / 非同期
  - 非同期処理なし。
- キャンセル
  - 非該当。
- エラー設計
  - `Result`は外部APIで受け取り、失敗時は`println!`で通知して早期returnすることあり（`test_resolution_with_project_rules`）。
  - `panic`（unwrap/expect）: `FileId::new(1).unwrap()`のみ。テストで妥当。

## Design & Architecture Suggestions

- グローバル副作用の削減
  - `env::set_current_dir`やディレクトリ移動は**テストを直列化**しない限り危険。`tempfile`クレートで**一時ディレクトリ**を作成し、**相対パスではなく絶対パス**指定で`.codanna`を探索するようにライブラリ側を設計変更すると安全。
- 依存の注入（DI）
  - `TypeScriptBehavior::new()`がcwdから`.codanna`を探すのではなく、**`ResolutionRules`や`ResolutionIndex`を明示的に注入**できるコンストラクタ/Builderを提供するとテストが**決定的**になり、副作用を避けられる。
- 設定ロードの抽象化
  - `Settings::load()`依存のテストは環境差で不安定。**モック可能なトレイト**（例: `SettingsProvider`）を導入し、テストでは**モック**を渡せる設計に。
- パス処理のユーティリティ化
  - `test_module_path_computation`のロジックは、ユーティリティ関数として本体側に実装し、テストで直接検証する形にすると**規約の一貫性**を保てる。

## Testing Strategy (Unit/Integration) with Examples

- 推奨ユニットテスト
  - 最長一致（most specific）のルールが選ばれることの検証。
  - ワイルドカード解決の境界（末尾の`*`に対する挙動）。
  - `baseUrl`あり/なし双方のケース。
  - OS差分（Windowsの`\\`）があっても一致する前提の正規化の検証。

- 推奨インテグレーションテスト
  - Settings→Provider→Persistenceのパイプラインを**完全にモック**し、ファイルI/Oを避けつつ挙動検証。
  - キャッシュ不整合時のフォールバック（例: キャッシュ破損ファイル）挙動。

- 追加テスト例（ユニット・決定的）
  ```rust
  #[test]
  fn test_most_specific_alias_wins() {
      // 重要部のみ
      let rules = ResolutionRules {
          base_url: None,
          paths: vec![
              ("@/components/*".to_string(), vec!["./src/components/*".to_string()]),
              ("@/components/ui/*".to_string(), vec!["./src/components/ui/*".to_string()]),
          ].into_iter().collect(),
      };
      let enhancer = TypeScriptProjectEnhancer::new(rules);
      let file_id = FileId::new(42).unwrap();

      let result = enhancer.enhance_import_path("@/components/ui/dialog", file_id);
      assert_eq!(result.as_deref(), Some("./src/components/ui/dialog"));
  }
  ```

  ```rust
  #[test]
  fn test_no_base_url_alias_resolution() {
      // baseUrlがNoneでもpathsのみで解決できることのテスト
      let rules = ResolutionRules {
          base_url: None,
          paths: vec![
              ("@/*".to_string(), vec!["./src/*".to_string()]),
          ].into_iter().collect(),
      };
      let enhancer = TypeScriptProjectEnhancer::new(rules);
      let file_id = FileId::new(7).unwrap();
      assert_eq!(
          enhancer.enhance_import_path("@/lib/api", file_id).as_deref(),
          Some("./src/lib/api")
      );
  }
  ```

## Refactoring Plan & Best Practices

- テストの決定性確保
  - `.codanna`依存のテストは**絶対パス**＋**tempdir**＋**RAIIガード**（Dropで自動復元）で安全に。
  - `serial_test`クレートで**直列実行**にする、もしくは**テストロック**を導入。
- ヘルパー関数の抽出
  - `.codanna`テスト環境の構築/クリーンアップを`setup_codanna_test_env()`/`teardown_codanna_test_env()`として共通化。
- Error handling
  - `unwrap`を**`expect("...")`**に置換し、失敗時の原因を明示。
  - I/O関数の戻り値をチェックし、失敗時に**詳細ログ**を出す。
- パスユーティリティ
  - `module_path_from_enhanced()`のような関数を本体に追加し、テストはその関数を検証。

## Observability (Logging, Metrics, Tracing)

- ログ
  - ルールロード時に**詳細ログ**（読み込んだファイル、ルール数、ハッシュ）を出力。
  - 強化の成功/失敗理由（どのパターンに一致したか、フォールバック発生）を**debugログ**に。
- メトリクス
  - 「適用されたエイリアス強化数」「未強化数」「ルールミスマッチ数」をカウンタで計測。
- トレーシング
  - `add_import`→`enhance_import_path`→`module_path変換`の各ステップに**span**を貼り、相関IDで関連付け。
- テスト時の観測
  - テスト専用の**in-memory logger**でログを捕捉し、期待ログもアサート。

## Risks & Unknowns

- 不明点
  - codanna内部の**解決アルゴリズム詳細**、**キャッシュフォーマット**、**優先順位ルール**はこのチャンクには現れない。
  - OS依存のパス正規化がどの層で行われるかは不明。
- リスク
  - **並列テスト**時にcwd変更やディレクトリ移動が他テストへ影響する可能性。
  - 実環境に`.codanna`が存在しない場合の**分岐**が環境に依存し、テストが**不安定**になり得る。
  - テストフィクスチャ`tests/fixtures/typescript_alias_test`の存在前提。CI環境差で**失敗**する可能性。