# plugins.mod.rs Review

## TL;DR

- 目的: GitベースのマーケットプレイスからClaude Codeプラグインを取得・検証・インストール/更新/削除・ロックファイル管理する仕組み
- 公開API: add_plugin, remove_plugin, update_plugin, list_plugins, verify_plugin, verify_all_plugins（各Result<(), PluginError>を返すCLI向け高水準操作）
- 中核ロジック: prepare_plugin（取得計画の構築）とexecute_install_with_plan（ファイルコピー・MCP統合・整合性計算・ロックへの反映・ロールバック）
- 重要な複雑点: 更新時のリモートコミット解決・インストール時の衝突検出・ロールバック整合性・.mcp.jsonとハッシュ整合性の扱い
- 重大リスク: manifestのパス処理（sanitize_manifest_path/add_single_path）により「../」や絶対パスが許容されうるため、プラグイン外のファイルに触れる可能性（パストラバーサル）⚠️
- 並行性: ロックファイルやワークスペースへの同時操作を制御する排他機構がないため、同時実行でレース条件が発生する可能性
- エラー設計: PluginError中心に包括的だが、expect/unwrapのパニック潜在箇所を一部含む（関数:行番号=不明）

## Overview & Purpose

本モジュールは、Claude Code向けプラグインのライフサイクル管理を司るコアです。具体的には以下を提供します。

- マーケットプレイスのGitリポジトリからプラグインのメタデータとソースを取得
- プラグインの構成ファイル（commands/agents/hooks/scripts）と追加ペイロードをワークスペースへ配置
- MCPサーバー設定（.mcp.json）への安全なマージと検証
- ロックファイル（PluginLockfile）にインストール情報を記録・検証
- CLIユースケースに適したdry-run/verbose/debugパラメータの取り扱い

設計上の特徴は、失敗時にロールバック可能なインストール処理、ファイル所有者の検出による衝突防止、整合性ハッシュ（integrity）検証、MCPサーバーキーの追跡です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| mod | error / fsops / lockfile / marketplace / merger / plugin / resolver | pub | 下位モジュールのAPI公開 | Low |
| Struct | WorkspacePaths | private | ワークスペース内の主要ディレクトリ/ファイルパス集約 | Low |
| Struct | PreparedPlugin | private | インストール/更新用に準備されたプラン（TempDirやmanifest等） | Med |
| Struct | ExistingPluginBackup | private | 既存プラグインのバックアップ（ロールバック用） | Med |
| fn | add_plugin | pub | マーケットプレイスから新規インストール | Med |
| fn | remove_plugin | pub | 既存プラグインのアンインストール | Low |
| fn | update_plugin | pub | 既存プラグインの更新（git ref考慮） | Med |
| fn | list_plugins | pub | インストール済み一覧の表示（JSON/verbose） | Low |
| fn | verify_plugin | pub | 指定プラグインの整合性検証 | Low |
| fn | verify_all_plugins | pub | 全プラグインの整合性検証 | Low |
| fn | prepare_plugin | private | マーケット/ソース解決・manifest読み込み・衝突検出 | High |
| fn | execute_install_with_plan | private | コピー・MCPマージ・integrity計算・ロック反映・ロールバック | High |
| fn | rollback_install | private | 失敗時にファイル/MCP/ロックの復旧 | Med |
| fn | verify_entry | private | integrity（ファイルハッシュ）とMCPキー存在チェック | Med |
| fn | uninstall_plugin | private | ファイル削除・MCPキー削除・ロック更新 | Med |
| fn | collect_component_files | private | plugin manifestに基づく構成ファイル列挙 | Med |
| fn | check_file_conflicts | private | 既存ファイルとの所有権衝突検知 | Med |

### Dependencies & Interactions

- 内部依存
  - add_plugin/update_plugin → prepare_plugin → check_file_conflicts/load_plugin_mcp → execute_install_with_plan → rollback_install/save_lockfile
  - verify_plugin/verify_all_plugins → verify_entry
  - remove_plugin → uninstall_plugin → save_lockfile
- 外部依存（表）

