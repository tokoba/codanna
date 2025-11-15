# definition.rs Review

## TL;DR

- 目的: **PHP言語をグローバルレジストリに自己登録**し、識別子・拡張子・パーサ/ビヘイビア生成・有効化判定を提供する。
- 主要公開API: **PhpLanguage**（構造体）、**PhpLanguage::ID**（定数）、**LanguageDefinition**の実装（id, name, extensions, create_parser, create_behavior, default_enabled, is_enabled）。
- 複雑箇所: **create_parser**でのエラーのラップ（`IndexError::General(e.to_string())`）、**is_enabled**の設定からのフォールバックロジック。
- 重大リスク: エラー型のロス（文字列化）により原因特定が難しくなる、**観測性（ログ/メトリクス）皆無**、`Settings`未使用の引数（将来の拡張との齟齬）。
- 並行性: `Arc`利用による安全な共有が前提だが、**Send/Sync境界やロック戦略はこのチャンクには現れない**。
- パフォーマンス: すべて**O(1)**の軽量操作。ボトルネックやI/Oはなし。
- テスト: **基本的な性質検証のみ**で、**エラー分岐・設定分岐の試験が不足**。

## Overview & Purpose

このファイルは、PHP言語の定義（識別子、表示名、拡張子、パーサ・ビヘイビアのファクトリ、デフォルト有効化設定、環境設定による有効化判定）を提供し、**グローバルな言語レジストリへ自己登録**する役割を担います。プロジェクトの言語サポート拡張における**プラグインポイント**として機能し、他のコンポーネント（レジストリ、設定、パーサ実装）と連携します。

根拠（関数名:行番号）については、当チャンクには行番号情報が含まれていないため、関数名のみで示します。

- 構造体: `PhpLanguage`
- トレイト実装: `LanguageDefinition for PhpLanguage`
- 内部登録関数: `register`

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | PhpLanguage | pub | PHP言語のメタデータとファクトリ提供 | Low |
| Const (assoc) | PhpLanguage::ID | pub | 言語識別子 "php" の定数 | Low |
| Trait Impl | LanguageDefinition for PhpLanguage | pub | id/name/extensions/parser/behavior/有効化の提供 | Low |
| Function | register | pub(crate) | レジストリへのPHP言語登録（Arcで所有） | Low |
| Tests | test_php_definition | private | 仕様の基本検証（ID/名前/拡張子） | Low |
| Tests | test_php_disabled_by_default | private | デフォルト有効化とレジストリ可用性検証 | Low |

### Dependencies & Interactions

- 内部依存
  - `create_parser` → `PhpParser::new()`（superモジュール）
  - `create_behavior` → `PhpBehavior::new()`（superモジュール）
  - `is_enabled` → `self.id()`（同impl内）、`Settings.languages`（crate設定）
  - `register` → `LanguageRegistry.register()`（crate::parsing）

- 外部依存（表）
  
  | 参照 | 役割 |
  |------|------|
  | std::sync::Arc | 言語定義の共有所有（スレッドセーフ参照カウント） |
  | super::{PhpBehavior, PhpParser} | PHP用のビヘイビア・パーサの具体実装ファクトリ |
  | crate::parsing::{LanguageBehavior, LanguageDefinition, LanguageId, LanguageParser} | 言語フレームワークのトレイトと型 |
  | crate::{IndexError, IndexResult, Settings} | 共通エラー・結果型・設定 |
  | crate::parsing::LanguageRegistry | グローバル言語レジストリ（registerで使用） |

- 被依存推定
  - レジストリ初期化モジュール（`get_registry`を呼ぶ初期化コード）
  - ドキュメント/インデクサ/ハイライト等、PHPファイルを扱う機能（レジストリ経由でパーサ/ビヘイビアにアクセス）

## API Surface (Public/Exported) and Data Contracts

