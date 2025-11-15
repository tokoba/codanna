# test_project_registry.rs Review

## TL;DR

- 目的: 一時ディレクトリに独立したレジストリファイルを使って、プロジェクト登録・検索の振る舞いを検証する統合テスト。
- 公開API: 本ファイルには公開APIはない。テスト専用のプライベート構造体とメソッドのみ。
- コアロジック: JSONファイル（projects.json）へ登録エントリを追加し、IDで検索する単純な読み書き処理。
- 複雑箇所: JSONのパース失敗時に空配列へフォールバックするが、非配列の正当なJSONに対してはpanicの可能性あり。
- 重大リスク: unwrapの多用によるpanic、ID生成の擬似性（UUIDではない）、配列線形探索のスケール限界、ファイルI/Oの非原子的更新。
- 並行性: TempDirによりテスト間の衝突は回避されるが、実運用を想定するとファイルロックなしで競合の可能性。
- エラー設計: Resultを返すが内部でunwrap/expectを使用しており、IO・時間取得等でpanicし得る。

## Overview & Purpose

このファイルは「Project registry can register a project」という統合テストで、グローバルなユーザ設定に影響を与えないよう、`tempfile::TempDir` を用いた隔離された環境下でプロジェクト登録と検索の振る舞いを検証します。実際の `ProjectRegistry` を直接使わず、テスト専用の `TestProjectRegistry` を用意して、JSONファイルへの読み書きを通じて以下を確認します。

- プロジェクト登録時に一意（に近い）なIDが返却されること
- 登録エントリ（id/name/path）がファイルに保存され、IDで検索できること
- 既存エントリの更新（path/name）の反映が確認できること

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | setup_test_env | private | 一時ディレクトリ＋レジストリファイルパスの生成 | Low |
| Struct | TestProjectRegistry | private | テスト用レジストリのファイルパス保持 | Low |
| Impl Fn | TestProjectRegistry::new | private | レジストリの初期化（ファイルパス受け渡し） | Low |
| Impl Fn | TestProjectRegistry::register_project | private | プロジェクトID生成、JSONファイルへの登録、書き込み | Med |
| Impl Fn | TestProjectRegistry::find_project_by_id | private | JSONファイルからIDでエントリ検索 | Low |
| Test | test_register_project_creates_entry | private | 登録成功・検索・値検証・非存在IDの確認 | Med |
| Test | test_update_project_path | private | 登録後のJSON直接編集による更新検証 | Med |

### Dependencies & Interactions

- 内部依存
  - `test_register_project_creates_entry` → `setup_test_env` → `TestProjectRegistry::new` → `register_project` → `find_project_by_id`
  - `test_update_project_path` → `setup_test_env` → `TestProjectRegistry::new` → `register_project` → （JSON直接編集）→ `find_project_by_id`
- 外部依存

| 依存クレート/モジュール | 用途 | 主な使用箇所 |
|------------------------|------|--------------|
| serde_json             | JSON作成・パース | `register_project`, `find_project_by_id`, テスト内の更新 |
| tempfile::TempDir      | 一時ディレクトリ生成・自動クリーンアップ | `setup_test_env` |
| std::fs                | ファイル読み書き | `register_project`, `find_project_by_id`, テスト更新 |
| std::time              | 近似的な一意ID生成（UNIX時間のナノ秒） | `register_project` |
| std::process::id       | プロセスIDをID構成要素に追加 | `register_project` |
| std::path::{Path, PathBuf} | パス表現 | 全体 |

- 被依存推定
  - 本ファイルは統合テストであり、他モジュールからの利用は「このチャンクには現れない」。実プロダクションコードの `ProjectRegistry` との関係も「不明」。

## API Surface (Public/Exported) and Data Contracts

公開APIは「該当なし」（全てテストモジュール内のプライベート要素）。ただし、テスト内で使用されるメソッドの仕様は以下。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| TestProjectRegistry::new | fn new(registry_file: PathBuf) -> Self | レジストリの初期化 | O(1) | O(1) |
| TestProjectRegistry::register_project | fn register_project(&self, project_path: &Path) -> Result<String, Box<dyn std::error::Error>> | プロジェクト登録とJSONへの保存 | O(n) | O(n) |
| TestProjectRegistry::find_project_by_id | fn find_project_by_id(&self, id: &str) -> Option<serde_json::Value> | ID検索でエントリ取得 | O(n) | O(n) |