| モジュール/クレート | 主な使用 | 備考 |
|--------------------|----------|------|
| chrono::Utc | RFC3339 timestamp | インストール/更新日時 |
| tempfile::{TempDir,tempdir} | 一時ディレクトリ | clone/extract先 |
| walkdir::WalkDir | ディレクトリ走査 | ファイル列挙 |
| serde_json::Value | .mcp.jsonハンドリング | MCPサーバー設定 |
| fsops | calculate_dest_path, calculate_integrity, copy_plugin_files, copy_plugin_payload, remove_plugin_files | ファイルI/O＋ハッシュ |
| lockfile | PluginLockfile, PluginLockEntry, LockfilePluginSource | ロック読み書き |
| marketplace | MarketplaceManifest, ResolvedPluginSource | マーケットメタデータ |
| resolver | clone_repository, extract_subdirectory, resolve_reference | Git操作 |
| merger | MCPサーバーマージ/削除/競合チェック | .mcp.json扱い |
| plugin | PluginManifest, PathSpec, HookSpec | プラグインmanifestモデル |
| std::fs/io/path/env | 基本I/O | — |

- 被依存推定
  - CLIコマンド層（codanna plugin ...）から直接利用
  - 設定管理(Settings)を保持するアプリケーションルートから呼び出し

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| add_plugin | fn add_plugin(settings: &Settings, marketplace_url: &str, plugin_name: &str, git_ref: Option<&str>, force: bool, dry_run: bool) -> Result<(), PluginError> | マーケットからプラグインをインストール | O(G + N) | O(N) |
| remove_plugin | fn remove_plugin(settings: &Settings, plugin_name: &str, force: bool, dry_run: bool) -> Result<(), PluginError> | プラグイン削除 | O(N) | O(1) |
| update_plugin | fn update_plugin(settings: &Settings, plugin_name: &str, git_ref: Option<&str>, force: bool, dry_run: bool) -> Result<(), PluginError> | 既存プラグインの更新 | O(G + N) | O(N) |
| list_plugins | fn list_plugins(settings: &Settings, verbose: bool, json: bool) -> Result<(), PluginError> | インストール済一覧 | O(P) | O(P) |
| verify_plugin | fn verify_plugin(settings: &Settings, plugin_name: &str, verbose: bool) -> Result<(), PluginError> | 個別整合性検証 | O(N) | O(1) |
| verify_all_plugins | fn verify_all_plugins(settings: &Settings, verbose: bool) -> Result<(), PluginError> | 一括整合性検証 | O(ΣN) | O(1) |

- 記号説明
  - G: Git clone/resolveのコスト（ネットワーク/IO）
  - N: 該当プラグインのファイル数
  - P: インストール済みプラグイン数
  - ΣN: 全プラグインのファイル総数

以下、各APIの詳細。

### add_plugin

1. 目的と責務
   - 指定マーケットURLとプラグイン名をもとに、必要なGit参照（git_ref）を解決し、プラグインをワークスペースへインストールする。既存インストールがある場合の上書きはforceで制御。dry_runで計画のみ出力。

2. アルゴリズム（簡略ステップ）
   - Settingsからワークスペースルートを取得
   - ロックファイルをロードし、既存エントリ有無とforceを判定
   - prepare_pluginで取得・衝突検出・manifest処理・MCP準備
   - dry_runならサマリー出力して終了
   - ワークスペースディレクトリ構成をensure
   - execute_install_with_planでコピー、MCPマージ、integrity計算、ロック更新、失敗時ロールバック
   - 成功メッセージ出力

3. 引数

| 引数 | 型 | 意味 |
|------|----|------|
| settings | &Settings | ワークスペース設定（workspace_root, debug） |
| marketplace_url | &str | マーケットリポジトリURL |
| plugin_name | &str | プラグイン名 |
| git_ref | Option<&str> | 取得リファレンス（タグ/ブランチ/commit） |
| force | bool | 上書き・衝突無視の強制フラグ |
| dry_run | bool | 実行せず計画を表示 |

4. 戻り値

| 戻り値 | 意味 |
|--------|------|
| Result<(), PluginError> | 成否。失敗時はPluginError詳細 |

5. 使用例

