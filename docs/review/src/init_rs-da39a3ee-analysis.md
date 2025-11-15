# init.rs Review

## TL;DR

- 本モジュールの目的は、Codanna の**グローバル初期化**（ディレクトリ作成）、**FastEmbed キャッシュのシンボリックリンク**整備、**プロジェクトレジストリの管理**、および**インデックスパス解決**を提供すること。
- 主要公開APIは、global_dir/models_dir/projects_file、init_global_dirs/init_profile_infrastructure、create_fastembed_symlink、ProjectRegistry::{load,save,register_or_update_project}、resolve_index_path。
- 複雑箇所は、条件分岐が多い create_fastembed_symlink と resolve_index_path の分岐ロジック。
- 重大リスクは、(1) global_dir がホームディレクトリ取得失敗時に panic、(2) プロジェクトレジストリ load エラーを register_* が飲み込むためデータ欠落の恐れ、(3) projects.json の**同時更新**に対する排他/アトミック性が不足しファイル破損/上書きロスの恐れ、(4) 異なる関数で**エラー型が混在**（io::Error と IndexError）。
- Rust安全性: unsafe なし、OnceLock によるスレッド安全な初期化。I/O・シンボリックリンク操作はOS依存の失敗がありうるため、適切なエラー伝播・ログが重要。
- セキュリティ: 直接的なインジェクションの懸念は低いが、ログや JSON に**絶対パス**が含まれる点に留意（情報漏洩の可能性）。

## Overview & Purpose

本ファイルは Codanna のグローバル初期化モジュールであり、以下を担います。
- ユーザホーム配下にある**グローバルディレクトリ**（~/.codanna（テスト時は ~/.codanna-test））と、その配下の**models**ディレクトリを作成・利用。
- **プロジェクトレジストリ（projects.json）**のシリアライズ/デシリアライズ・登録/更新。
- FastEmbed のキャッシュディレクトリ名（.fastembed_cache）と**グローバル models への symlink を管理**（後方互換・クリーンアップ目的）。
- 設定からの**インデックスパス解決**（--config 指定時の相対パスの扱い等）。

対象は CLI/ライブラリ双方。テスト条件ではディレクトリ名が切り替わるよう設計されています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | global_dir | pub | グローバルディレクトリの取得（OnceLockキャッシュ） | Low |
| Function | models_dir | pub | グローバル models ディレクトリの取得 | Low |
| Function | projects_file | pub | レジストリファイルパスの取得 | Low |
| Function | local_dir_name | pub | ローカル設定ディレクトリ名の返却 | Low |
| Function | fastembed_cache_name | pub | FastEmbed キャッシュディレクトリ名の返却 | Low |
| Function | init_global_dirs | pub | グローバル・models ディレクトリの作成、プロファイル初期化 | Low |
| Function | init_profile_infrastructure | pub | providers.json（バージョン1、空）を生成 | Low |
| Function | create_fastembed_symlink | pub | .fastembed_cache → グローバル models への symlink 作成 | Med |
| Struct | ProjectId | pub | プロジェクトID（SHA-256で生成）、表示/デフォルト実装 | Low |
| Struct | ProjectInfo | pub | プロジェクトメタ（パス、名前、件数等） | Low |
| Struct | ProjectRegistry | pub | レジストリのロード/セーブ/登録/更新/検索 | Med |
| Function | resolve_index_path | pub | 設定と --config に基づくインデックスパスの解決 | Med |

### Dependencies & Interactions

- 内部依存
  - models_dir → global_dir
  - projects_file → global_dir
  - init_global_dirs → global_dir, models_dir, init_profile_infrastructure
  - create_fastembed_symlink → fastembed_cache_name, models_dir
  - ProjectRegistry::{register_project, register_or_update_project} → Self::load, Self::save, create_project_info, find_project_by_path
  - resolve_index_path → local_dir_name

- 外部依存（クレート/モジュール）