n は JSON内のプロジェクト数。

### TestProjectRegistry::new

1. 目的と責務
   - レジストリファイルパスを受け取り、`TestProjectRegistry` インスタンスを生成。

2. アルゴリズム（ステップ）
   - 引数の `PathBuf` をフィールドへ格納して `Self` を返す。

3. 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| registry_file | PathBuf | Yes | レジストリファイルのパス |

4. 戻り値

| 型 | 説明 |
|----|------|
| Self | 初期化済みのレジストリ |

5. 使用例
```rust
let (_temp_dir, registry_file) = setup_test_env();
let registry = TestProjectRegistry::new(registry_file);
```

6. エッジケース
- 特になし（コピー可能なPathBufを保持するのみ）。

行番号根拠: 関数定義は本ファイル内。行番号は不明（チャンクに行番号情報なし）。

### TestProjectRegistry::register_project

1. 目的と責務
   - プロジェクトID生成、プロジェクト名抽出、JSONファイルへの追記保存、ID返却。

2. アルゴリズム（ステップ分解）
   - 現在時刻ナノ秒＋プロセスIDから擬似一意IDを生成。
   - `project_path` から末尾のディレクトリ名を抽出（UTF-8でない場合は `unknown`）。
   - JSONエントリ `{id, name, path}` を作成。
   - レジストリファイルが存在すれば読み込み・パース（失敗時は空配列へフォールバック）。存在しなければ空配列を作成。
   - 配列へエントリを push。
   - pretty JSON を書き戻し。
   - 生成したIDを返却。

3. 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| self | &Self | Yes | レジストリ参照 |
| project_path | &Path | Yes | 登録するプロジェクトのパス |

4. 戻り値

| 型 | 説明 |
|----|------|
| Result<String, Box<dyn std::error::Error>> | 成功時は生成ID、失敗時はIO/シリアライズ等のエラー |

5. 使用例
```rust
let project_path = PathBuf::from("/Users/test/my-project");
let project_id = registry.register_project(&project_path)?;
println!("Registered id = {project_id}");
```

6. エッジケース
- SystemTimeがUNIX_EPOCHより前の場合、`duration_since` の `unwrap` がpanic。
- 既存ファイルのJSONが配列でない正当なJSONの場合、`as_array_mut().unwrap()` がpanic。
- 非UTF-8のパス末尾は `unknown` になる。
- 大規模データでは配列追加・全体書き戻しに伴うI/Oコスト増。
- 複数プロセスからの同時書き込みで競合（ファイルロックなし）。

行番号根拠: 関数名は本ファイル記載。正確な行番号は不明（チャンクに行番号情報なし）。

抜粋コード（主要部）:
```rust
fn register_project(&self, project_path: &Path) -> Result<String, Box<dyn std::error::Error>> {
    /* ID生成 */
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let project_id = format!("test-id-{}-{}", timestamp, std::process::id());
    let project_name = project_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown")
        .to_string();

    /* エントリ作成 */
    let entry = json!({
        "id": project_id,
        "name": project_name,
        "path": project_path.to_string_lossy()
    });

    /* 既存読み込み・フォールバック */
    let mut projects = if self.registry_file.exists() {
        let content = std::fs::read_to_string(&self.registry_file)?;
        serde_json::from_str(&content).unwrap_or_else(|_| json!([]))
    } else {
        json!([])
    };

    /* 追記と書き戻し */
    projects.as_array_mut().unwrap().push(entry);
    std::fs::write(
        &self.registry_file,
        serde_json::to_string_pretty(&projects)?,
    )?;

    Ok(project_id)
}
```

### TestProjectRegistry::find_project_by_id

1. 目的と責務
   - レジストリファイルからID一致のプロジェクトを検索して返す。

2. アルゴリズム（ステップ分解）
   - レジストリファイルの存在チェック。なければ `None`。
   - ファイル読み込み・JSONパース。失敗すれば `None`。
   - 配列でなければ `None`。
   - `id` キーが一致する要素を線形検索して `Some(value)` を返す。見つからなければ `None`。

3. 引数

| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| self | &Self | Yes | レジストリ参照 |
| id | &str | Yes | 検索対象ID |

4. 戻り値

| 型 | 説明 |
|----|------|
| Option<serde_json::Value> | 見つかれば該当エントリ、なければ `None` |