公開API一覧（このファイルから「crate外」に公開されるもの）：

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| PhpLanguage | `pub struct PhpLanguage;` | 言語定義の具象型 | O(1) | O(1) |
| PhpLanguage::ID | `pub const ID: LanguageId` | 言語識別子定数 "php" | O(1) | O(1) |
| LanguageDefinition::id(PhpLanguage) | `fn id(&self) -> LanguageId` | 言語ID取得 | O(1) | O(1) |
| LanguageDefinition::name(PhpLanguage) | `fn name(&self) -> &'static str` | 表示名取得 | O(1) | O(1) |
| LanguageDefinition::extensions(PhpLanguage) | `fn extensions(&self) -> &'static [&'static str]` | 対応拡張子一覧 | O(1) | O(1) |
| LanguageDefinition::create_parser(PhpLanguage) | `fn create_parser(&self, settings: &Settings) -> IndexResult<Box<dyn LanguageParser>>` | パーサ生成 | O(1) | O(1) |
| LanguageDefinition::create_behavior(PhpLanguage) | `fn create_behavior(&self) -> Box<dyn LanguageBehavior>` | ビヘイビア生成 | O(1) | O(1) |
| LanguageDefinition::default_enabled(PhpLanguage) | `fn default_enabled(&self) -> bool` | デフォルト有効か | O(1) | O(1) |
| LanguageDefinition::is_enabled(PhpLanguage) | `fn is_enabled(&self, settings: &Settings) -> bool` | 設定に基づく有効化判定 | O(1) | O(1) |

注: `register` は `pub(crate)` であり、このファイルの公開APIには含めません。

以下、各APIの詳細。

1) PhpLanguage（構造体）
- 目的と責務
  - **PHP言語の定義オブジェクト**として、`LanguageDefinition`トレイトを実装し、各種メタ情報とファクトリを提供。
- アルゴリズム
  - 状態を持たないゼロサイズ的構造体。メソッドはすべて静的データまたはファクトリ呼び出しを返す。
- 引数
  - なし
- 戻り値
  - なし
- 使用例
  ```rust
  use crate::parsing::php::definition::PhpLanguage;
  let php = PhpLanguage;
  ```
- エッジケース
  - 特になし（状態を持たないため）。

2) PhpLanguage::ID（定数）
- 目的と責務
  - **言語IDを不変な定数として提供**（"php"）。
- アルゴリズム
  - コンパイル時に`LanguageId::new("php")`で初期化。
- 引数/戻り値
  - 引数: なし
  - 戻り値: `LanguageId`
- 使用例
  ```rust
  use crate::parsing::php::definition::PhpLanguage;
  let id = PhpLanguage::ID;
  assert_eq!(id.as_str(), "php");
  ```
- エッジケース
  - なし

3) id(&self) -> LanguageId
- 目的と責務
  - この言語の**識別子**を返す。
- アルゴリズム
  - `Self::ID`を返却。
- 引数
  | 名称 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | self | &PhpLanguage | 必須 | レシーバ |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | LanguageId | "php" を表すID |
- 使用例
  ```rust
  let php = PhpLanguage;
  assert_eq!(php.id().as_str(), "php");
  ```
- エッジケース
  - なし

4) name(&self) -> &'static str
- 目的と責務
  - 表示名 `"PHP"` を返す。
- アルゴリズム
  - リテラルを返却。
- 引数/戻り値
  | 名称 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | self | &PhpLanguage | 必須 | レシーバ |
  | 戻り値 | &'static str | - | "PHP" |
- 使用例
  ```rust
  assert_eq!(PhpLanguage.name(), "PHP");
  ```
- エッジケース
  - なし

5) extensions(&self) -> &'static [&'static str]
- 目的と責務
  - **サポートする拡張子一覧**を返す（例: "php", "php5", "phtml" など）。
- アルゴリズム
  - 静的配列スライスを返却。
- 引数/戻り値
  | 名称 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | self | &PhpLanguage | 必須 | レシーバ |
  | 戻り値 | &'static [&'static str] | - | 拡張子配列 |
- 使用例
  ```rust
  let exts = PhpLanguage.extensions();
  assert!(exts.contains(&"php"));
  ```
- エッジケース
  - なし（静的配列）。

6) create_parser(&self, settings: &Settings) -> IndexResult<Box<dyn LanguageParser>>
- 目的と責務
  - **PHPパーサのインスタンスを生成**し、エラーを共通型へラップして返す。
- アルゴリズム（ステップ）
  1. `PhpParser::new()` を呼ぶ。
  2. エラー時は `map_err(|e| IndexError::General(e.to_string()))` で `IndexError` に変換。
  3. 成功時は `Box<dyn LanguageParser>` に包んで返す。