| 依存 | 用途 |
|------|------|
| serde, serde_json | JSONシリアライズ/デシリアライズ（レジストリ、providers.json） |
| rand | ProjectId の乱数生成 |
| sha2 | ProjectId ID生成用ハッシュ |
| dirs | ホームディレクトリ取得 |
| std::fs, std::path, std::os::{unix,windows}::fs | ファイル/パス/シンボリックリンク操作 |
| std::sync::OnceLock | グローバルディレクトリのキャッシュ |

- 被依存推定
  - CLIの初期化コマンド（codanna init）や、起動時の環境セットアップ
  - インデックス作成・検索機能が、プロジェクトIDやプロジェクトメタへアクセス
  - FastEmbed 利用箇所（旧バージョン互換の symlink 前提構成）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| global_dir | fn global_dir() -> PathBuf | グローバルディレクトリパスの取得 | O(1) | O(1) |
| models_dir | fn models_dir() -> PathBuf | models ディレクトリパスの取得 | O(1) | O(1) |
| projects_file | fn projects_file() -> PathBuf | projects.json パスの取得 | O(1) | O(1) |
| local_dir_name | fn local_dir_name() -> &'static str | ローカル設定ディレクトリ名の取得 | O(1) | O(1) |
| fastembed_cache_name | fn fastembed_cache_name() -> &'static str | FastEmbed キャッシュ名の取得 | O(1) | O(1) |
| init_global_dirs | fn init_global_dirs() -> Result<(), io::Error> | グローバル/モデルディレクトリ作成と providers.json 初期化 | ほぼO(1) | O(1) |
| init_profile_infrastructure | fn init_profile_infrastructure() -> Result<(), io::Error> | providers.json（空）を作成 | O(1) | O(1) |
| create_fastembed_symlink | fn create_fastembed_symlink() -> Result<(), io::Error> | .fastembed_cache の symlink を作成 | O(1) | O(1) |
| ProjectId::new | fn new() -> Self | 新規ランダムID生成（SHA-256） | O(1) | O(1) |
| ProjectId::from_string | fn from_string(String) -> Self | 既存文字列からID生成 | O(1) | O(1) |
| ProjectId::as_str | fn as_str(&self) -> &str | ID文字列参照取得 | O(1) | O(1) |
| ProjectRegistry::new | fn new() -> Self | 空レジストリ生成 | O(1) | O(1) |
| ProjectRegistry::load | fn load() -> Result<Self, IndexError> | ディスクからレジストリ読込 | O(n) n=ファイル長 | O(n) |
| ProjectRegistry::save | fn save(&self) -> Result<(), IndexError> | レジストリ保存 | O(n) | O(n) |
| ProjectRegistry::register_project | fn register_project(&Path) -> Result<String, IndexError> | 新規登録しID返却 | O(n) | O(n) |
| ProjectRegistry::register_or_update_project | fn register_or_update_project(&Path) -> Result<String, IndexError> | 既存検出で更新、なければ登録 | O(n) | O(n) |
| ProjectRegistry::find_project_by_id | fn find_project_by_id(&self, &str) -> Option<&ProjectInfo> | ID検索 | O(1)期待 | O(1) |
| ProjectRegistry::find_project_by_id_mut | fn find_project_by_id_mut(&mut self, &str) -> Option<&mut ProjectInfo> | ID検索（可変） | O(1)期待 | O(1) |
| ProjectRegistry::update_project_path | fn update_project_path(&mut self, &str, &Path) -> Result<(), IndexError> | プロジェクト移動時のパス更新 | O(1) | O(1) |
| resolve_index_path | fn resolve_index_path(&Settings, Option<&Path>) -> PathBuf | インデックスパス解決 | O(1) | O(1) |

データ契約（JSONフォーマット）
- providers.json（init_profile_infrastructure が生成）
  - { "version": 1, "providers": {} }
- projects.json（ProjectRegistry::save が生成）
  - { "version": 1, "projects": { "<id>": ProjectInfo }, "default_project": "<id>"? }
