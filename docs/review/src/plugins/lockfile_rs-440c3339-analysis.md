# plugins\lockfile.rs Review

## TL;DR

- 目的: インストール済みプラグインをローカルJSONロックファイルで管理するための**データ構造とI/O**を提供
- 主要公開API: **PluginLockfile::{load, save, add_plugin, remove_plugin, is_installed, get_plugin, find_file_owner}**
- 複雑箇所: **find_file_owner**は全プラグイン・全ファイルをスキャンし、さらに毎回文字列を新規に生成するため非効率
- 重大リスク: **HashMap走査順の非決定性**により、同一ファイルを複数プラグインが所有した場合、**所有者特定が不安定**
- エラー設計: **LockfileCorrupted**へ一律変換で原因喪失。元エラーのコンテキストを保持すべき
- Rust安全性: **unsafeなし**、参照のライフタイムは**selfに束縛**で安全。TOCTOUを含むI/O競合には注意
- 改善提案: **逆引きインデックス（file→plugin）**導入、**filesをHashSet**に、**atomic write**採用、**Path/PathBuf**の利用

## Overview & Purpose

このモジュールは、インストール済みプラグインを記録するロックファイル（JSON）を読み書きし、プラグインの存在確認・追加・削除・ファイル所有者検索などの基本操作を提供します。ロックファイルの**スキーマ定義**（PluginLockfile, PluginLockEntry, LockfilePluginSource）とそれに対する**I/OとクエリAPI**がコアです。

用途:
- プラグインのインストール・更新時に**状態を永続化**
- プロセス起動時に**現在のインストール状況を復元**
- ファイルとの紐付けにより**所有プラグインの特定**

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | PluginLockfile | pub | ロックファイルのトップレベル。バージョンとプラグイン一覧を保持し、読み書き・管理APIを提供 | Low |
| Struct | PluginLockEntry | pub | 個々のプラグインのメタデータ（バージョン、コミット、URL、タイムスタンプ、整合性、ファイル群、MCPキー、ソース） | Low |
| Enum | LockfilePluginSource | pub | プラグインファイルの取得元情報（マーケットプレイス内パスまたは外部Git） | Low |
| Fn (impl) | PluginLockfile::new | pub | 空のロックファイル生成（version=1.0.0） | Low |
| Fn (impl) | PluginLockfile::load | pub | JSONロックファイルの読み込み（存在しなければ空を返す） | Med |
| Fn (impl) | PluginLockfile::save | pub | JSONロックファイルの保存（親ディレクトリの作成含む） | Med |
| Fn (impl) | PluginLockfile::is_installed | pub | プラグイン名の存在確認 | Low |
| Fn (impl) | PluginLockfile::get_plugin | pub | プラグインエントリ取得（参照） | Low |
| Fn (impl) | PluginLockfile::add_plugin | pub | プラグインエントリの追加/更新 | Low |
| Fn (impl) | PluginLockfile::remove_plugin | pub | プラグインエントリの削除 | Low |
| Fn (impl) | PluginLockfile::find_file_owner | pub | 指定ファイルの所有プラグイン名を探索 | Med |

### Dependencies & Interactions

- 内部依存
  - load → Self::new を利用（存在しない場合に空生成）
  - save → 親ディレクトリ作成（create_dir_all）
  - find_file_owner → plugins HashMapを走査し、各PluginLockEntry.filesを検索

- 外部依存（表）
  | クレート/モジュール | 用途 |
  |---------------------|------|
  | super::error::{PluginError, PluginResult} | エラー・Result型（詳細はこのチャンクに現れない） |
  | serde::{Deserialize, Serialize} | JSONシリアライズ/デシリアライズ |
  | std::collections::HashMap | プラグイン一覧管理 |
  | std::fs | ファイル読み書き |
  | std::path::Path | ファイルパス入力 |

- 被依存推定
  - プラグイン管理の上位レイヤ（インストーラ/アップデータ）
  - CLI/サービス起動時の初期化コード
  - MCPサーバ設定管理の連携部分（mcp_keysを利用するコンポーネント）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | fn new() -> Self | 空ロックファイルの生成 | O(1) | O(1) |
