# profiles\test_project_manifest.rs Review

## TL;DR

- 🎯 目的: ProjectManifestのJSONパースとファイル永続化API（from_json, new, save, load, load_or_create）の基本挙動を検証するユニットテスト群。
- 🔑 主要公開API（推定）: `ProjectManifest::from_json`, `::new`, `::save`, `::load`, `::load_or_create`。特に「空のprofileはエラー」や「存在しないファイルはErr」が仕様としてテストで確認されている。
- 🧩 複雑箇所: ファイルIO系（save/load/load_or_create）の分岐（存在時はロード、非存在時は作成）が仕様上重要だが、このチャンクには実装は現れない。
- ⚠️ 重大リスク（推定）: エラーメッセージ依存テスト（"Profile name"の文字列一致）、ファイル書き込みの原子性・並行アクセス安全性は不明、エラー型詳細は不明。
- 🛡️ Rust安全性: このチャンクにunsafeは登場しない。テストでは`unwrap`多用（テストでは妥当だが本番コードでは避けるべき）。
- 📈 パフォーマンス: JSON長とファイルサイズに線形（O(n)）でスケール。IOが主ボトルネック。
- 🧪 テスト網羅性: 基本パス・失敗パス（空profile、missing file）・存在時/非存在時のload_or_createをカバー。異常系（破損JSON、権限エラー、競合書き込み）は未カバー。

## Overview & Purpose

このファイルは、プロジェクト設定を表現する`ProjectManifest`の以下の振る舞いを検証するためのRustユニットテストです。

- JSON文字列からのパース（最小構成）
- バリデーション（profileが空文字の場合はエラー）
- ファイルへの保存およびファイルからの読み込み
- ファイルが存在しない場合の`load_or_create`の作成動作と、存在する場合のロード動作