- ProjectInfo
  - path: PathBuf（Serde経由で文字列化。非UTF-8は環境依存）
  - name: String
  - symbol_count: u32
  - file_count: u32
  - last_modified: u64
  - doc_count: u64

以下、主な API の詳細。

1) global_dir
- 目的と責務: ユーザホーム配下のグローバルディレクトリ（テスト時は .codanna-test、非テストは .codanna）を返却。OnceLock で初回のみ解決。
- アルゴリズム:
  1. dirs::home_dir() を取得（失敗時 expect で panic）
  2. 返値に GLOBAL_DIR_NAME を join
  3. OnceLock に保存して再利用
- 引数: なし
- 戻り値: PathBuf（グローバルディレクトリ）
- 使用例:
  ```rust
  let dir = codanna::init::global_dir();
  println!("{}", dir.display());
  ```
- エッジケース:
  - ホームディレクトリが取得できない: 現実装は panic

2) models_dir
- 目的: グローバル配下 models ディレクトリパスを返す
- アルゴリズム: global_dir().join("models")
- 使用例:
  ```rust
  let models = codanna::init::models_dir();
  ```
- エッジケース: 特になし

3) projects_file
- 目的: グローバル配下 projects.json パスを返す
- アルゴリズム: global_dir().join("projects.json")
- 使用例:
  ```rust
  let p = codanna::init::projects_file();
  ```
- エッジケース: 特になし

4) local_dir_name / fastembed_cache_name
- 目的: ディレクトリ名/キャッシュ名の定数返却
- 使用例:
  ```rust
  assert_eq!(codanna::init::local_dir_name(), ".codanna");
  assert_eq!(codanna::init::fastembed_cache_name(), ".fastembed_cache");
  ```
- エッジケース: なし

5) init_global_dirs
- 目的: グローバル、models ディレクトリ作成と providers.json 初期化
- アルゴリズム:
  1. global_dir と models_dir を計算
  2. 不存在なら作成（存在ならその旨表示）
  3. init_profile_infrastructure を呼ぶ
- 引数: なし
- 戻り値: Result<(), io::Error>
- 使用例:
  ```rust
  codanna::init::init_global_dirs()?;
  ```
- エッジケース:
  - パーミッション不足/存在しない親ディレクトリ: io::Error

6) init_profile_infrastructure
- 目的: providers.json が存在しなければ空スキーマを生成
- アルゴリズム:
  1. ~/.codanna/providers.json を構築
  2. 不存在なら {version:1, providers:{}} を pretty JSON で書き込み
- 戻り値: Result<(), io::Error>
- 使用例:
  ```rust
  codanna::init::init_profile_infrastructure()?;
  ```
- エッジケース:
  - シリアライズ失敗（ほぼ起きない）時に io::Error::other で包む

7) create_fastembed_symlink
- 目的: .fastembed_cache → グローバル models へのシンボリックリンクを作成（後方互換）
- アルゴリズム（主要分岐は下図参照）:
  - .fastembed_cache が存在
    - シンボリックリンクなら
      - 参照先がグローバル models と一致
        - 既定モデル（models--Qdrant--all-MiniLM-L6-v2-onnx）が存在なら OK
        - 無ければ「初回でDL」と案内して OK
      - 不一致ならリンク削除
    - シンボリックでないなら警告出して何もしない
  - シンボリックリンクを作成（Unix: symlink、Windows: symlink_dir）
- 戻り値: Result<(), io::Error>
- 使用例:
  ```rust
  codanna::init::create_fastembed_symlink()?;
  ```
- エッジケース:
  - Windows で開発者モード/権限不足により symlink 失敗
  - 既存の通常ディレクトリが .fastembed_cache にある場合は非破壊（そのまま）

8) ProjectId::{new, from_string, as_str}
- 目的:
  - new: 16バイト乱数を SHA-256 でハッシュし、先頭32 hex桁（128bit）をIDとして採用
  - from_string: 既存文字列からID生成
  - as_str: 文字列参照を返す