5. 使用例
```rust
if let Some(project) = registry.find_project_by_id(&project_id) {
    println!("Found path: {}", project["path"]);
}
```

6. エッジケース
- ファイル非存在 → `None`。
- パース失敗 → `None`。
- JSONが配列でない → `None`。
- キー欠落（例えば `"id"` がない）→ 見つからない扱いで `None`。

関数全文（<=20行）:
```rust
fn find_project_by_id(&self, id: &str) -> Option<serde_json::Value> {
    if !self.registry_file.exists() {
        return None;
    }

    let content = std::fs::read_to_string(&self.registry_file).ok()?;
    let projects: serde_json::Value = serde_json::from_str(&content).ok()?;

    projects.as_array()?.iter().find(|p| p["id"] == id).cloned()
}
```

## Walkthrough & Data Flow

- test_register_project_creates_entry の流れ
  1. `setup_test_env` で TempDir と `projects.json` パスを取得。
  2. `TestProjectRegistry::new` でレジストリを初期化。
  3. `register_project` へ `project_path = "/Users/test/my-project"` を渡して登録。
  4. 成功結果のIDの非空性と最低長（>=32）を検証。
  5. ファイル存在を検証。
  6. `find_project_by_id` でエントリを読み取り、`path` と `name` を検証。
  7. 存在しないIDで `None` を確認。

- test_update_project_path の流れ
  1. 初期登録（`original-location`）。
  2. ファイル内容を読み込み、JSONを直接編集して `path` と `name` を変更（`new-location`）。
  3. 書き戻し後、`find_project_by_id` で更新が反映されていることを検証。

データフロー（register_project 主要経路）
- 入力: `&Path` → 名称抽出 → JSONエントリ
- ファイル存在チェック → 読み込み+パース（失敗時 []）→ 配列へ追加 → JSON整形文字列へシリアライズ → 書き込み
- 出力: 生成ID

注記: 正確な行番号は不明（チャンクに行番号情報なし）。

## Complexity & Performance

- register_project
  - 時間計算量: O(n)（JSON配列の読み込み・パース・整形・書き戻し）
  - 空間計算量: O(n)（配列構築とシリアライズバッファ）
  - ボトルネック: ファイルI/O、JSONの全体再シリアライズ
  - スケール限界: エントリ数が増えると毎回線形に遅くなる。数万〜数十万件規模で顕著。

- find_project_by_id
  - 時間計算量: O(n)（線形探索）
  - 空間計算量: O(n)（パースしたJSON保持）
  - ボトルネック: ファイルI/O、線形検索
  - スケール限界: 大規模配列で検索が遅い。

実運用負荷要因
- I/O: 毎操作でファイル全体を読み書き。
- フォーマット: JSONは人間可読だが、更新毎に全体再シリアライズが必要。
- 並行性: ロックなしで同時更新すると破損の可能性。

## Edge Cases, Bugs, and Security

- メモリ安全性
  - Rustの安全な標準APIのみ使用。`unsafe` は「該当なし」。
  - `String`/`PathBuf` の所有権・借用は適切（所有権移動は `new(registry_file: PathBuf)` でフィールド保持、借用は `&Path` を受け取る）。
  - ライフタイムパラメータは不要。

- Panic/unwrap
  - `SystemTime::now().duration_since(UNIX_EPOCH).unwrap()` によるpanic可能性（極端な時刻設定）。
  - `projects.as_array_mut().unwrap()` は配列でない正当JSONでpanic。
  - テスト内 `expect`/`unwrap` は失敗時にテストをpanicさせる。

- インジェクション
  - SQL/Command/Path Traversal: 該当なし。ファイルパスは読み書き先固定（TempDir直下の `projects.json`）。
  - JSONインジェクション: 値はシリアライズされるため構文破壊されないが、信頼できない入力ソースから来る場合は検証が必要。

- 認証・認可
  - 該当なし（テストでのみファイル操作）。

- 秘密情報
  - ハードコード秘密情報: 該当なし。
  - ログ漏えい: テスト中にパスやIDを `println!` するが、機微情報ではない前提。

- 並行性
  - TempDirによりテスト間衝突は低リスク。
  - 同一ファイルに対する並行更新の保護なし（ロック・原子的書き込みなし）→ 実運用ならレース・破損の可能性。

- ファイル整合性
  - パース失敗時に空配列へフォールバックするため、壊れたJSONは上書きで「消失」する可能性（データロスのリスク）。