| load | fn load(path: &Path) -> PluginResult<Self> | ロックファイル読み込み（なければ新規） | O(n) | O(n) |
| save | fn save(&self, path: &Path) -> PluginResult<()> | ロックファイル保存（pretty JSON） | O(n) | O(n) |
| is_installed | fn is_installed(&self, name: &str) -> bool | プラグイン存在確認 | O(1)平均 | O(1) |
| get_plugin | fn get_plugin(&self, name: &str) -> Option<&PluginLockEntry> | プラグイン参照取得 | O(1)平均 | O(1) |
| add_plugin | fn add_plugin(&mut self, entry: PluginLockEntry) | プラグイン追加/更新 | O(1)平均 | O(1)増加 |
| remove_plugin | fn remove_plugin(&mut self, name: &str) -> Option<PluginLockEntry> | プラグイン削除 | O(1)平均 | O(1)減少 |
| find_file_owner | fn find_file_owner(&self, file_path: &str) -> Option<&str> | ファイル所有者のプラグイン名探索 | O(P+F) | O(1)+一時文字列 |

データ契約（主要フィールド）
- PluginLockfile
  - version: String（ロックファイルフォーマットのバージョン。newでは"1.0.0"）
  - plugins: HashMap<String, PluginLockEntry>（キー＝プラグイン名）
- PluginLockEntry
  - name, version, commit, marketplace_url: String（必須）
  - installed_at, updated_at: String（タイムスタンプ。形式はこのチャンクでは不明）
  - integrity: String（整合性チェックサム）
  - files: Vec<String>（インストール済みファイルの相対パス等）
  - mcp_keys: Vec<String>（serde(default)で省略可）
  - source: Option<LockfilePluginSource>（serde(tag="type")でバリアント識別）
- LockfilePluginSource
  - MarketplacePath { relative: String }
  - Git { url: String, git_ref: Option<String>, subdir: Option<String> }（Optionはskip_serializing_if）

各APIの詳細

1) PluginLockfile::load
- 目的と責務
  - 指定パスのロックファイル（JSON）を読み込み、構造体へ復元。ファイルが存在しなければ新規ロックファイルを返す
- アルゴリズム
  1. path.exists()で存在確認
  2. なければ Self::new() を返す
  3. read_to_stringで内容を取得
  4. serde_json::from_strでパース
  5. 失敗時は PluginError::LockfileCorrupted に変換
- 引数
  | 名 | 型 | 説明 |
  |----|----|------|
  | path | &Path | ロックファイルパス |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | PluginResult<Self> | 成功時PluginLockfile、失敗時PluginError |
- 使用例
  ```rust
  use std::path::Path;
  let lf = PluginLockfile::load(Path::new(".claude/plugins.lock"))?;
  ```
- エッジケース
  - 空ファイル/不正JSONでLockfileCorruptedへ変換（元原因が失われる）
  - TOCTOU: exists→readの間に状態変化

2) PluginLockfile::save
- 目的と責務
  - ロックファイルの現在の状態を指定パスへ保存（pretty JSON）
- アルゴリズム
  1. 親ディレクトリがあれば create_dir_all
  2. serde_json::to_string_prettyでシリアライズ
  3. std::fs::writeで書き込み
- 引数
  | 名 | 型 | 説明 |
  |----|----|------|
  | path | &Path | 出力ファイルパス |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | PluginResult<()> | 成功はOk(()), 失敗はPluginError |
- 使用例
  ```rust
  let lf = PluginLockfile::new();
  lf.save(Path::new(".claude/plugins.lock"))?;
  ```
- エッジケース
  - 書込み中断で部分的ファイル/破損
  - パーミッション不足

3) PluginLockfile::add_plugin
- 目的と責務
  - プラグインのエントリを追加または更新（キーはentry.name）
- アルゴリズム
  1. HashMap.insert(entry.name.clone(), entry)
- 引数
  | 名 | 型 | 説明 |
  |----|----|------|
  | entry | PluginLockEntry | 追加/更新対象 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | () | なし |
