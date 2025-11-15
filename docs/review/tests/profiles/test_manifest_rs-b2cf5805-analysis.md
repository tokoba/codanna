# profiles\test_manifest.rs Review

## TL;DR

- 目的: ProfileManifest::from_jsonでのJSONパースとバリデーションの挙動を4つのテストで検証（空name/空versionの拒否、空ファイルパスのスキップ、最小構成の成功）。
- 主要公開API: ProfileManifest::from_json（推定シグネチャ: fn(&str) -> Result<ProfileManifest, E>）。フィールド name, version, files が外部から参照可能であることを確認。
- 複雑箇所: エラー内容の妥当性検証（err_msg.contains("name"/"version")）にとどまり、詳細なエラー型やバリアントは不明。
- 重大リスク: 無効JSON・フィールド欠落・空白のみのファイルパス・パストラバーサルなどのセキュリティ/堅牢性ケースが未カバー。
- Rust安全性: テストにunsafeはなく、unwrapは成功ケースのみで使用。失敗ケースはis_errを先行確認してunwrap_err（安全）。
- 並行性: 該当なし（同期テストのみ）。
- パフォーマンス: 文字列パースと配列フィルタ程度でO(n)。本テストではボトルネックなし。

## Overview & Purpose

このファイルは、プロファイルマニフェストのJSON文字列を解析するProfileManifest::from_jsonの基本的なバリデーション挙動を保証するための単体テスト群です。

テスト内容（根拠の行番号付き）:
- 最小プロファイルの正常パース（test_parse_minimal_profile_manifest: L5-L17）
- 空のnameを拒否（test_reject_empty_name: L19-L32）
- 空のversionを拒否（test_reject_empty_version: L34-L47）
- files配列内の空文字列パスをスキップ（test_skip_empty_file_paths: L49-L61）

これにより、最低限のデータ契約（name, versionの非空、filesの空要素除去）が満たされることを確認します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_parse_minimal_profile_manifest | private（テスト） | 正常系の最小JSONをパースしフィールドを検証 | Low |
| Function | test_reject_empty_name | private（テスト） | nameが空文字の場合にエラーを返すことを検証 | Low |
| Function | test_reject_empty_version | private（テスト） | versionが空文字の場合にエラーを返すことを検証 | Low |
| Function | test_skip_empty_file_paths | private（テスト） | files配列内の空文字をフィルタし、非空のみ残ることを検証 | Low |
| Struct（外部） | ProfileManifest | pub（利用前提） | マニフェストモデル。name, version, filesフィールドにアクセス（L14-L16, L58-L60） | Med |
| Associated Function（外部） | ProfileManifest::from_json | pub（利用前提） | JSON文字列からProfileManifestを構築し、バリデーション | Med |

### Dependencies & Interactions

- 内部依存:
  - 各テスト関数はProfileManifest::from_jsonを呼び出し、Resultを評価する。
  - 標準マクロassert_eq!, assert!使用。

- 外部依存（推定含む）:

| 依存 | 種別 | 用途 | 備考 |
|------|------|------|------|
| codanna::profiles::manifest::ProfileManifest | クレート内モジュール | モデルとパーサAPI | フィールドアクセスが可能（pub推定） |
| std（標準） | マクロ | assert!, assert_eq! | Rust標準 |
| serde/serde_json | 不明 | JSONパース | このチャンクには現れない（推定のみ） |

- 被依存推定:
  - プロファイル読み込み機能全般、設定適用ロジック、ファイル収集処理などから利用される可能性あり（不明）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ProfileManifest::from_json | fn from_json(json: &str) -> Result<ProfileManifest, E> | JSONからマニフェストを構築しバリデーション | O(n) | O(n) |

詳細（このチャンクから観測可能な範囲のみ）:

1. 目的と責務
   - JSON文字列を解析してProfileManifestを返す。
   - nameとversionの非空を検証（根拠: 空name/空versionでErrを返すテスト L27-L31, L42-L46）。
   - files配列から空文字要素を除去（根拠: L57-L60）。