```rust
use crate::{Settings, plugins::add_plugin};

let settings = Settings { workspace_root: None, debug: true /* 他フィールド不明 */ };
add_plugin(
    &settings,
    "https://github.com/example/marketplace.git",
    "cool-plugin",
    Some("v1.2.3"),
    false,
    false,
)?;
```

6. エッジケース
- 既にインストール済みでforce=false → PluginError::AlreadyInstalled
- manifestがstrict要求だが存在しない → InvalidPluginManifest
- ファイル所有権の衝突（他プラグイン/未知）→ FileConflict（forceで上書き可）
- MCPサーバー競合 → merger側のエラー（forceで許容可）
- ネットワーク/クローン失敗 → resolver由来のエラー

（根拠: add_plugin/prepare_plugin/execute_install_with_plan 関数。行番号=不明）

### remove_plugin

1. 目的と責務
   - 指定プラグインのファイルとMCPキーを削除し、ロックファイルからエントリを除去。

2. アルゴリズム
   - ワークスペースルート→ロックファイルロード→エントリ存在確認
   - uninstall_pluginで関連ファイル削除、MCPキー削除、ロック更新
   - save_lockfileで反映

3. 引数/戻り値（略。同上）

5. 使用例

```rust
use crate::plugins::remove_plugin;

remove_plugin(&settings, "cool-plugin", false, false)?;
```

6. エッジケース
- 未インストール → PluginError::NotInstalled
- ファイル削除権限なし → fsops::remove_plugin_files由来のエラー

（根拠: remove_plugin/uninstall_plugin。行番号=不明）

### update_plugin

1. 目的と責務
   - 既存プラグインを更新。git_ref指定またはリモートHEADからコミット解決。既に最新かつ整合性OKなら再インストール不要。

2. アルゴリズム
   - ロックファイルで既存エントリ取得
   - force=falseのときresolve_remote_commitで最新コミット確認
   - 既存commitと同じならverify_entryで整合性検証→OKなら終了
   - prepare_plugin→dry_run判定→ensureレイアウト→execute_install_with_plan
   - 成功メッセージ出力

3. 引数/戻り値（略）

5. 使用例

```rust
use crate::plugins::update_plugin;

update_plugin(&settings, "cool-plugin", Some("main"), false, false)?;
```

6. エッジケース
- NotInstalled → PluginError::NotInstalled
- リモートコミット解決失敗 → resolver由来のNone/Err
- verify_entry失敗（整合性崩れ）→ 再インストールを試行

（根拠: update_plugin/resolve_remote_commit/verify_entry。行番号=不明）

### list_plugins

- 目的: ロックファイルからプラグイン一覧を表示。json=trueでJSON出力。
- 例

```rust
use crate::plugins::list_plugins;

list_plugins(&settings, true, false)?;
list_plugins(&settings, false, true)?; // JSON出力
```

（根拠: list_plugins。行番号=不明）

### verify_plugin / verify_all_plugins

- 目的: ロックファイルのintegrityとMCPキー存在を検証（.mcp.json自体は除外）
- 例

```rust
use crate::plugins::{verify_plugin, verify_all_plugins};

verify_plugin(&settings, "cool-plugin", true)?;
verify_all_plugins(&settings, false)?;
```

（根拠: verify_plugin/verify_all_plugins/verify_entry。行番号=不明）

### Data Contracts（Lock/Manifestの主要フィールド）

- PluginLockEntry（このチャンクから読み取れるフィールド）
  - name: String
  - version: String
  - commit: String
  - marketplace_url: String
  - installed_at: String（RFC3339）
  - updated_at: String（RFC3339）
  - integrity: String（ハッシュ）
  - files: Vec<String>（.mcp.jsonは除外）
  - mcp_keys: Vec<String>（追加されたMCPサーバキー）
  - source: Option<LockfilePluginSource>（MarketplacePath|Git）

- PluginManifest/MarketplaceManifestの詳細形状はこのチャンクには現れない（不明）

## Walkthrough & Data Flow

インストールと更新は多岐にわたる分岐と失敗時ロールバックを含むため、主要フローを図示します。

