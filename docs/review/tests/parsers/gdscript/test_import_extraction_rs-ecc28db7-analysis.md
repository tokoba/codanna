# parsers\gdscript\test_import_extraction.rs Review

## TL;DR

- 目的: GDScriptの**import抽出**（extends、class_name、preload）ロジックの検証を行うテスト3件を提供。
- 公開API: このファイル自体に**公開APIはなし**。外部の**GdscriptParser::new**と**find_imports**、**FileId::new**を使用。
- コアロジック: 各テストでコード文字列→パーサ作成→**find_imports**→結果検証の流れを一貫して確認。
- 複雑箇所: 明示的な複雑ロジックはなし。テストは直線的。出力フィールドのうち**is_type_only**は表示のみで未検証。
- 重大リスク: テストが**is_type_only**や**alias**の振る舞いをアサートしておらず、仕様逸脱の検知漏れの可能性。
- Rust安全性: 例外処理に**expect/unwrap**を使用し、失敗時はテストがパニック。並行性要素は**なし**。
- パフォーマンス: テスト内の探索は**O(n)**。パーサの計算量はこのチャンクでは**不明**。

## Overview & Purpose

このファイルは、codannaクレートに含まれるGDScriptパーサ（GdscriptParser）がコードから**インポート情報**を抽出する機能をテストするためのRustテストモジュールです。対象インポートは以下の3種類です。

- extends（継承対象クラス名の抽出）
- class_name（クラス名の抽出。グローバル公開扱い）
- preload（リソースパスの抽出）

各テストは最小限のGDScriptコードスニペットを用いて、抽出結果（件数とフィールド）を**assert**で検証します。

このファイル自体はテスト専用であり、公開APIの定義はありません。外部クレートの**codanna::parsing::gdscript::GdscriptParser**と**LanguageParser**トレイト、および**FileId**を用いています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_gdscript_extends_import_extraction | private(test) | extendsとclass_nameの抽出を検証 | Low |
| Function | test_gdscript_preload_import_extraction | private(test) | preloadによるリソースパス抽出を検証 | Low |
| Function | test_gdscript_mixed_imports | private(test) | extends、class_name、preloadの混在ケースを検証 | Low |

### Dependencies & Interactions

- 内部依存
  - なし（各テスト間でヘルパー共有はなく、重複した初期化コードあり）

- 外部依存（推奨表）

| 依存 | モジュール/型 | 用途 | 備考 |
|-----|---------------|------|------|
| FileId | codanna::FileId | ファイル識別子の生成 | `FileId::new(1).unwrap()`でID作成 |
| LanguageParser | codanna::parsing::LanguageParser | パーサの共通トレイト（推定） | `find_imports`呼び出しの根拠 |
| GdscriptParser | codanna::parsing::gdscript::GdscriptParser | GDScriptのインポート抽出 | `new()`と`find_imports`使用 |
| 標準ライブラリ | println!, assert!, assert_eq! | ログ出力と検証 | テスト標準 |

- 被依存推定
  - このテストモジュールは**テストハーネス**からのみ実行され、他モジュールから再利用されません。

## API Surface (Public/Exported) and Data Contracts

- 公開API一覧（このファイル内）: 該当なし

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| 該当なし | 該当なし | 該当なし | 該当なし | 該当なし |

- 外部API（使用）のデータ契約（このチャンクから読み取れる範囲）
  - GdscriptParser::new(): Result<T, E>を返し、テストでは`expect("Failed to create parser")`で成功を前提（型詳細は不明）
  - LanguageParser::find_imports(code, file_id): Vec<Importレコード>を返す（型名は不明）。少なくとも以下のフィールドが存在
    - import.path: 文字列（例: "Node2D", "Player", "res://scripts/enemy.gd"）
    - import.alias: Debug表示可能な型（恐らくOption<...>だが型は不明）
    - import.is_glob: bool（class_nameがtrueになることを期待）
    - import.is_type_only: bool（混在テストで出力のみ、意味は不明）
  - FileId::new(u64/usize?): Result<FileId, E>を返す（具体的型は不明）。テストでは`unwrap()`で成功を前提

各APIの詳細説明: このチャンクに公開APIは存在しないため、詳細は該当なし。

## Walkthrough & Data Flow