### Edge Cases詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| SystemTimeが過去（UNIX_EPOCH未満） | OS時刻が1970年以前 | エラー戻り or 安全なフォールバック | `unwrap`でpanic | 要改善 |
| 既存JSONが配列でない | `{"projects": [...]}` | エラー扱い or スキーマ変換 | `as_array_mut().unwrap()`でpanic | 要改善 |
| 既存JSONが壊れている | `"[invalid]"` | エラー戻り/復旧手順提示 | 空配列へフォールバックして上書き | 仕様要検討 |
| 非UTF-8のパス末尾 | `OsStr`が非UTF-8 | 名前抽出不可→適切な代替名 | `"unknown"` に設定 | 許容 |
| 非存在ID検索 | `"nonexistent..."` | `None` | `None` | OK |
| 大量エントリ | 数十万件 | 高速検索・部分更新 | 線形探索＋全体書き戻し | スケール不向き |
| 並行更新 | 複数プロセス/スレッド | 一貫性維持 | ロックなし | 要改善 |

### Rust特有の観点（詳細チェックリスト）

- 所有権
  - `new(registry_file: PathBuf)` で値の所有権を構造体に移動（関数: new, 行番号不明）。
  - `register_project(&self, project_path: &Path)` は借用参照を使用し、データ競合なし。

- 借用
  - 可変借用は `projects.as_array_mut()` の返す一時的な可変参照のみ。スコープ内で完結。

- ライフタイム
  - 明示的ライフタイムは不要。返却値は `String` と `serde_json::Value`（所有型）。

- unsafe境界
  - 使用箇所: なし。
  - 不変条件/安全性根拠: 全て安全API。

- 並行性・非同期
  - `Send/Sync`: `TestProjectRegistry` は `PathBuf` のみ保持で問題なし（理論上は `Send/Sync`）。ただし本テストでは同期/非同期を扱わない。
  - データ競合: 単一ファイルアクセスにロックなし。並行時は競合しうる。
  - await境界/キャンセル: 該当なし。

- エラー設計
  - `register_project` は `Result<String, Box<dyn Error>>` を返すが、内部で `unwrap` を使用しており、エラーがResultに乗らない箇所がある。
  - `find_project_by_id` は `Option` を返す合理的設計。
  - `unwrap/expect` の妥当性：テストコードでは許容だが、ライブラリコードでは避けるべき。
  - エラー変換: `Box<dyn Error>` は汎用だが詳細化に乏しい。

## Design & Architecture Suggestions

- **ID生成の堅牢化**: `uuid` クレートなどで適切なUUIDを生成。テスト依存を避けたいなら簡易的でも衝突をより低減する方式（ランダム＋時刻）を採用。
- **JSONスキーマの型化**: `struct Project { id: String, name: String, path: String }` を定義し、`Vec<Project>` でやり取り。`serde` の `Deserialize/Serialize` を用い、パース失敗時の挙動を明示。
- **エラー伝播の一貫化**: `unwrap` を排除し、`thiserror` などで専用エラー型を整備。
- **ファイル更新の原子性**: 一時ファイルへ書いて `rename` する、あるいは `fsync`、`atomicwrites` 的な手法で破損防止。
- **ロック導入**: OSファイルロックやプロセス内ミューテックス（`parking_lot` 等）で並行更新保護。
- **スケーラビリティ**: 線形探索を改善（ID→エントリのハッシュインデックスを別途保持）、もしくはSQLiteなどの軽量DBへ移行。
- **フォーマット検証**: 非配列JSONを検出してエラーにする。フォールバック時は警告ログを出す。

## Testing Strategy (Unit/Integration) with Examples

既存テストは基本ケースを網羅。追加するとよいテスト:

- パース失敗時の挙動（現在は空配列へフォールバックし上書きする）
```rust
#[test]
fn test_parse_failure_fallback() {
    let (_temp_dir, registry_file) = setup_test_env();
    std::fs::write(&registry_file, "not-a-json").unwrap();

    let registry = TestProjectRegistry::new(registry_file.clone());
    let id = registry.register_project(Path::new("/Users/test/a")).unwrap();

    // 破損JSONから空配列にフォールバックして新規エントリが作られる
    let project = registry.find_project_by_id(&id);
    assert!(project.is_some());
}
```