2. アルゴリズム（テストからの推定）
   - 入力JSON文字列をパース。
   - name, versionの存在と非空検証。
   - files: 配列要素のうち空文字列をフィルタリング（trimの有無は不明）。
   - 検証が通ればProfileManifestを返し、違反時はエラーを返す。

3. 引数

| 名前 | 型 | 必須 | 制約 |
|------|----|------|------|
| json | &str | はい | JSONとして妥当であること。スキーマはname, version, files（配列）含む（不明部分あり） |

4. 戻り値

| ケース | 型 | 内容 |
|--------|----|------|
| Ok | ProfileManifest | name, version, filesが妥当なモデル（フィールドアクセス可能: L14-L16, L58-L60） |
| Err | E（不明） | nameやversionのエラー時にはメッセージに該当フィールド名が含まれる（L30-L31, L45-L46） |

5. 使用例

```rust
use codanna::profiles::manifest::ProfileManifest;

let json = r#"{
    "name": "codanna",
    "version": "1.0.0",
    "files": ["README.md", ""]
}"#;

let manifest = ProfileManifest::from_json(json).unwrap();
assert_eq!(manifest.name, "codanna");
assert_eq!(manifest.version, "1.0.0");
// 空ファイルパスは除外される
assert_eq!(manifest.files, vec!["README.md"]);
```

6. エッジケース
- 無効なJSON（構文エラー）：不明（このチャンクには現れない）。
- 欠落フィールド（name/version/files）：不明。
- 空白のみのname/version（"  "）：不明。
- files要素が空白のみ（"  "）：不明。
- 重複ファイルパス：不明。
- パストラバーサル（"../secret"）：不明。

データ契約（ProfileManifestのフィールド、テストからの観測）
- name: String（非空であるべき）根拠: L14, L27-L31
- version: String（非空であるべき）根拠: L15, L42-L46
- files: Vec<String>（空文字が除外される）根拠: L16, L58-L60

## Walkthrough & Data Flow

- test_parse_minimal_profile_manifest（L5-L17）
  1. 最小JSON（name, version, 空files配列）を準備（L7-L11）。
  2. from_jsonでパースしてunwrapで成功を前提（L13）。
  3. フィールドの値を検証（name/versionが指定値、files.len() == 0）（L14-L16）。

- test_reject_empty_name（L19-L32）
  1. nameが空文字のJSONを準備（L21-L25）。
  2. from_jsonを呼び出してResultを受け取る（L27）。
  3. is_errで失敗であることを検証（L28）。
  4. unwrap_errでエラーを取り出し、文字列化したメッセージに"name"が含まれることを検証（L30-L31）。

- test_reject_empty_version（L34-L47）
  1. versionが空文字のJSONを準備（L36-L40）。
  2. from_jsonを呼び出してResultを受け取る（L42）。
  3. is_errで失敗を検証（L43）。
  4. unwrap_errでエラーを取り出し、メッセージに"version"が含まれることを検証（L45-L46）。

- test_skip_empty_file_paths（L49-L61）
  1. filesに空文字を含むJSONを準備（L51-L55）。
  2. from_jsonでパースしてunwrap（L57）。
  3. filesの要素数が2（空文字が除外）であること、および順序保持を検証（L58-L60）。

データフロー要点:
- 入力: &str JSON → 解析（from_json）→ Result<ProfileManifest, E>
- 正常系では所有データ（String, Vec<String>）で返却され、テストからフィールド参照
- 異常系ではメッセージに該当フィールド名が含まれる

## Complexity & Performance