- 使用例:
  ```rust
  let id = ProjectId::new();
  println!("ID={}", id);
  ```
- エッジケース: なし（rand 失敗は通常なし）

9) ProjectInfo
- 目的: プロジェクトのメタ情報保持（path, name, symbol_count 等）
- 直接APIはないが、JSON のスキーマ契約の一部。path は canonicalize 結果または元パス。

10) ProjectRegistry::{new, load, save, register_project, register_or_update_project, find_project_by_id(_mut), update_project_path}
- 目的と責務:
  - new: 空のレジストリ
  - load: projects.json を読み込み（無ければ空）
  - save: レジストリを書き出し（親ディレクトリ作成含む）
  - register_project: 新規 ID で登録し保存
  - register_or_update_project: パス一致の既存があれば更新、無ければ新規登録
  - find_project_by_id(_mut): ID 検索
  - update_project_path: 移動後のパス/名前更新し保存
- アルゴリズム要点:
  - register_* は load の失敗を new にフォールバックしてから保存（エラーデータを消す恐れ）
  - パス比較は canonicalize ベース（失敗時のフォールバックあり）
- 使用例:
  ```rust
  use std::path::Path;
  let id = ProjectRegistry::register_or_update_project(Path::new("/path/to/project"))?;
  let reg = ProjectRegistry::load()?;
  if let Some(info) = reg.find_project_by_id(&id) {
      println!("project {} at {}", info.name, info.path.display());
  }
  ```
- エッジケース:
  - projects.json が壊れている → load は IndexError を返すが、register_* は黙って空で上書きする可能性
  - update_project_path は new_path を canonicalize しない

11) resolve_index_path
- 目的: --config 指定や workspace_root を考慮し、index_path を最終的な絶対/相対パスに解決
- アルゴリズム（下図参照）:
  1. settings.index_path が絶対なら即返却
  2. config_path があれば、その親に対して相対解決。ただし親が local_dir_name（.codanna 等）なら一段上（ワークスペースルート）を基準に解決
  3. settings.workspace_root が Some ならそこを基準
  4. それ以外は index_path をそのまま返却
- 使用例:
  ```rust
  let resolved = resolve_index_path(&settings, Some(Path::new("/ws/.codanna/settings.toml")));
  ```
- エッジケース:
  - config_path に親がない（普通は起きない） → workspace_root/fallback に流れる

図（Mermaid）

create_fastembed_symlink の分岐（行番号: 不明。本チャンクに含まれる関数の主要分岐）
```mermaid
flowchart TD
  A[Start create_fastembed_symlink] --> B{.fastembed_cache exists?}
  B -- No --> K[Create symlink (OS-specific)] --> Z[Ok]
  B -- Yes --> C{is_symlink?}
  C -- No --> D[Warn & Do nothing] --> Z
  C -- Yes --> E[read_link] --> F{target == global_models?}
  F -- No --> G[remove_file (symlink)]
  G --> K
  F -- Yes --> H{default model exists?}
  H -- Yes --> I[Print verified] --> Z
  H -- No --> J[Inform model will download] --> Z
  K --> L{Unix?}
  L -- Yes --> M[unix::fs::symlink]
  L -- No --> N[windows::fs::symlink_dir]
  M --> Z
  N --> Z
```

resolve_index_path の分岐（行番号: 不明。本チャンクに含まれる関数の主要分岐）
```mermaid
flowchart TD
  A[Start resolve_index_path] --> B{index_path is absolute?}
  B -- Yes --> Z[Return index_path]
  B -- No --> C{config_path provided?}
  C -- No --> E{workspace_root set?}
  C -- Yes --> D{parent endswith local_dir_name?}
  D -- Yes --> D1[parent.parent() join index_path] --> Z
  D -- No --> D2[parent join index_path] --> Z
  E -- Yes --> F[workspace_root join index_path] --> Z
  E -- No --> G[Return index_path as-is] --> Z
```