```mermaid
flowchart TD
  A[add_plugin] --> B[resolve_workspace_root]
  B --> C[load_lockfile]
  C --> D{既存エントリ?}
  D -- Yes & !force --> E[Err(AlreadyInstalled)]
  D -- No or force --> F[prepare_plugin]
  F --> G{dry_run?}
  G -- Yes --> H[print_dry_run_summary & End]
  G -- No --> I[ensure_workspace_layout]
  I --> J[execute_install_with_plan]
  J --> K[save_lockfile]
  K --> L[Ok(Installed)]
  J -- Err --> R[rollback_install]
  R --> S[restore/cleanup & End]
```

上記の図はadd_plugin関数およびexecute_install_with_planの主要分岐を示す（コード行範囲: 不明）。

```mermaid
flowchart TD
  U[update_plugin] --> A1[resolve_workspace_root]
  A1 --> A2[load_lockfile]
  A2 --> A3{既存?}
  A3 -- No --> A4[Err(NotInstalled)]
  A3 -- Yes --> B1{force?}
  B1 -- No --> B2[resolve_remote_commit]
  B2 --> B3{同一commit?}
  B3 -- Yes --> B4[verify_entry]
  B4 -- Ok --> B5[Up-to-date & End]
  B4 -- Err --> C1[prepare_plugin]
  B3 -- No --> C1[prepare_plugin]
  B1 -- Yes --> C1[prepare_plugin]
  C1 --> C2{dry_run?}
  C2 -- Yes --> C3[print_dry_run_summary & End]
  C2 -- No --> C4[ensure_workspace_layout]
  C4 --> C5[execute_install_with_plan]
  C5 --> C6[save_lockfile]
  C6 --> C7[Ok(Updated)]
  C5 -- Err --> C8[rollback_install]
  C8 --> C9[restore/cleanup & End]
```

上記の図はupdate_plugin関数の主要分岐を示す（コード行範囲: 不明）。

データフローの要点:
- prepare_plugin
  - MarketplaceManifestからプラグイン定義を取得
  - ResolvedPluginSourceがMarketplacePathならextract_subdirectory、Gitなら別リポジトリをclone
  - PluginManifestを読み込み（strict時必須）、構成ファイル列挙
  - ファイル所有者をlockfileから照合し、衝突検出
  - MCP設定を読み込み、既存との競合チェック
- execute_install_with_plan
  - 既存エントリがある場合はバックアップ＋uninstall
  - copy_plugin_files（構成要素）→ copy_plugin_payload（追加payload）→ merge_mcp_servers（MCP）
  - normalize_paths→calculate_integrity→PluginLockEntry作成→ロックに追加→save
  - いずれかの段階で失敗時はrollback_installで復旧

## Complexity & Performance

- 時間計算量
  - add_plugin/update_plugin: O(G + N)（GはGit clone/参照解決、Nはファイル列挙・コピー・ハッシュ）
  - remove_plugin: O(N)（削除対象ファイル数に比例）
  - list_plugins: O(P)
  - verify_plugin: O(N)（ハッシュ計算とMCPキー確認）
  - verify_all_plugins: O(ΣN)
- 空間計算量
  - インストール/更新はコピー中のファイルバッファと一時ディレクトリ分のO(N)
- ボトルネック
  - ネットワークIO（clone_repository/resolve_reference）
  - ディスクIO（WalkDir・fs::read/fs::write・integrity計算）
- スケール限界
  - 大規模プラグイン（多数ファイル）でハッシュ計算とコピーが遅延
  - 多数プラグイン同時操作を想定した排他制御がないため、競合時に失敗や整合性崩れの可能性

## Edge Cases, Bugs, and Security

セキュリティチェックリストに基づいて分析。

- メモリ安全性
  - unsafe不使用。標準APIの利用のみ。所有権・借用はRustの規約に従って安全（関数:広範、行番号=不明）。