- from_jsonの時間計算量: O(n)（nは入力JSON文字列長）。配列フィルタも要素数mに対してO(m)。
- 空間計算量: O(n)（パース結果の構築とstringsの所有）。
- ボトルネック: 本テストスコープではなし。大規模files配列や巨大JSON時にはメモリ使用が増加。
- 実運用負荷要因: I/O（ファイル読み込み）やネットワーク、DBはこのチャンクには現れない。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価（このチャンクから判定可能な範囲）:
- メモリ安全性: unsafe未使用（不明だがテスト側では使用なし）。unwrapは正常系のみ、異常系はis_err確認後のunwrap_errで安全（L28→L30, L43→L45）。
- インジェクション:
  - SQL/Command: 該当なし。
  - Path traversal: filesがパスを含む想定。検査は不明。攻撃ベクトル（"../", 絶対パス）は未テスト。
- 認証・認可: 該当なし。
- 秘密情報: ハードコード秘密なし。ログ漏えいは不明。
- 並行性: レース/デッドロックの懸念なし（同期テスト）。

Rust特有の観点:
- 所有権: ProfileManifestは所有データ（String/Vec<String>）を返すと推定（フィールド参照が可能なため）。このチャンクには現れない詳細は不明。
- 借用/ライフタイム: 明示的ライフタイムは不要と推定。from_jsonは&str入力→所有データ出力。
- unsafe境界: テストコードにunsafeなし。実装側は不明。
- 並行性・非同期: Send/Sync要件は不明。async/awaitの使用なし。
- エラー設計: Resultで返す。具体的なエラー型やFrom/Intoは不明。panic箇所はテストのunwrapのみ（正常系で使用）。

詳細化されたエッジケース一覧:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空name | `"name": ""` | Err（メッセージに"name"） | test_reject_empty_nameで検証（L27-L31） | カバー済み |
| 空version | `"version": ""` | Err（メッセージに"version"） | test_reject_empty_versionで検証（L42-L46） | カバー済み |
| filesの空文字 | `["a", "", "b", ""]` | 空文字は除外し["a","b"] | test_skip_empty_file_pathsで検証（L58-L60） | カバー済み |
| 無効JSON | `{ "name": "x" }`（カンマ欠落など） | Err（構文エラー） | 不明 | このチャンクには現れない |
| 欠落フィールド（filesなし） | `{ "name": "x", "version": "1.0.0" }` | Errまたはデフォルト | 不明 | このチャンクには現れない |
| 空白のみのname | `"name": "   "` | Err（trimして空扱い） | 不明 | このチャンクには現れない |
| 空白のみのfiles要素 | `"files": ["  "]` | 除外またはErr | 不明 | このチャンクには現れない |
| パストラバーサル | `"files": ["../secret"]` | Errまたは拒否 | 不明 | このチャンクには現れない |
| 重複パス | `"files": ["README.md","README.md"]` | 重複許可/排除 | 不明 | このチャンクには現れない |

## Design & Architecture Suggestions

- エラー型の明確化:
  - エラーを型安全に扱うため、ManifestError（例）にバリアント（EmptyName, EmptyVersion, InvalidFilesなど）を定義し、to_stringに頼らずパターンマッチ可能にする。
- パースと検証の分離:
  - ProfileManifest::from_jsonはパース専用にし、validate(&self)を別途用意すると責務分離が明瞭。
- データ正規化:
  - filesの空白トリム、重複排除、パス正規化（canonicalizeはI/Oを伴うためポリシー次第）を検討。
- バージョン厳格化:
  - semverとしての妥当性検証を導入（このチャンクには現れないが、要件次第）。
- スキーマ定義:
  - JSONスキーマに準拠する設計（name/version必須、filesは配列、要素は非空）を明文化。

## Testing Strategy (Unit/Integration) with Examples

追加テストの提案（補強すべき境界条件とセキュリティ観点）:

- 無効JSONの検証
```rust
#[test]
fn test_invalid_json_fails() {
    let json = r#"{ "name": "codanna", "version": "1.0.0", "files": [ "a" ]"#; // 末尾の } が欠落
    let result = ProfileManifest::from_json(json);
    assert!(result.is_err());
}
```