- test_gdscript_extends_import_extraction（行番号:不明）
  1. GDScriptコードを組み立て（extends Node2D, class_name Player, _ready関数）
  2. `GdscriptParser::new().expect(...)`でパーサ生成
  3. `FileId::new(1).unwrap()`でファイルID生成
  4. `parser.find_imports(code, file_id)`でインポート抽出
  5. 結果の件数と中身（path, alias, is_glob）を出力
  6. `assert_eq!(imports.len(), 2)`で件数検証
  7. `imports.iter().find(|i| i.path == "Node2D")`でextends検出、`is_glob == false`を検証
  8. `imports.iter().find(|i| i.path == "Player")`でclass_name検出、`is_glob == true`を検証

- test_gdscript_preload_import_extraction（行番号:不明）
  1. preloadを2件持つGDScriptコードを組み立て
  2. パーサ生成とFileId生成
  3. `find_imports`実行
  4. `assert_eq!(imports.len(), 2)`で件数検証
  5. `"res://scripts/enemy.gd"`と`"res://items/weapon.gd"`のpathを検索し、存在を検証

- test_gdscript_mixed_imports（行番号:不明）
  1. extends, class_name, preloadが混在するコードを組み立て
  2. パーサ生成とFileId生成
  3. `find_imports`実行
  4. 結果の各インポートについて`path`, `is_glob`, `is_type_only`を出力
  5. `assert_eq!(imports.len(), 3)`で件数検証
  6. `"CharacterBody2D"`（extends）、`"Enemy" && is_glob == true`（class_name）、`"res://projectiles/bullet.gd"`（preload）の存在を個別に検証

データフロー観点
- 入力: GDScriptの**コード文字列**と**FileId**
- 処理: `GdscriptParser::find_imports`が文字列を解析してインポートレコードの**Vec**を返す
- 出力: `Vec<Import>`相当から`path/is_glob/...`フィールドを参照したアサーション

## Complexity & Performance

- テスト内計算量
  - インポート配列からの探索（`iter().find` / `iter().any`）は**O(n)**、n=抽出されたインポート件数
  - 追加メモリはインポート配列の保持に比例（**O(n)**）
- パーサの計算量
  - `find_imports`の内部アルゴリズムはこのチャンクでは**不明**
- 実運用負荷要因（このファイルの範囲）
  - I/O/ネットワーク/DBアクセスは**なし**（純粋に文字列解析）
  - ログ出力（println!）はテスト時の標準出力のみ

## Edge Cases, Bugs, and Security

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | 0件のインポート | このチャンクには現れない | 未検証 |
| extendsのみ | "extends Node2D" | Node2Dの1件のみ抽出、glob=false | 部分的に類似（extends+class_name）あり | 未検証（単独ケース） |
| class_nameのみ | "class_name Enemy" | Enemyの1件抽出、glob=true | このチャンクには現れない | 未検証 |
| preloadのみ | r#"const A = preload("res://a.gd")"# | "res://a.gd"の1件抽出 | 類似（2件preload）あり | 検証済み（複数件） |
| 無効構文 | "clas_name X"など | 0件抽出またはエラー | このチャンクには現れない | 未検証 |
| 重複インポート | 同一pathが複数 | 重複扱いの方針（重複許容/除去） | このチャンクには現れない | 未検証 |
| 別API読み込み | load() vs preload() | 対応有無の定義 | このチャンクには現れない | 未検証 |
| 相対/異常パス | "../x.gd"や空パス | バリデーション方針 | このチャンクには現れない | 未検証 |

- セキュリティチェックリスト（このファイルの範囲）
  - メモリ安全性: 標準的なRust安全機構内、unsafe使用**なし**。Buffer overflow/Use-after-free/Integer overflowの懸念**なし**（文字列操作とイテレータのみ）。
  - インジェクション: SQL/Command/Path traversal等の外部I/Oが**ない**ためリスク低。preloadパスは文字列であり、外部実行なし。
  - 認証・認可: 該当なし（テストのみ）。
  - 秘密情報: ハードコード秘密情報**なし**。ログ出力はインポート情報のみ。
  - 並行性: レースコンディション/デッドロック**なし**。並行実行の要素はコードに**ない**。

- Rust特有の観点
  - 所有権: `imports`（Vec）は関数ローカル所有。`for import in &imports`や`imports.iter()`で不変借用のみ。
  - 借用: `extends_import`や`class_name_import`は`Option<&T>`の不変参照。`is_some()`後に`unwrap()`するため安全（存在チェック済み）。
  - ライフタイム: 明示的ライフタイム指定は**不要**（標準的借用）。
  - unsafe境界: **unsafeブロックなし**。
  - Send/Sync: 並行性利用**なし**のため境界検討不要。
  - データ競合: 共有状態**なし**。
  - await境界/キャンセル: 非同期**未使用**。
  - エラー設計: `new().expect(...)`と`FileId::new(...).unwrap()`は失敗時に**panic**。テストとしては妥当だが、ライブラリ利用側ではエラー伝播を推奨。