## Walkthrough & Data Flow

- 初期化フェーズ（CLI起動時や `codanna init`）
  1. init_global_dirs → global_dir と models_dir を確認・作成
  2. init_profile_infrastructure → providers.json を生成（存在しなければ）
  3. 旧 FastEmbed の互換が必要な環境では create_fastembed_symlink を呼び出し、.fastembed_cache をグローバル models へリンク
- プロジェクト登録/更新
  1. register_or_update_project → load（失敗時は空レジストリ）→ canonicalize で既存検索 → 更新 or 新規 → save
  2. 後続処理で find_project_by_id/_mut や update_project_path を利用してメタを取得・編集
- インデックスパス解決
  1. resolve_index_path が設定/--config の文脈から最終的なインデックスディレクトリを決定

データフローの要点
- projects.json は JSON で round-trip される（Serde）
- ProjectInfo.path は canonicalize の結果（失敗時は元パス）で重複登録を避ける

## Complexity & Performance

- ディレクトリ作成/存在チェック: O(1)
- JSON読み書き（load/save/register_*）: O(n) で n=ファイルサイズ。典型的運用では数KB〜数百KB規模が想定され、メモリ・CPUともに軽微
- パス canonicalize: 基本 O(1) だが OS のファイルシステムルックアップに依存（I/Oコスト）
- symlink 操作: O(1)。ただし OS 権限/ファイルシステムの制約で失敗することがある
- スケール限界:
  - projects.json が巨大化すると、全読み/全書きのため遅延・競合が増える。多数プロジェクトを扱う場合は KV ストア等への移行を検討

## Edge Cases, Bugs, and Security

詳細表

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ホームディレクトリ取得不可 | dirs::home_dir() → None | エラーとして呼び出し元に返す | expect で panic | Bug |
| projects.json が壊れている | 破損JSON | エラーを通知し、上書きしない | load は IndexError、しかし register_* は new で上書き保存の恐れ | Risk |
| projects.json の同時更新 | 複数プロセスが save | アトミックで破損しない | 直接 write。ロック/一時ファイル経由の rename なし | Risk |
| .fastembed_cache がディレクトリ | 実体ディレクトリが存在 | 非破壊でスキップ | 警告して終了 | OK |
| .fastembed_cache が他所へのリンク | 間違いリンク | 正しく作り直す | 削除後に作成 | OK |
| 既定モデル不在 | グローバル models に既定モデルなし | ダウンロード予定の情報提供 | 案内メッセージ | OK |
| Windows 権限不足 | symlink 権限なし | 明確なエラー | io::Error で伝播 | OK |
| 非UTF-8のパス | OS依存パス | ロスなく保持 | Serde PathBuf は文字列化。環境依存の制約あり | 注意 |
| update_project_path の正規化 | new_path がシンボリック・相対 | 正規化して保存 | そのまま保存（name は file_name から） | 改善余地 |
| resolve_index_path の特殊ケース | config_path に親がない | フォールバックで安全に解決 | 分岐上は workspace_root/そのままへ | OK |

セキュリティチェックリスト
- メモリ安全性: unsafe 不使用。ヒープ/スタック管理は Rust により安全。
- インジェクション: SQL/コマンド実行なし。パス結合は固定ベースディレクトリを使い、ユーザ入力の任意結合はない（Path traversal のリスクは低い）。
- 認証・認可: 対象外。
- 秘密情報: ハードコードされた秘密はなし。ただしログに**絶対パス**が出力されるため運用ログの扱いに注意。
- 並行性: OnceLock は安全。一方、projects.json の読み書きにプロセス間/スレッド間のロックがないため**競合/破損**のリスクがある（アトミック書き込みやロックを推奨）。

Rust特有の観点（詳細）
- 所有権/借用
  - find_project_by_id は &ProjectInfo、find_project_by_id_mut は &mut ProjectInfo を返し、ミュータブル不変性は正しく守られる。
  - update_project_path は内部で mutable 参照取得後にフィールド更新し save する。ライフタイムはスコープ内に限定され安全。