- 欠落フィールドの検証
```rust
#[test]
fn test_missing_fields() {
    // files欠落
    let json = r#"{ "name": "codanna", "version": "1.0.0" }"#;
    let result = ProfileManifest::from_json(json);
    assert!(result.is_err()); // 仕様に応じて変更

    // name欠落
    let json2 = r#"{ "version": "1.0.0", "files": [] }"#;
    assert!(ProfileManifest::from_json(json2).is_err());

    // version欠落
    let json3 = r#"{ "name": "codanna", "files": [] }"#;
    assert!(ProfileManifest::from_json(json3).is_err());
}
```

- 空白のみのname/version/files要素
```rust
#[test]
fn test_whitespace_only_values() {
    let json = r#"{ "name": "   ", "version": "1.0.0", "files": [] }"#;
    assert!(ProfileManifest::from_json(json).is_err());

    let json2 = r#"{ "name": "codanna", "version": "   ", "files": [] }"#;
    assert!(ProfileManifest::from_json(json2).is_err());

    let json3 = r#"{ "name": "codanna", "version": "1.0.0", "files": ["   ", "README.md"] }"#;
    let manifest = ProfileManifest::from_json(json3).unwrap();
    // 仕様により: "   " を除外するなら
    assert_eq!(manifest.files, vec!["README.md"]);
}
```

- パストラバーサルの検証
```rust
#[test]
fn test_reject_path_traversal() {
    let json = r#"{ "name": "codanna", "version": "1.0.0", "files": ["../secret", "/etc/passwd"] }"#;
    let result = ProfileManifest::from_json(json);
    // ポリシーに応じてErrを期待
    assert!(result.is_err());
}
```

- 重複パスの扱い
```rust
#[test]
fn test_duplicate_files_handling() {
    let json = r#"{ "name": "codanna", "version": "1.0.0", "files": ["README.md", "README.md"] }"#;
    let manifest = ProfileManifest::from_json(json).unwrap();
    // 仕様次第: 重複排除なら1件、許容なら2件
    // assert_eq!(manifest.files.len(), 1);
    // assert_eq!(manifest.files.len(), 2);
}
```

- プロパティベーステスト（proptest等）による堅牢性検証（このチャンクには現れないが有用）
  - ランダム生成したJSONでパニックしないこと、エラーは型安全に返ること。

## Refactoring Plan & Best Practices

- テストの重複削減:
  - ヘルパー関数で「Errに期待文字が含まれるか」を共通化。
```rust
fn assert_err_contains(json: &str, needle: &str) {
    let result = ProfileManifest::from_json(json);
    assert!(result.is_err(), "expected error but got Ok");
    let msg = result.unwrap_err().to_string();
    assert!(msg.contains(needle), "error message missing '{}': {}", needle, msg);
}
```

- テーブル駆動テスト:
  - 空name/空version/空filesのケースを配列にし、ループで検証。
- 正常系/異常系の分離:
  - 正常系はunwrapを使い、異常系はis_err→unwrap_errの順守で明確化。
- 期待仕様の明文化:
  - 空白トリム、重複排除、パス検査などの仕様をコメントで明記するとテストの意図が明瞭。

## Observability (Logging, Metrics, Tracing)

- ログ:
  - from_json内での失敗原因を構造化（フィールド名、理由、位置）してログに残せる設計が望ましい。
- メトリクス:
  - パース成功/失敗のカウンタ、失敗理由の分類。
- トレーシング:
  - 大規模な読み込みフロー内で、マニフェストパースのスパンを追加（このチャンクには現れない）。

## Risks & Unknowns

- エラー型と詳細バリアント: 不明（このチャンクには現れない）。現状は文字列検索で検証。
- JSONパーサの選定（serde_json等）: 不明。
- 空白の扱い（trim有無）: 不明。
- filesの仕様（重複、相対/絶対パス、正規化）: 不明。
- バージョンフォーマット（semver厳格性）: 不明。
- 大規模入力時のパフォーマンス/メモリ: 不明だが一般的にはO(n)。
- 並行利用時のスレッド安全性（Send/Sync）: 不明。