実装本体は`codanna::profiles::project::ProjectManifest`にあり、このチャンクにはテストコードのみが含まれます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function (#[test]) | test_parse_minimal_project_manifest | private | 最小JSONからのパース成功を検証 | Low |
| Function (#[test]) | test_reject_empty_profile | private | 空のprofileでエラーになることを検証 | Low |
| Function (#[test]) | test_save_and_load | private | save後にloadで同値が取得できることを検証 | Low |
| Function (#[test]) | test_load_missing_file | private | 存在しないファイルでloadがErrになることを検証 | Low |
| Function (#[test]) | test_load_or_create_existing | private | 既存ファイルがある場合、load_or_createがロードすることを検証 | Low |
| Function (#[test]) | test_load_or_create_missing | private | ファイルがない場合、load_or_createが新規作成することを検証 | Low |

### Dependencies & Interactions

- 内部依存
  - 各テストは`ProjectManifest`のAPIを直接呼び出すのみ。テスト間での相互依存はありません。
  - `test_save_and_load`および`test_load_or_create_existing`は、`new`→フィールド設定→`save`→`load`/`load_or_create`の順に呼び出し。

- 外部依存（このチャンクに現れるもの）
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | codanna::profiles::project::ProjectManifest | マニフェストのパース/IO | 実装詳細はこのチャンクには現れない |
  | tempfile::tempdir | テスト用の一時ディレクトリ作成 | OSごとに適切な一時領域を使用 |

- 被依存推定
  - 本ファイルはテスト専用であり、プロダクションコードから直接参照されません。Cargoのテストランナーによって実行されます。

## API Surface (Public/Exported) and Data Contracts

このチャンクはテストであり、公開APIの定義は含みません。テストから推測されるAPIを以下に列挙します（詳細は「不明」「推測」と明記）。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ProjectManifest::from_json | fn from_json(json: &str) -> Result<ProjectManifest, E> | JSON文字列から`ProjectManifest`へパースし、バリデーション（profile非空）を行う | O(n) | O(n) |
| ProjectManifest::new | fn new() -> ProjectManifest | デフォルト値を持つ新規インスタンス生成 | O(1) | O(1) |
| ProjectManifest::save | fn save(path: &Path) -> Result<(), E> | マニフェストをJSONでファイルに保存 | O(n) | O(n) |
| ProjectManifest::load | fn load(path: &Path) -> Result<ProjectManifest, E> | ファイルからJSONを読み込み、パースして返す | O(n) | O(n) |
| ProjectManifest::load_or_create | fn load_or_create(path: &Path) -> Result<ProjectManifest, E> | ファイルがあればロード、なければ新規作成を返す（保存有無は不明） | O(n) | O(n) or O(1) |

詳細（テストから読み取れる契約のみ）:

- ProjectManifest::from_json
  1. 目的と責務
     - JSONから`ProjectManifest`を生成。
     - 少なくとも`profile`フィールドが存在し、空文字列はエラー。
  2. アルゴリズム（推定）
     - JSONをデシリアライズ。
     - `profile`が空でないことを検証。
     - 成功ならOk、失敗ならErr。
  3. 引数
     | 引数名 | 型 | 意味 |
     |--------|----|------|
     | json | &str | マニフェストJSON文字列 |
  4. 戻り値
     | 型 | 意味 |
     |----|------|
     | Result<ProjectManifest, E> | 成功時は構造体、失敗時はエラー（型Eは不明） |
  5. 使用例
     ```rust
     #[test]
     fn test_parse_minimal_project_manifest() {
         let json = r#"{
             "profile": "claude"
         }"#;
         let manifest = ProjectManifest::from_json(json).unwrap();
         assert_eq!(manifest.profile, "claude");
     }
     ```
  6. エッジケース
     - 空文字の`profile`はErr（test_reject_empty_profileにて検証）。
     - 無効なJSON（構文エラー）: 不明。
     - `profile`フィールド欠如: 不明。

- ProjectManifest::new
  1. 目的と責務
     - デフォルト初期化。テストからは`profile`が空の可能性が高い。
  2. アルゴリズム
     - デフォルト値で構造体を返す。
  3. 引数
     | 引数名 | 型 | 意味 |
     |--------|----|------|
     | なし | - | - |
  4. 戻り値
     | 型 | 意味 |
     |----|------|
     | ProjectManifest | 新規インスタンス |
  5. 使用例
     ```rust
     let mut manifest = ProjectManifest::new();
     manifest.profile = "claude".to_string();
     ```
  6. エッジケース
     - デフォルトの`profile`が空である契約（推測）。明示仕様は不明。

- ProjectManifest::save
  1. 目的と責務
     - 現在の構造体状態をJSONとして指定パスに保存。
  2. アルゴリズム（推定）
     - 構造体をJSONにシリアライズ。
     - ファイルへ書き込み。
  3. 引数
     | 引数名 | 型 | 意味 |
     |--------|----|------|
     | path | &Path | 保存先パス |
  4. 戻り値
     | 型 | 意味 |
     |----|------|
     | Result<(), E> | 成功/失敗 |
  5. 使用例
     ```rust
     #[test]
     fn test_save_and_load() {
         let temp = tempfile::tempdir().unwrap();
         let manifest_path = temp.path().join("manifest.json");

         let mut manifest = ProjectManifest::new();
         manifest.profile = "claude".to_string();

         manifest.save(&manifest_path).unwrap();
         assert!(manifest_path.exists());

         let loaded = ProjectManifest::load(&manifest_path).unwrap();
         assert_eq!(loaded.profile, "claude");
     }
     ```
  6. エッジケース
     - 書き込み権限なし: 不明。
     - ディレクトリが存在しない: 不明（親ディレクトリ自動作成の有無は不明）。
     - 原子的な書き込み（部分書き込み防止）: 不明。

- ProjectManifest::load
  1. 目的と責務
     - 指定パスからJSONを読み込み、パースして返す。
  2. アルゴリズム（推定）
     - ファイル読み込み→JSONパース→バリデーション。
  3. 引数
     | 引数名 | 型 | 意味 |
     |--------|----|------|
     | path | &Path | 読み込み元パス |
  4. 戻り値
     | 型 | 意味 |
     |----|------|
     | Result<ProjectManifest, E> | 成功時は構造体、失敗時はエラー |
  5. 使用例
     ```rust
     let loaded = ProjectManifest::load(&manifest_path).unwrap();
     assert_eq!(loaded.profile, "claude");
     ```
  6. エッジケース
     - ファイルが存在しない場合はErr（test_load_missing_fileで検証）。
     - JSON破損・不正スキーマ: 不明。

- ProjectManifest::load_or_create
  1. 目的と責務
     - ファイルが存在すればロード、なければ新規作成（返すのみか保存も行うかは不明）。
  2. アルゴリズム（推定）
     - `path.exists()`をチェック。
     - あれば`load`、なければ`new`を返す（必要に応じて`save`するかは不明）。
  3. 引数
     | 引数名 | 型 | 意味 |
     |--------|----|------|
     | path | &Path | 対象パス |
  4. 戻り値
     | 型 | 意味 |
     |----|------|
     | Result<ProjectManifest, E> | 成功時は構造体、失敗時はエラー |
  5. 使用例
     ```rust
     #[test]
     fn test_load_or_create_existing() {
         let temp = tempfile::tempdir().unwrap();
         let path = temp.path().join("manifest.json");

         let mut manifest = ProjectManifest::new();
         manifest.profile = "existing".to_string();
         manifest.save(&path).unwrap();

         let loaded = ProjectManifest::load_or_create(&path).unwrap();
         assert_eq!(loaded.profile, "existing");
     }

     #[test]
     fn test_load_or_create_missing() {
         let temp = tempfile::tempdir().unwrap();
         let path = temp.path().join("missing.json");

         let manifest = ProjectManifest::load_or_create(&path).unwrap();
         assert!(manifest.profile.is_empty());
     }
     ```
  6. エッジケース
     - 競合（他プロセスが同時に作成/削除）: 不明。
     - 新規作成時に即保存するか: 不明。

## Walkthrough & Data Flow

- JSONパース（test_parse_minimal_project_manifest）
  - 入力: JSON文字列
  - 処理: `from_json`でパースし、`manifest.profile`を検証
  - 出力: `ProjectManifest`インスタンス

- バリデーション（test_reject_empty_profile）
  - 入力: `{"profile": ""}`
  - 処理: `from_json`がErrを返す
  - 出力: エラーメッセージに"Profile name"を含むことを確認

- 保存と読み込み（test_save_and_load）
  - 入力: `new`で生成、`profile`に"claude"を設定
  - 処理: `save`でファイル作成、`load`で再読み込み
  - 出力: 同一`profile`を持つ構造体が得られる

- 存在しないファイルのロード（test_load_missing_file）
  - 入力: 存在しないパス
  - 処理: `load`がErr
  - 出力: エラーであることを確認

- 既存時/非既存時のロードまたは作成（test_load_or_create_existing, test_load_or_create_missing）
  - 入力: 既存ファイル or 不存在ファイル
  - 処理: `load_or_create`分岐（ロード or 新規作成）
  - 出力: 既存時は内容保持、非存在時は空`profile`（推定初期値）

このチャンクのテストコードは直線的で分岐が少なく、Mermaid図の使用基準に照らして図は不要です（分岐4つ以上等の条件を満たさない）。

## Complexity & Performance

- from_json: O(n) 時間（n=JSON文字列長）、O(n) 空間（デシリアライズの中間構造体）
- save: O(n) 時間（n=JSONサイズ＋書き込みバッファ）、O(n) 空間（シリアライズ）
- load: O(n) 時間（n=ファイルサイズ）、O(n) 空間（バッファ＋デシリアライズ）
- load_or_create: 既存時はO(n)、非既存時はO(1)（推定）
- ボトルネック: ディスクIOとJSONシリアライズ/デシリアライズ
- スケール限界: マニフェストが巨大な場合に線形時間・メモリ増、頻繁なIOでレイテンシ増加
- 実運用負荷要因: ファイルシステム性能、同時アクセス数、権限・ロック機構の有無

## Edge Cases, Bugs, and Security

セキュリティチェックリスト観点の評価（このチャンクのテストから判定可能な範囲）:

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（このチャンクには現れない）。
  - 所有権/借用: `save(&manifest_path)`や`load(&manifest_path)`における`&Path`借用は安全。`mut manifest`でフィールド更新は適切。
- インジェクション
  - JSONインジェクションの防止: デシリアライズ処理の具体は不明。入力バリデーションは`profile`非空のみ確認。
  - Path traversal: `Path`を直接使用。サニタイズやルート制限の有無は不明。
- 認証・認可
  - 該当なし（ローカルファイルIO）。権限チェックはOS依存で、API側の扱いは不明。
- 秘密情報
  - ハードコード秘密: 該当なし。
  - ログ漏えい: エラーメッセージ内容は"Profile name"を含む程度。不明。
- 並行性
  - Race condition / Deadlock: このチャンクには並行処理なし。ファイル共有時の競合対策は不明。

詳細エッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字のprofile | `{"profile": ""}` | Err（"Profile name"を含むメッセージ） | test_reject_empty_profile | Pass |
| 最小JSONのパース | `{"profile": "claude"}` | Okでprofile="claude" | test_parse_minimal_project_manifest | Pass |
| ファイル保存後の読み込み | tempdir/manifest.json | Okで同一内容 | test_save_and_load | Pass |
| 存在しないファイルのload | tempdir/missing.json | Err | test_load_missing_file | Pass |
| 既存ファイルのload_or_create | 事前にsave済み | Okで既存内容ロード | test_load_or_create_existing | Pass |
| 非存在ファイルのload_or_create | tempdir/missing.json | Okで新規作成（profile空） | test_load_or_create_missing | Pass |
| 破損JSON | `{"profile": }` | Err | このチャンクには現れない | 不明 |
| 書き込み権限なし | ルート直下など | Err | このチャンクには現れない | 不明 |
| 親ディレクトリなし | 深いパス | 生成/失敗のどちらか | このチャンクには現れない | 不明 |
| 同時アクセス競合 | 複数プロセス/スレッド | 安全に動作 | このチャンクには現れない | 不明 |

Rust特有の観点（このチャンクに基づく）:

- 所有権: `let mut manifest = ProjectManifest::new();`で所有し、`manifest.profile = ...`で内部フィールドにムーブ（String）を行うのは整合的。
- 借用: `save(&manifest_path)`/`load(&manifest_path)`で不変借用を短期間使用。借用期間は呼出しのスコープ内で終了。
- ライフタイム: 明示的ライフタイムは不要。
- unsafe境界: unsafeブロックは登場しない（このチャンク）。
- 並行性/非同期: Send/Sync境界は不明。非同期・awaitは登場しない。
- エラー設計: `Result`を返すAPI群。テストで`unwrap`使用（テストでは許容）。エラー型や`From/Into`の変換は不明。

## Design & Architecture Suggestions

- 明確なエラー型の定義
  - `E`を専用エラー（例: `ManifestError`）にし、バリアント（InvalidProfile, IoError, ParseError）を明示することでテストの堅牢性が向上。
- バリデーションの拡張
  - `profile`のフォーマット（禁止文字、長さ制約）や将来フィールドの整合性チェック。
- saveの原子性
  - 一時ファイルへの書き込み→`rename`による原子的更新で破損防止。
- load_or_createの契約明示
  - 新規作成時にディスクへ保存するか否かをAPIドキュメントで明示。並行アクセス時の動作（ロックの有無）も定義。
- フィールド更新API
  - 直接フィールド代入ではなく`set_profile`のようなセッターでバリデーションを再利用可能に。

## Testing Strategy (Unit/Integration) with Examples

- 破損JSONのテスト
  ```rust
  #[test]
  fn test_from_json_invalid_syntax() {
      let json = r#"{ "profile": }"#;
      let err = ProjectManifest::from_json(json).unwrap_err();
      let msg = err.to_string();
      assert!(msg.contains("parse"). || msg.contains("invalid"), "unexpected error: {}", msg);
  }
  ```
- `profile`欠落のテスト
  ```rust
  #[test]
  fn test_from_json_missing_profile() {
      let json = r#"{}"#;
      let result = ProjectManifest::from_json(json);
      assert!(result.is_err());
  }
  ```
- 権限エラーのテスト（Unix系）
  ```rust
  #[test]
  fn test_save_permission_denied() {
      let temp = tempfile::tempdir().unwrap();
      let dir = temp.path();
      // 擬似的に書き込み不可パスを想定（システム依存のため工夫が必要）
      let path = dir.join("readonly/manifest.json"); // 親作成失敗を期待
      let mut manifest = ProjectManifest::new();
      manifest.profile = "x".to_string();
      let result = manifest.save(&path);
      assert!(result.is_err());
  }
  ```
- 並行アクセスのテスト（注意: 実装側のロックが必要）
  - 異なるスレッドで`load_or_create`を同時実行し、破損や競合がないことを検証（このチャンクには非同期/並行コードは現れないため概念提案）。

- プロパティベーステスト
  - ランダムな`profile`文字列で`from_json`→`save`→`load`の往復不変性を検証。

## Refactoring Plan & Best Practices

- エラー型の導入と`thiserror`の利用
  - メッセージ文字列一致ではなく、型・バリアント一致でテストする。
- APIドキュメント整備
  - `load_or_create`の保存契約、`save`の親ディレクトリ作成可否、エンコーディング（UTF-8）などを明記。
- セッターメソッド導入
  - `set_profile(&str) -> Result<(), ManifestError>`で一貫したバリデーション。
- 原子的ファイル更新
  - `write -> flush -> fsync -> rename`の流れを採用。
- JSONスキーマのバージョニング
  - 将来拡張に備えてバージョンフィールドを追加し、後方互換性を保つ。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - `load`失敗時にファイルパス・原因（Io/Parse/Validation）をログ。機密情報は含めない。
- メトリクス
  - 成功/失敗カウント（load/save）、平均サイズ、レイテンシ計測。
- トレーシング
  - `save`/`load`にspanを付与し、IO時間・リトライ有無を追跡。
- テストでの観測
  - 現状は`err.to_string()`を検証のみ。型一致へ移行することで安定化。

## Risks & Unknowns

- エラー型・メッセージ仕様が不明なため、文字列一致に依存したテストは脆弱。
- `load_or_create`の新規作成時にファイルへ保存するのか（返すだけなのか）が不明。
- JSONスキーマ（`profile`以外のフィールド）はこのチャンクには現れない。将来拡張による互換性リスク。
- ファイルIOの原子性・並行時の整合性・ロック戦略は不明。
- 親ディレクトリの自動作成、権限エラー時の挙動は不明。