- 非配列JSONの扱い（現状はpanicするはず）
```rust
#[test]
#[should_panic]
fn test_non_array_json_panics() {
    let (_temp_dir, registry_file) = setup_test_env();
    std::fs::write(&registry_file, r#"{"projects":[]}"#).unwrap();
    let registry = TestProjectRegistry::new(registry_file.clone());
    // as_array_mut().unwrap() で panic
    let _ = registry.register_project(Path::new("/Users/test/a")).unwrap();
}
```

- 非UTF-8パス末尾の処理
```rust
#[test]
fn test_non_utf8_name() {
    use std::ffi::OsString;
    let (_temp_dir, registry_file) = setup_test_env();
    let mut os = OsString::from("/Users/test/");
    os.push(OsString::from_vec(vec![0xFF, 0xFE, 0xFD])); // 非UTF-8
    let registry = TestProjectRegistry::new(registry_file.clone());
    let id = registry.register_project(Path::new(&os)).unwrap();
    let project = registry.find_project_by_id(&id).unwrap();
    assert_eq!(project["name"], "unknown");
}
```

- 競合書き込み（同一ファイルへ連続登録。レースは難しいが単純並列で再現度を上げる）
```rust
#[test]
fn test_concurrent_like_writes() {
    let (_temp_dir, registry_file) = setup_test_env();
    let registry = TestProjectRegistry::new(registry_file.clone());

    let ids: Vec<_> = (0..10)
        .map(|i| registry.register_project(Path::new(&format!("/p/{i}"))).unwrap())
        .collect();

    for id in ids {
        assert!(registry.find_project_by_id(&id).is_some());
    }
}
```

- Windows風パス（環境依存だが、末尾名抽出の互換性）
```rust
#[test]
fn test_windows_like_path_name_extraction() {
    let (_temp_dir, registry_file) = setup_test_env();
    let registry = TestProjectRegistry::new(registry_file.clone());
    let id = registry.register_project(Path::new("C:/Users/test/my-project")).unwrap();
    let project = registry.find_project_by_id(&id).unwrap();
    assert_eq!(project["name"], "my-project");
}
```

## Refactoring Plan & Best Practices

1. 型定義の導入
   - `#[derive(Serialize, Deserialize)] struct Project { id, name, path }`
   - レジストリ形式を `Vec<Project>` に固定。

2. エラー型の定義
   - `thiserror` で `enum RegistryError`（Io/Parse/Schema/Time等）を用意。
   - 全ての失敗箇所で `Result` へ変換（`?` を活用）。

3. ID生成の改善
   - `uuid::Uuid::new_v4().to_string()` の採用（テスト内でも許容）。

4. JSON読み書きの堅牢化
   - 非配列JSONなら `Err(RegistryError::Schema)` を返し、上書きしない。
   - 原子的書き込み（`write to temp` → `rename`）。

5. ロック
   - `fs2::FileExt::lock_exclusive()` 等でロック、またはアプリ側で `Mutex` 保護。

6. パフォーマンス
   - インデックスファイル（ID→オフセット）を用いて部分更新。
   - 代替としてSQLiteなどのKV/表形式DBを採用。

7. 観測可能性
   - ログ出力（info/debug/warn）と失敗時の詳細メッセージ。
   - メトリクス（登録回数、失敗回数、I/O時間）。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - 登録成功: ID/名前/パスをinfoで記録（テストではprintln、実運用ではロガー使用）。
  - パース失敗・スキーマ不一致: warn/errorで記録し、データロスを防ぐ意思決定に役立てる。
- メトリクス
  - `counter`: 登録成功/失敗回数。
  - `histogram`: 読み込み・書き込み時間、JSONシリアライズ時間。
  - `gauge`: レジストリのエントリ数。
- トレーシング
  - ファイルI/Oのスパンを記録し、遅延のボトルネック可視化。

## Risks & Unknowns

- 実際の `ProjectRegistry` 実装仕様は「不明」。本テストはモックの振る舞いに依存。
- データスキーマ（`projects.json` が配列確定か、オブジェクト型か）は「不明」。
- 複数プロセス/スレッド運用時の一貫性要件・ロック方針は「不明」。
- Windows/Unix混在環境でのパス抽出ルールの違いは「不明」。
- IDのフォーマット要件（真のUUID必要か否か）は「不明」。

以上の不確定要素はプロダクト要件の確認と仕様化が必要です。