- 引数
  | 名称 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | self | &PhpLanguage | 必須 | レシーバ |
  | settings | &Settings | 必須 | 現状未使用（将来拡張用の可能性） |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | IndexResult<Box<dyn LanguageParser>> | 成功時: パーサ、失敗時: `IndexError` |
- 使用例
  ```rust
  use crate::Settings;
  let php = PhpLanguage;
  let parser = php.create_parser(&Settings::default())?;
  // parser.parse(...); // 仮例：実際のメソッドはこのチャンクには現れない
  ```
- エッジケース
  - `PhpParser::new()` が失敗した場合、**エラー型が文字列化され `IndexError::General` に集約**され、元の型情報が失われる。
  - `settings` を無視しているため、**設定依存の挙動が期待される場合に反映されない**。

7) create_behavior(&self) -> Box<dyn LanguageBehavior>
- 目的と責務
  - **PHPのビヘイビア（言語特性の操作群）**のインスタンスを生成。
- アルゴリズム
  - `PhpBehavior::new()` を呼び、`Box` に詰めて返す。
- 引数/戻り値
  | 名称 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | self | &PhpLanguage | 必須 | レシーバ |
  | 戻り値 | Box<dyn LanguageBehavior> | - | ビヘイビア |
- 使用例
  ```rust
  let behavior = PhpLanguage.create_behavior();
  // behavior.some_method(); // 実メソッドはこのチャンクには現れない
  ```
- エッジケース
  - 生成時のエラー経路は**このチャンクには現れない**（`new()` が失敗しない前提の実装）。

8) default_enabled(&self) -> bool
- 目的と責務
  - **デフォルトで有効**かどうかを返す（`true`）。
- アルゴリズム
  - リテラル `true` を返却。
- 引数/戻り値
  | 名称 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | self | &PhpLanguage | 必須 | レシーバ |
  | 戻り値 | bool | - | 既定で有効（true） |
- 使用例
  ```rust
  assert!(PhpLanguage.default_enabled());
  ```
- エッジケース
  - なし

9) is_enabled(&self, settings: &Settings) -> bool
- 目的と責務
  - **設定に基づいてこの言語が有効か**どうかを返す。
- アルゴリズム（ステップ）
  1. `settings.languages.get(self.id().as_str())` で該当言語設定を検索。
  2. 見つかれば `config.enabled` を返却。
  3. 見つからなければ `unwrap_or(true)` で `true` を返す。
- 引数
  | 名称 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | self | &PhpLanguage | 必須 | レシーバ |
  | settings | &Settings | 必須 | 言語ごとの有効/無効設定を含む |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | bool | 有効（true）/ 無効（false） |
- 使用例
  ```rust
  use crate::Settings;
  let php = PhpLanguage;
  let s = Settings::default();
  assert!(php.is_enabled(&s));
  ```
- エッジケース
  - `settings.languages` に "php" エントリがない場合でも **true を返す**（デフォルト有効）。
  - 明示的に無効化（`config.enabled == false`）された場合は **false**。

10) register(registry: &mut LanguageRegistry) [pub(crate)]
- 目的と責務
  - **PHP言語をグローバルレジストリに登録**（`Arc`で共有所有）する。
- アルゴリズム
  - `registry.register(Arc::new(PhpLanguage))` を呼ぶのみ。
- 引数/戻り値
  | 名称 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | registry | &mut LanguageRegistry | 必須 | 共有レジストリ |
  | 戻り値 | なし | - | - |
- 使用例
  ```rust
  // crate内から:
  // let mut registry = ...;
  // register(&mut registry);
  ```
- エッジケース
  - 重複登録時の挙動は**このチャンクには現れない**（レジストリ側の仕様に依存）。

## Walkthrough & Data Flow

典型的な処理の流れ:

- レジストリ登録（`register`）
  - システム初期化時に `register` が呼ばれ、`Arc::new(PhpLanguage)` を登録する。
  - 以後、レジストリは PHP 言語に関するメタ情報・パーサ/ビヘイビアの生成を委譲できる。

- メタ情報取得（`id`, `name`, `extensions`）
  - クライアントは言語の識別や拡張子マッチングに使用。