- インジェクション
  - Path traversal（重大）⚠️
    - sanitize_manifest_pathは"./"除去のみで"../"や絶対パスを拒否しない。
    - add_single_pathでplugin_root.join(sanitized)が絶対/上位ディレクトリの場合、collect_files_for_pathはbaseを越えたファイルパスを「相対文字列」として収集しうる。
    - 結果的にcopy_plugin_filesでplugin_dir.join(relative)が「絶対パスをjoin」すると、親を無視して当該絶対パスをソースにし、プラグイン外の任意ファイルをコピーする危険がある可能性（fsopsの実装次第だが一般にPath::joinは絶対パスを優先）（関数: add_single_path/collect_files_for_path、行番号=不明）。
    - 対策案: 
      - sanitize_manifest_pathで絶対パスと「..」コンポーネントを拒否
      - canonicalize後にplugin_rootをprefixとしてstrip_prefixに成功することを必須化。失敗時はInvalidPluginManifest
      - copy側でもsrcがplugin_dir配下であることを検証
  - Command/SQLインジェクション: 該当なし
- 認証・認可
  - 権限チェックはOSファイル権限に依存。アプリ層の認可はなし（CLI前提）。
- 秘密情報
  - Hard-coded secrets: 該当なし
  - Log leakage: debug時にファイルリストをeprintlnするが秘密情報の出力は限定的。安全側に配慮を。
- 並行性
  - ロックファイルやワークスペースへの同時変更に対する排他制御がない。複数プロセス/スレッドが同時にadd/update/remove/verifyを実行すると、レース条件や整合性崩れ（partial writes, integrity mismatch）が起こりうる。
  - 対策案: ワークスペースレベルのファイルロック（flock/Advisory lock）/pidファイル、トランザクション的なテンポラリ→原子的rename
- panicリスク
  - ensure_workspace_layoutでpaths.lockfile_path.parent().unwrap(): parentが必ず存在する設計だが理論上Noneでpanicの可能性（低）
  - check_file_conflictsでstrip_prefix(plugin_dir).expect(...): WalkDirはplugin_dir以下だが、シンボリックリンクやOSの特殊ファイルで仮定が崩れる可能性は低だがゼロではない
  - 対策案: 期待が外れたらPluginErrorで返す防御的実装に変更

詳細なエッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 既存インストール（force=false） | add_plugin("x") | Err(AlreadyInstalled) | 既存チェックあり | OK |
| .mcp.jsonの整合性 | verify_plugin("x") | MCPキー存在確認 | mcp_keys検証あり | OK |
| manifest不在（strict=true） | prepare_plugin | Err(InvalidPluginManifest) | チェックあり | OK |
| ファイル所有者衝突 | 既存他プラグインの同パス | Err(FileConflict) or forceで上書き | check_file_conflicts | OK |
| sanitizeで「..」 | add_single_path("../..") | Err(InvalidPluginManifest) | 許容され得る | NG ⚠️ |
| 絶対パス参照 | add_single_path("/etc/passwd") | Err(InvalidPluginManifest) | 許容され得る | NG ⚠️ |
| lockfile保存失敗 | ディスク満杯 | installをロールバック | rollback_install | OK |
| dry_run | dry_run=true | ファイル未変更/サマリ表示 | print_dry_run_summary | OK |
| 同時更新 | 2プロセスがupdate | 排他で順序保証 | 排他なし | NG ⚠️ |

（根拠: 各関数のコードロジックから推定。行番号=不明）

## Design & Architecture Suggestions

- パス安全性強化（最重要）🔥
  - sanitize_manifest_pathで以下を拒否:
    - 絶対パス（Path::is_absolute）
    - 「..」を含むコンポーネント（Path::componentsでNormal/ParentDirチェック）
  - add_single_pathでcanonicalizeし、plugin_rootの配下判定（path.starts_with(plugin_root)）を厳格に
  - copy_plugin_files/copy_plugin_payload側でも「srcがplugin_dir配下」を検証する二重フェンス
- 排他制御
  - ワークスペース操作時にロックファイル（例: .codanna/plugins/.lock）を作成して排他
  - save_lockfileは一時ファイル→原子rename（fsync含む）でクラッシュ耐性を強化
- エラー処理の一貫性
  - expect/unwrapは廃し、PluginErrorへ変換
  - IOエラーやwalkdirエラーは文脈（操作種別/対象パス）を付与
- ディレクトリ/ファイル操作の抽象化
  - calculate_dest_pathの仕様を明確化（絶対パス入力防止）
  - WorkspacePathsの生成時に前提検証（parentがNoneにならない）