## Design & Architecture Suggestions

- テストの重複削減
  - パーサ生成とFileId生成をヘルパー関数に抽出（例: `fn make_parser_and_file_id() -> (GdscriptParser, FileId)`）
  - インポート存在確認ヘルパー（`fn has_path(imports: &[Import], path: &str) -> bool`）を用意
- 仕様の明確化・アサート強化
  - `alias`と`is_type_only`の意味と期待値を仕様化し、テストで**明示的にアサート**する
  - `class_name`が常に`is_glob == true`であることの仕様化と、例外ケースの検証
- 異常系のテスト追加
  - 誤字/未対応記法/コメント含み/空コードなどの**ロバスト性**確認
- スナップショットテスト
  - `println!`内容を**snapshot**化して変更検知（例: `insta`クレート）。ただし外部依存追加はプロジェクト方針に従う
- パラメトリックテスト
  - 複数の入力と期待出力を列挙する**テーブルドリブン**テストで網羅性を高める

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト強化例（エイリアス/タイプのみの検証）

```rust
#[test]
fn test_class_name_alias_and_type_only_contract() {
    let code = r#"
class_name Enemy
"#;
    let mut parser = GdscriptParser::new().expect("parser");
    let file_id = FileId::new(1).unwrap();
    let imports = parser.find_imports(code, file_id);

    // 1件のclass_nameのみ
    assert_eq!(imports.len(), 1);

    let imp = &imports[0];
    assert_eq!(imp.path, "Enemy");

    // 仕様に応じてアサート（例: class_nameはglob）
    assert!(imp.is_glob, "class_name should be globally visible");

    // alias/type_onlyの仕様があるならアサート（このチャンクでは型/意味が不明）
    // 例: assert_eq!(imp.alias, None);
    // 例: assert!(!imp.is_type_only);
}
```

- 異常系テスト例（空文字列）

```rust
#[test]
fn test_empty_code_has_no_imports() {
    let mut parser = GdscriptParser::new().expect("parser");
    let file_id = FileId::new(1).unwrap();

    let imports = parser.find_imports("", file_id);
    assert_eq!(imports.len(), 0);
}
```

- 重複インポートの扱い確認（仕様次第）

```rust
#[test]
fn test_duplicate_imports_behavior() {
    let code = r#"
extends Node2D
extends Node2D
"#;
    let mut parser = GdscriptParser::new().expect("parser");
    let file_id = FileId::new(1).unwrap();
    let imports = parser.find_imports(code, file_id);

    // 仕様に応じて: 重複を許容か除去か
    // 例: assert_eq!(imports.len(), 1);
    // 例: assert_eq!(imports.len(), 2);
}
```

## Refactoring Plan & Best Practices

- ヘルパー抽出
  - パーサ/ファイルID生成を関数化してDRYにする
  - 汎用アサート関数（`assert_has(imports, path)`、`assert_glob(imports, path)`）の導入
- ログの最小化
  - テストの成功/失敗に影響しない`println!`は必要最小限に（出力ノイズ削減）
- 名前の一貫性
  - テスト名に入力と期待を反映（例: `..._extracts_extends_and_class_name`）
- 仕様注釈
  - `is_glob`/`is_type_only`/`alias`の意味をテストコメントで**明文化**し、将来の保守者に意図を伝える

## Observability (Logging, Metrics, Tracing)

- このファイルでは`println!`でテスト中に抽出結果を表示
  - テストハーネスが標準出力を**キャプチャ**するため、通常の実行で大量ログは推奨しない
  - 変更検知目的ならスナップショットテストの導入を検討
- メトリクス/トレーシングは**対象外**（テストのみ）

## Risks & Unknowns

- `find_imports`の内部仕様と計算量は**不明**（このチャンクに実装なし）
- インポートレコードの型名・`alias`の型/意味・`is_type_only`の解釈は**不明**
- `FileId::new(1)`の失敗条件は**不明**（`unwrap()`使用のため失敗時はパニック）
- `class_name`のglob扱いはテストが期待値を示すのみで、例外ケース（例えばネストや条件付き定義）の扱いは**未検証**
- `preload`以外の読み込みAPI（例: `load`）のサポート有無は**不明**