- 使用例
  ```rust
  let mut lf = PluginLockfile::new();
  lf.add_plugin(PluginLockEntry{
      name: "test".into(), version: "1.0.0".into(),
      commit: "abc".into(), marketplace_url: "https://example.com".into(),
      installed_at: "2024-01-01".into(), updated_at: "2024-01-01".into(),
      integrity: "sha256:...".into(),
      files: vec![".claude/commands/test.md".into()],
      mcp_keys: vec![], source: None,
  });
  ```
- エッジケース
  - entry.nameが既存キーと異なっても上書きされるため一貫性チェックが必要

4) PluginLockfile::remove_plugin
- 目的と責務
  - 指定名のプラグインを削除し、エントリを返す
- アルゴリズム
  1. HashMap.remove(name)
- 引数
  | 名 | 型 | 説明 |
  |----|----|------|
  | name | &str | プラグイン名 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Option<PluginLockEntry> | 削除されたエントリまたはNone |
- 使用例
  ```rust
  let removed = lf.remove_plugin("test");
  ```

5) PluginLockfile::is_installed
- 目的と責務
  - プラグイン名の存在確認
- アルゴリズム
  1. HashMap.contains_key(name)
- 引数/戻り値
  | 引数 | 型 | 戻り値 | 説明 |
  |------|----|--------|------|
  | name | &str | bool | 存在する場合true |

6) PluginLockfile::get_plugin
- 目的と責務
  - プラグインの参照取得
- アルゴリズム
  1. HashMap.get(name)
- 引数/戻り値
  | 引数 | 型 | 戻り値 | 説明 |
  |------|----|--------|------|
  | name | &str | Option<&PluginLockEntry> | 参照またはNone |

7) PluginLockfile::find_file_owner
- 目的と責務
  - 指定ファイルを所有するプラグイン名を返す
- アルゴリズム
  1. self.pluginsを反復
  2. 各entry.filesに対しcontains(&file_path.to_string())で検索
  3. 見つかったらそのキー名を返す
- 引数/戻り値
  | 引数 | 型 | 戻り値 | 説明 |
  |------|----|--------|------|
  | file_path | &str | Option<&str> | 所有者名またはNone |
- 使用例
  ```rust
  assert_eq!(lf.find_file_owner(".claude/commands/test.md"), Some("test"));
  ```
- エッジケース
  - 同一ファイルが複数プラグインに存在する場合、返却は非決定的（HashMap順序）
  - パス正規化をしていないため、区切り文字や相対/絶対でミスマッチ

## Walkthrough & Data Flow

- ロード
  - 入力: Path
  - 処理: 存在チェック → ファイル読み込み → JSONパース → PluginLockfile構築
  - 出力: PluginLockfile（存在しない場合は new による空）
- 保存
  - 入力: PluginLockfile, Path
  - 処理: 親ディレクトリ作成 → JSON整形 → 書込み
  - 出力: 成否
- 操作
  - add_plugin: entry.nameをキーにHashMapへ挿入
  - remove_plugin: nameキーで削除し、所有していたエントリを返却
  - get/is_installed: nameキーで参照・存在判定
  - find_file_owner: HashMap全件走査 → filesベクタ検索 → 最初に見つかったキー名を返却

## Complexity & Performance

- new: O(1)時間・空間
- load: O(n)時間（ファイルサイズn）、O(n)空間（文字列バッファ＋構造体）
- save: O(n)時間（シリアライズサイズn）、O(n)空間（出力バッファ）
- is_installed/get/remove/add: 平均O(1)（HashMap）、空間は状況に応じて増減
- find_file_owner: O(P + Σfiles_i) 時間。さらに各contains呼び出しで**file_path.to_string()**が都度割り当てを行うため、**追加の割り当てコスト**あり（最悪O(P)回の文字列生成）