- パーサ生成（`create_parser`）
  - レジストリやクライアントが `Box<dyn LanguageParser>` を得る。
  - 失敗時は `IndexError::General` に文字列化された原因が入る。

- ビヘイビア生成（`create_behavior`）
  - 言語特性に紐づく操作群を得る。

- 有効化判定（`default_enabled`, `is_enabled`）
  - `is_enabled` は `Settings.languages["php"].enabled` が存在すればそれに従い、無ければ `true`。

データフロー要点:
- 入力: `Settings`（`create_parser`, `is_enabled`）
- 出力: メタ情報、`Box<dyn LanguageParser>`, `Box<dyn LanguageBehavior>`, `bool`。
- 例外/エラー: `create_parser` が `IndexResult` を返すのみ。

このチャンクのコードは条件分岐が少なく、Mermaid図の使用基準に達しないため図は割愛します。

## Complexity & Performance

- 時間計算量: 全メソッドとも**O(1)**。
- 空間計算量: **O(1)**（静的データの参照と小さなボックス化のみ）。
- ボトルネック: なし。`PhpParser::new()` のコストは外部に依存（このチャンクには現れない）。
- スケール限界: なし（生成と参照のみ）。I/O/ネットワーク/DBは関与しない。
- 実運用負荷要因: パーサ生成のコスト・失敗率がシステム体感に影響しうるが、詳細は不明。

## Edge Cases, Bugs, and Security

エッジケース詳細表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| パーサ生成失敗 | `PhpParser::new()`がErr | `IndexError`で失敗を返す | `IndexError::General(e.to_string())` | 実装済 |
| 言語設定未登録 | `settings.languages`に"php"なし | true（デフォルト有効） | `unwrap_or(true)` | 実装済 |
| 明示的無効化 | `settings.languages["php"].enabled=false` | false | `map(|c| c.enabled)` | 実装済 |
| 重複登録 | 同一言語の再登録 | レジストリ仕様に依存 | 不明 | 不明 |
| Settings未使用 | `create_parser(&settings)` | settings非依存 | settings未参照 | 実装済（設計上検討要） |

セキュリティチェックリスト:
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（安全なRust、`unsafe`不使用）。
  - 所有権/借用: `Arc::new(PhpLanguage)` による共有所有のみ。可変借用なし。ライフタイム指定不要。
- インジェクション
  - SQL / Command / Path traversal: 該当なし（文字列定数と構築のみ）。
- 認証・認可
  - 権限チェック漏れ / セッション固定: 該当なし（登録/参照のみ）。
- 秘密情報
  - Hard-coded secrets / Log leakage: 該当なし。
- 並行性
  - Race condition / Deadlock: このファイル内には共有可変状態なし。レジストリのロックポリシーは**このチャンクには現れない**。
  - `Arc`はスレッド間共有に安全。`Send/Sync`境界の詳細は**不明**（トレイトオブジェクトに関しては外部型に依存）。
- Rust特有の観点（詳細チェックリスト）
  - 所有権: `register`で`Arc<PhpLanguage>`を生成し、レジストリへ移動（所有権移転）。
  - 借用: すべて不変借用。可変借用は`register`の引数`&mut`のみ（レジストリ更新のため）。
  - ライフタイム: 明示的パラメータ不要。静的データ（拡張子、名前）は`'static`。
  - unsafe境界: **unsafeブロックは存在しない**。
  - Send/Sync: `PhpLanguage`はゼロ状態であり、`Arc`で共有可能。`LanguageParser`/`LanguageBehavior`の`Send/Sync`は**このチャンクには現れない**。
  - 非同期/await: 非同期コードは**このチャンクには現れない**。
  - エラー設計: `create_parser`は`IndexResult`を返し、内部エラーを`IndexError::General(String)`に変換。`panic`は使用していない。

## Design & Architecture Suggestions