- ライフタイム
  - 明示ライフタイムは不要。返却参照はレジストリインスタンスに束縛。
- unsafe境界
  - unsafe ブロックは存在しない（このチャンクには現れない → 実装無し）。
- 並行性/非同期
  - API は同期的。Send/Sync 制約明示はないが、構造体は標準型の集約であり Send/Sync を満たす想定。並行アクセス時の内部整合性（ファイル書き込み）は未保護。
  - await の境界なし。キャンセル考慮は不要。
- エラー設計
  - ファイル周りで io::Error と IndexError が混在し、統一性に欠ける。
  - unwrap/expect は global_dir で使用（ホーム無し時 panic）。本番コードでは避けるべき。
  - エラー変換は serde_json エラーを io::Error::other に包む箇所あり。

## Design & Architecture Suggestions

- エラー方針の統一
  - init_global_dirs/init_profile_infrastructure/create_fastembed_symlink も含め、全体を IndexError（あるいは anyhow::Error）に統一するか、逆に Registry 側も io::Error ベースに統一すると理解が容易。
  - global_dir の expect を廃止し Result<PathBuf, IndexError>（または io::Error）で返却する API を追加（後方互換のため get_or_panic と結果型の get_or_error を併存可）。
- レジストリ I/O の強化
  - 読み込み失敗時に空で上書きしないよう、register_* は load エラーをそのまま返す（ユーザに「バックアップして削除」案内は load() がすでに提供済み）。
  - 保存は一時ファイルへ書き、fs::rename でアトミックに差し替える（tempfile クレート + persist）。
  - 可能ならファイルロック（fd-lock/fs2）を導入し、同時更新を防止。
- 依存の見直し
  - dirs はメンテ状況に留意。directories / directories-next / home など代替の検討。
  - ProjectId は uuid クレートの v4 で代替可能（標準化された表現・パーサが得られる）。現行方式でも安全だが、エコシステム互換性で利点あり。
- ログ基盤
  - println!/eprintln! ではなく tracing を導入し、レベル（info/warn/error）とターゲット名、コンテキスト（プロジェクトID等）を整備。
- データモデル
  - ProjectRegistry の version を活かし、読み込み時のマイグレーションフックを設ける。
  - default_project の setter/getter を追加し、明確に操作できるAPIを提供。
  - ProjectInfo.last_modified/doc_count などを更新する専用APIを追加（整合性維持）。
- API 設計
  - update_project_path は new_path を canonicalize して保存する方が重複回避に一貫。
  - resolve_index_path: local_dir_name のみに依存するヒューリスティクスは脆い場合があるため、設定ファイルのメタ情報（workspace_root）を優先する設計も検討。

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト
  - ProjectId::new の長さ/一意性確認（数百回生成して重複なし）。
  - ProjectRegistry の load/save round-trip：
    ```rust
    use tempfile::tempdir;
    use std::fs;

    #[test]
    fn registry_round_trip() -> Result<(), Box<dyn std::error::Error>> {
        let dir = tempdir()?;
        // グローバルディレクトリ差し替えが必要なら、init.rs を DI 可能にする設計へ
        // ここでは疑似的にパスを直接操作
        let mut reg = ProjectRegistry::new();
        reg.save()?; // デフォルトパスに保存（実運用では DI 推奨）

        let loaded = ProjectRegistry::load()?;
        // 期待する構造が同じ
        Ok(())
    }
    ```
  - register_or_update_project の重複検出（canonicalize で同一判定）。
  - resolve_index_path の全分岐（絶対パス、--config直下、.codanna 配下、workspace_root 指定あり/なし）。
  - create_fastembed_symlink：事前に .fastembed_cache の状況をモック（実ファイルシステムで assert_fs や tempdir を用い、Unix/Windows 切替は cfg で分ける）。