- 検証/署名
  - integrity計算のアルゴリズム・バージョンをLockfileに記録（将来の互換性）
  - MCPのマージ前後差分をログ記録（監査性）

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト
  - sanitize_manifest_pathとadd_single_path
    - "../x"や"/abs/x"を与えるとErr（修正後の期待）になることを確認
  - collect_files_for_path
    - base配下・非配下のパスでstrip_prefixの動作確認
  - check_file_conflicts
    - lockfileに別プラグイン所有のファイルを設定し、force=falseでErr
  - verify_entry
    - .mcp.jsonに必要キーがないケースでErr
  - normalize_paths
    - Windows/Unixパスで正規化されること

- 統合テスト
  - add_plugin（dry_run）
    - 実ファイル作成なし、サマリの件数が期待通り
  - add_plugin→verify_plugin
    - ファイルの整合性ハッシュ一致、MCPキー存在
  - update_plugin（同commit）
    - verify_entry成功なら再インストールされない
  - 失敗時ロールバック
    - copy途中で意図的エラーを発生させ、既存状態が復元される

- 例（パス安全ユニットテスト・修正案を想定）

```rust
#[test]
fn add_single_path_rejects_parent_dir() {
    use std::path::PathBuf;
    let plugin_root = PathBuf::from("/repo/plugin");
    let mut files = std::collections::HashSet::new();
    let res = super::add_single_path(&plugin_root, "../outside", &mut files);
    assert!(res.is_err(), "should reject paths escaping plugin_root");
}
```

- 例（インストール統合テストのスケルトン）

```rust
#[test]
fn install_and_verify_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
    use crate::{plugins::add_plugin, plugins::verify_plugin, Settings};
    let tmp = tempfile::tempdir()?;
    let settings = Settings { workspace_root: Some(tmp.path().to_path_buf()), debug: false /* 他不明 */ };
    add_plugin(&settings, "https://github.com/example/marketplace.git", "cool-plugin", Some("main"), false, false)?;
    verify_plugin(&settings, "cool-plugin", true)?;
    Ok(())
}
```

## Refactoring Plan & Best Practices

- 入力検証の強化
  - plugin_name, marketplace_url, git_refの検証（空/不正文字列の拒否）
- エラーハンドリング標準化
  - map_errを使い、IO/WalkDirエラーに操作コンテキスト（"copy payload", 対象パス）を付与
- パス型の導入
  - 新しい型で「plugin_root配下保証」を静的に表現するラッパーを用意し、join/strip_prefixの誤用を防止
- トランザクション整備
  - コピー→integrity→ロック更新→MCPマージの順序を再検討し、最終コミット/保存をAtomicに
- 依存関係/グラフ
  - remove_plugin(force)が現状未使用。将来的な依存グラフを導入し、利用中のプラグインを安全にアンインストールできるようにする

## Observability (Logging, Metrics, Tracing)

- ログ
  - 現状、println/eprintlnベース。構造化ログ（level, event, plugin, path, counts）へ移行推奨
  - debugフラグ拡張（コピー対象件数、MCP差分、ロックファイル更新結果）
- メトリクス
  - 処理時間（clone/コピー/ハッシュ）、ファイル数、失敗率、ロールバック発生数
- トレーシング
  - インストールID（UUID）を生成し、処理フェーズのspanを付与すると障害解析が容易に

## Risks & Unknowns

- fsops/calculate_dest_path/copy_plugin_files/copy_plugin_payloadの詳細（絶対パスjoin時の挙動・安全性）はこのチャンクには現れない（不明）。ここがパストラバーサル耐性の鍵。
- merger（MCP）統合の競合解決ポリシー詳細は不明
- resolver::resolve_referenceの解決戦略（マーケットURLに対してHEADで何を返すか）が不明
- Windows/Unix間のパス差異はreplace('\\', '/')である程度吸収しているが、canonicalizeや絶対パス判定の差異は検討が必要
- remove_plugin時の「依存関係を無視」仕様（TODO記載あり）により、将来の依存グラフ導入時に挙動が変わる可能性

以上の観点から、最優先は「パス検証の強化」と「排他制御の導入」です。これにより、重大なセキュリティリスクと同時実行時の整合性問題を緩和できます。