- **エラー型の明確化**: `IndexError::General(e.to_string())`では原因情報が乏しい。`IndexError`に**専用バリアント（例: ParserInit）**を追加し、元エラー型を`Box<dyn std::error::Error + Send + Sync>`で保持すると良い。
- **Settingsの活用**: `create_parser`が`settings`を受け取りながら未使用。将来的なパーサ設定（バージョン・互換モード等）を**設定から渡す**インタフェース検討。
- **拡張子の管理**: 拡張子配列は静的だが、**設定で拡張可能**にする余地あり（例: `settings.languages["php"].extensions`）。
- **登録重複防止**: `register`で**二重登録の防止**またはログ出力（Warn）を検討（レジストリ仕様次第）。
- **APIの一貫性**: `default_enabled`と`is_enabled`のデフォルトフォールバックが一致している点は良好。今後、**グローバルポリシー**に基づく一元管理を明示。

## Testing Strategy (Unit/Integration) with Examples

既存テストは基本的な挙動検証のみ。以下を追加推奨。

- `create_parser` のエラー経路
  - 目的: パーサ初期化失敗時に `IndexError` が返ること。
  - 実装案: `PhpParser::new()` が失敗するケースの**シミュレーションはこのチャンクには現れない**ため、可能ならインジェクション/DIパターン検討。現状は外部で失敗を誘発する条件が必要。

- `is_enabled` の分岐網羅
  ```rust
  #[test]
  fn test_php_is_enabled_with_settings() {
      let php = PhpLanguage;
      let mut s = Settings::default();
      // 擬似設定: languages["php"].enabled = false を設定するユーティリティはこのチャンクには現れない
      // ここでは概念的な例のみ
      // s.languages.insert("php".to_string(), LanguageConfig { enabled: false, /* ... */ });
      assert!(php.is_enabled(&Settings::default())); // エントリなし → true
      // assert!(!php.is_enabled(&s)); // エントリありかつ false → false
  }
  ```

- `register` の動作
  ```rust
  #[test]
  fn test_register_php_language() {
      // get_registry や LanguageRegistry の詳細はこのチャンクには現れないため、概念的な例
      // let registry = get_registry();
      // let mut reg = registry.lock().unwrap();
      // super::register(&mut reg);
      // assert!(reg.is_available(LanguageId::new("php")));
  }
  ```

- `extensions` の網羅
  ```rust
  #[test]
  fn test_php_extensions() {
      let exts = PhpLanguage.extensions();
      for e in ["php","php3","php4","php5","php7","php8","phps","phtml"] {
          assert!(exts.contains(&e));
      }
  }
  ```

## Refactoring Plan & Best Practices

- **Error改善**:
  - `create_parser` の `map_err` を `IndexError::ParserInit { source: e.into() }` のような**構造化エラー**へ変更。
- **設定活用**:
  - `create_parser` に `settings` を渡している意図を反映（例: パーサコンストラクタに設定を渡す）。
- **拡張性**:
  - `extensions` を設定やビルド構成で拡張可能に。
- **登録関数の設計**:
  - `register` を `pub` にするか、初期化フェーズの**明示的な登録順序**を文書化。
- **ドキュメンテーション**:
  - 各メソッドにおける**契約（Contract）**をRustdocに記す（設定がない場合に`true`を返す、など）。

## Observability (Logging, Metrics, Tracing)

- ログ
  - `register` 実行時に **Info** ログで登録成功、**Warn** で重複検出（レジストリ側が返すなら）。
  - `create_parser` 失敗時に **Error** ログで原因を記録（元エラー型保持が望ましい）。
- メトリクス
  - パーサ生成成功/失敗のカウンタ。
  - 言語別の有効化率（設定読み込み時点で集計）。
- トレーシング
  - 初期化フェーズのスパンに**言語登録**イベントを含める。
- 現状: このチャンクでは**観測処理は実装されていない**。

## Risks & Unknowns

- 不明点
  - `PhpParser::new()` の失敗条件・エラー型の詳細は**このチャンクには現れない**。
  - `LanguageRegistry.register()` の重複登録時挙動やスレッドセーフ性は**不明**。
  - `Settings` の構造（`languages` の具体型や挿入API）は**このチャンクには現れない**。
  - `LanguageParser`/`LanguageBehavior` の `Send/Sync` 境界は**不明**。
- リスク
  - エラーの文字列化による**デバッグ困難**。
  - `settings` 非使用により**将来の設定駆動の拡張が困難**。
  - 観測性欠如により運用時の**問題検知が遅れる**。

以上の分析は、当チャンクのコードに基づくものであり、記載のない機能・詳細は「不明」または「このチャンクには現れない」としています。