- 破損系テスト
  - projects.json に無効 JSON を置いて load のエラーを検証。
  - register_* がエラーを適切に伝搬すること（現状は改善後に）。

- 統合テスト
  - HOME 環境変数の一時変更と OnceLock の初期化リセット（設計変更で DI 可能にしてから）により、実ホームを汚染せず検証。
  - 初期化 → 登録 → 取得 → パス更新 → 保存 → 再読込 の一連の流れ。

- Windows 特有
  - symlink 権限が無い場合の失敗を想定し、テストは条件付きでスキップ。

## Refactoring Plan & Best Practices

- エラー統一と expect 排除
  - global_dir_safe() -> Result<PathBuf, E> を追加し、既存 global_dir() は非推奨化。
  - register_* の load 失敗を返すよう変更。CLI 層でユーザガイダンス表示。
- 保存のアトミック化
  - tempfile::NamedTempFile で JSON を書き、persist/rename。
  - 保存前にバックアップ（.bak）を任意で作成。
- ファイルロック
  - 保存/ロード中に排他（fd-lock::RwLockFile など）を導入。
- ログ標準化
  - tracing を採用し、info!(target="codanna:init", ...) のようにターゲットを固定。
- DI でテスト容易化
  - 基底ディレクトリ（ホーム配下）を注入可能にし、OnceLock の中身をテスト専用に切り替えられるフックを用意。
- API 拡充
  - ProjectRegistry に set_default_project/get_default_project を追加。
  - update_project_path で canonicalize を適用。

## Observability (Logging, Metrics, Tracing)

- ログ
  - 成功/失敗/分岐（特に create_fastembed_symlink と resolve_index_path）で info/warn/error を適切に記録。
  - パスは必要最小限にマスク化（情報漏洩対策）。
- メトリクス
  - 登録プロジェクト数、save/load 成功回数・失敗回数、処理時間（ヒストグラム）。
- トレーシング
  - init_global_dirs, ProjectRegistry::save/load, resolve_index_path をスパン化し、相関 ID（プロジェクトID）を属性に付与。
- 例（tracing）
  ```rust
  tracing::info!(target="codanna:init", dir=%global.display(), "Using global directory");
  ```

## Risks & Unknowns

- crate::config::Settings の正確な構造は「このチャンクには現れない」。index_path と workspace_root が存在する前提で記述されているが、将来の拡張で仕様変更の可能性あり。
- fastembed 5.0+ では with_cache_dir() が推奨のため、symlink 関数は**後方互換**目的。今後のバージョン組合せ次第で不要化/削除の判断が必要。
- Windows の symlink 要件（権限/開発者モード）は実行環境依存。ドキュメント化/フォールバック戦略（コピーなど）が必要な場合あり。
- ProjectInfo の数値フィールド（symbol_count/file_count/last_modified/doc_count）は初期値0で運用。更新契機・整合性の定義は「不明」。
- projects.json のスキーマ version は 1 固定。将来の移行戦略は「不明」。

## Walkthrough & Data Flow（補足・コード抜粋）

create_fastembed_symlink の重要部分（短縮）
```rust
#[cfg(unix)]
std::os::unix::fs::symlink(&global_models, &local_cache)?;
#[cfg(windows)]
std::os::windows::fs::symlink_dir(&global_models, &local_cache)?;
```

resolve_index_path の要点（短縮）
```rust
if settings.index_path.is_absolute() { return settings.index_path.clone(); }
if let Some(cfg) = config_path {
    if let Some(parent) = cfg.parent() {
        if parent.file_name() == Some(OsStr::new(local_dir_name())) {
            if let Some(ws) = parent.parent() {
                return ws.join(&settings.index_path);
            }
        }
        return parent.join(&settings.index_path);
    }
}
if let Some(ws) = &settings.workspace_root {
    return ws.join(&settings.index_path);
}
settings.index_path.clone()
```

上記はこのチャンク内の関数本体に対応（行番号: 不明）。