ボトルネック/スケール限界:
- 大量プラグイン/多数ファイル時、**find_file_owner**が線形スキャンで遅くなる
- ロックファイルのサイズが大きい場合、**pretty JSON**のシリアライズ/デシリアライズがCPU/メモリを消費
- I/Oは単純なwriteで**アトミック性**がなく、クラッシュ/競合時に破損の可能性

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価
- メモリ安全性: unsafe未使用で基本安全。参照は&selfに束縛。大規模JSONで**メモリ圧迫**の潜在リスク
- インジェクション: SQL/Commandは無し。Pathは外部入力になり得るが**ファイル読み書きのみ**。パストラバーサルは「指定パスに対して」行うため、上位での検証責任
- 認証・認可: 該当なし（ローカルI/Oのみ）
- 秘密情報: ハードコード秘密は無し。ログ出力は現状無し
- 並行性: 同時読み書きで**競合/破損**の可能性。ファイルロックや**atomic write**が未対応

Rust特有の観点（関数名:行番号不明）
- 所有権
  - add_plugin: entryはムーブされHashMapに所有権移転
  - remove_plugin: エントリはムーブアウト（所有権返却）
- 借用/ライフタイム
  - find_file_owner: 戻り値はHashMapキー（String）への**借用&str**で、selfに束縛され安全
- unsafe境界
  - なし
- 並行性/非同期
  - Send/Syncは構造的に満たされる可能性が高い（String/Vec/HashMap）だが、**&mut self**メソッド利用時は外部同期が前提
- エラー設計
  - load: serdeの詳細エラーを**PluginError::LockfileCorrupted**へ一律変換し、**原因喪失**。コンテキスト付与が望ましい

エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ファイル非存在 | path=".claude/plugins.lock" 不在 | 空ロックファイル返却 | loadで分岐 | OK |
| 空ファイル | "" | エラー（LockfileCorrupted） | serde失敗→変換 | 要改善（原因喪失） |
| 破損JSON | "{ not json" | エラー（詳細保持） | 一律変換 | 要改善（詳細不足） |
| 同一ファイル複数所有 | files重複 | 明確な優先規則/エラー | 線形探索で最初一致（順序非決定） | 問題あり |
| 大量ファイル | 1万件 | 所有者検索が速い | 線形探索＋文字列割当 | ボトルネック |
| パス正規化 | ".\a\b" vs "./a/b" | 同一扱い | 文字列比較のみ | 要改善 |
| name/キー不一致 | mapキー="A", entry.name="B" | 検知/拒否 | 未検知 | 要改善 |
| タイムスタンプ形式 | "2025-01-11" | フォーマット検証 | Stringのみ | 不明 |

バグ/改善ポイント
- find_file_ownerのcontainsで毎回**file_path.to_string()**を生成するため不要なヒープ割り当て（パフォーマンス劣化）
- HashMap反復順序に依存し所有者決定が非決定的
- PluginLockfileに**#[derive(Default)]**があるが、newはversion="1.0.0"で、Defaultはversion=""となるため**二重の初期化規則**が存在（混乱の原因）
- エラーの詳細が失われる（LockfileCorrupted一律）

## Design & Architecture Suggestions

- find_file_owner改善
  - filesをVec<String>から**HashSet<String>**へ変更しO(1)探索化（順序不要なら）
  - またはロード時に**逆引きインデックス（HashMap<String, String>：file→plugin）**を構築し、O(1)で所有者特定
  - 競合（同一ファイルを複数プラグインが所有）検知時に**明示的エラー**を返す
- I/Oの堅牢化
  - **atomic write**（一時ファイル作成→fsync→rename）により破損を防ぐ
  - existsチェックの代わりに**open→NotFoundでnew**にすることでTOCTOUを緩和
- Path扱いの改善
  - API引数や保存値を**PathBuf**ベースにし、OS依存文字列と**正規化**（canonicalize/相対→相対統一）を行う
- エラー拡張
  - PluginError::LockfileCorruptedに**sourceエラー**を含める（map_err(|e| PluginError::LockfileCorrupted{ /* eを格納 */ })）
  - コンテキストメッセージ（path含む）を付与
- スキーマ整備
  - PluginLockfileのDefaultを**newと整合**させる（Defaultで"1.0.0"）
  - timestampsは**ISO 8601**の型（time/chrono）で保持し、serdeで文字列化
  - integrityを構造化（アルゴリズム/値を分離）
  - marketplace_urlは**Option**にし、無いケースでskip_serializing_if

## Testing Strategy (Unit/Integration) with Examples

推奨テストケース
- Unit
  - newの初期値（version="1.0.0"）
  - add/remove/get/is_installedの基本操作
  - find_file_ownerの所有者特定とNoneケース
  - nameとキーの不一致検知（設計改善後）
- Integration（I/O）
  - save→loadのラウンドトリップでフィールド完全一致
  - 不正JSONでLockfileCorrupted（詳細が保持されるか、改善後）
  - 大量エントリのパフォーマンス測定
  - atomic write採用時の中断耐性

例: save/loadラウンドトリップ
```rust
use std::path::Path;
use tempfile::tempdir;

let dir = tempdir()?;
let path = dir.path().join("plugins.lock");

let mut lf = PluginLockfile::new();
lf.add_plugin(PluginLockEntry{
    name: "p1".into(), version: "1.0.0".into(),
    commit: "abc123".into(), marketplace_url: "https://example.com".into(),
    installed_at: "2024-01-01".into(), updated_at: "2024-01-01".into(),
    integrity: "sha256:deadbeef".into(),
    files: vec![".claude/commands/p1.md".into()],
    mcp_keys: vec![], source: Some(LockfilePluginSource::MarketplacePath { relative: "plugins/p1".into() }),
});
lf.save(&path)?;
let lf2 = PluginLockfile::load(Path::new(&path))?;
assert!(lf2.is_installed("p1"));
assert_eq!(lf2.find_file_owner(".claude/commands/p1.md"), Some("p1"));
```

例: 破損JSONの検知（エラー詳細保持がある前提）
```rust
use std::fs;
use std::path::Path;
let dir = tempfile::tempdir()?;
let path = dir.path().join("plugins.lock");
fs::write(&path, "{ this is not json")?;
let err = PluginLockfile::load(Path::new(&path)).unwrap_err();
// assert!(matches!(err, PluginError::LockfileCorrupted { .. }));
```

プロパティ/ファズ
- ランダム生成PluginLockfileをserdeでserialize→deserializeして**不変性**検証
- ランダムJSONでloadして**健全な失敗**を保証

## Refactoring Plan & Best Practices

- 短期
  - find_file_ownerの割当削減（to_stringをやめ、イテレータで&str比較）
  - Defaultのversionを"1.0.0"に合わせる
  - エラーに元例外の詳細を含める
- 中期
  - filesをHashSetへ、または逆引きインデックスを導入
  - saveで**atomic write**採用（tempfile→rename）
  - Path/PathBufの導入とパス正規化
- 長期
  - スキーマバージョン管理（マイグレーションフロー）
  - integrityやtimestampの型安全化
  - 競合検知とポリシー（同一ファイル複数所有の扱い）

改善版find_file_owner（割当削減）
```rust
impl PluginLockfile {
    pub fn find_file_owner(&self, file_path: &str) -> Option<&str> {
        for (name, entry) in &self.plugins {
            if entry.files.iter().any(|f| f == file_path) {
                return Some(name.as_str());
            }
        }
        None
    }
}
```

## Observability (Logging, Metrics, Tracing)

- ログ（tracing）
  - load開始/成功/失敗（path, サイズ、エラー詳細）
  - save開始/成功/失敗（path, サイズ）
  - add/removeで**plugin name**をinfo/debugログ
- メトリクス
  - plugins数、総ファイル数
  - load/saveの所要時間/失敗率
- トレーシング
  - ロックファイル操作には**span**を付与し、上位からの呼び出しチェーンを追跡可能に

## Risks & Unknowns

- PluginError/PluginResultの詳細は**不明**（このチャンクには現れない）
- タイムスタンプ形式・バリデーションは**不明**
- ロックファイルのスキーマ進化（versionの互換性ポリシー）が**不明**
- 複数プロセス/スレッドによる同時書込みの扱いが**未定**（ファイルロック・atomic write未導入）
- パスのプラットフォーム差異（区切り文字・ケース感度）への対応は**未定**

以上により、このモジュールは基本要件を満たしつつ安全に動作しますが、I/O堅牢化・検索性能・エラーコンテキスト・スキーマ整備の観点で改善余地があります。