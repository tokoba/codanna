# definition.rs Review

## TL;DR

- 目的: レジストリにRust言語を自己登録し、設定に基づいてRust用のパーサと振る舞いを生成する言語定義の実装
- 主な公開API: Struct RustLanguage、関連定数 ID、LanguageDefinitionトレイト実装の各メソッド（id/name/extensions/create_parser/create_behavior/default_enabled/is_enabled）
- コアロジック: Settings.debugからRustParserを生成（create_parser: L34-L37）、Settings.languagesから有効/無効を判定（is_enabled: L47-L53）
- エラー設計: create_parserでRustParser::with_debugのエラーをIndexError::Generalにマップ（文脈喪失の懸念）
- 複雑箇所: なし（直線的な処理のみ、分岐は最小）
- 重大リスク: レジストリ重複登録の扱い不明、LanguageDefinitionがSend/Syncか不明で並行利用の要件が見えない
- セキュリティ: unsafeなし、外部入力の取り扱いなし、インジェクション等の懸念は現時点で低い

## Overview & Purpose

このファイルは、システム全体の言語レジストリにおいてRust言語を表現・登録するための「言語定義」を提供します。RustLanguageはLanguageDefinitionトレイトを実装し、以下を担います。

- 言語メタ情報の提供（id/name/extensions）
- 設定（Settings）からRustパーサ（RustParser）と振る舞い（RustBehavior）を生成
- デフォルトの有効化状態および設定に基づく有効/無効判定
- グローバルレジストリへの自己登録（register）

これにより、レジストリはRustソースファイル（.rs）を正しく識別し、対応するパーサ/振る舞いを供給できます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | RustLanguage | pub | Rust言語定義の具象型 | Low |
| Associated Const | RustLanguage::ID | pub | 言語ID("rust")の定数 | Low |
| Trait Impl | impl LanguageDefinition for RustLanguage | - | 言語メタ情報/生成/有効化判定の実装 | Low |
| Function | register | pub(crate) | レジストリにRustLanguageを登録 | Low |
| Module | tests | cfg(test) | 単体テスト | Low |

短い関数は全て10行未満で直線的な処理です。条件分岐はis_enabledにおけるOption処理のみです。

### Dependencies & Interactions

- 内部依存
  - RustLanguage.create_parser → RustParser::with_debug(settings.debug)（L34-L37）
  - RustLanguage.create_behavior → RustBehavior::new()（L39-L41）
  - RustLanguage.is_enabled → self.id().as_str() を利用した設定照会（L47-L53）
  - register → LanguageRegistry.register(Arc::new(RustLanguage))（L60-L62）

- 外部依存（このチャンクに存在しない型/関数）
  | 依存 | 由来 | 用途 | 備考 |
  |------|------|------|------|
  | RustParser | super | パーサ生成 | with_debug(bool) -> Result<_, _>（詳細不明） |
  | RustBehavior | super | 振る舞い生成 | new() |
  | LanguageDefinition | crate::parsing | トレイト実装 | レジストリが保持 |
  | LanguageParser | crate::parsing | トレイト | パーサ用trait object |
  | LanguageBehavior | crate::parsing | トレイト | ビヘイビア用trait object |
  | LanguageId | crate::parsing | ID型 | LanguageId::new("rust") |
  | LanguageRegistry | crate::parsing | レジストリ | registerメソッドの詳細不明 |
  | IndexResult/IndexError | crate | 例外ラッパ | Generalでエラー変換 |
  | Settings | crate | 設定 | debugとlanguagesを使用（構造詳細は不明） |

- 被依存推定
  - initialize_registry()（ドキュメントコメントより）からregisterが呼ばれる（L56-L59）
  - レジストリを介して、拡張子"rs"のファイル処理時にRustLanguageのAPIが呼ばれる

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| RustLanguage | pub struct RustLanguage; | 言語定義の具象型 | O(1) | O(1) |
| ID | pub const ID: LanguageId | 言語ID定数 | O(1) | O(1) |
| id | fn id(&self) -> LanguageId | 言語IDを返す | O(1) | O(1) |
| name | fn name(&self) -> &'static str | 言語名を返す | O(1) | O(1) |
| extensions | fn extensions(&self) -> &'static [&'static str] | 対応拡張子一覧を返す | O(1) | O(1) |
| create_parser | fn create_parser(&self, settings: &Settings) -> IndexResult<Box<dyn LanguageParser>> | 設定からパーサを生成 | O(1)※ | 不明 |
| create_behavior | fn create_behavior(&self) -> Box<dyn LanguageBehavior> | 振る舞い生成 | O(1) | O(1) |
| default_enabled | fn default_enabled(&self) -> bool | デフォルト有効状態 | O(1) | O(1) |
| is_enabled | fn is_enabled(&self, settings: &Settings) -> bool | 設定から有効/無効を判定 | O(1) | O(1) |
| register | pub(crate) fn register(registry: &mut LanguageRegistry) | レジストリへの登録 | O(1)※ | 不明 |

※ RustParser::with_debugやLanguageRegistry::register内部のコストに依存（このチャンクには現れない）。

以下、各APIの詳細。

1) RustLanguage::ID（L16-L19）
- 目的と責務: 言語ID定数 "rust" を提供
- アルゴリズム: 定数参照を返すだけ
- 引数: なし
- 戻り値: LanguageId（定数）
- 使用例:
  ```rust
  let id = RustLanguage::ID;
  assert_eq!(id.as_str(), "rust");
  ```
- エッジケース:
  - 特になし（定数）

2) id(&self) -> LanguageId（L22-L24）
- 目的と責務: 言語IDを返す
- アルゴリズム: Self::IDを返す
- 引数:
  | 引数 | 型 | 役割 |
  |------|----|------|
  | self | &RustLanguage | 受け手 |
- 戻り値:
  | 型 | 意味 |
  |----|------|
  | LanguageId | "rust" |
- 使用例:
  ```rust
  let rust = RustLanguage;
  assert_eq!(rust.id().as_str(), "rust");
  ```
- エッジケース:
  - なし

3) name(&self) -> &'static str（L26-L28）
- 目的と責務: 表示名"Rust"を返す
- アルゴリズム: リテラル返却
- 引数/戻り値:
  - 引数: &self
  - 戻り値: &'static str = "Rust"
- 使用例:
  ```rust
  assert_eq!(RustLanguage.name(), "Rust");
  ```
- エッジケース: なし

4) extensions(&self) -> &'static [&'static str]（L30-L32）
- 目的と責務: 対応拡張子（"rs"）の静的スライスを返す
- アルゴリズム: リテラル配列への参照を返却
- 引数/戻り値:
  - 引数: &self
  - 戻り値: &'static [&'static str] = &["rs"]
- 使用例:
  ```rust
  assert!(RustLanguage.extensions().contains(&"rs"));
  ```
- エッジケース: なし

5) create_parser(&self, settings: &Settings) -> IndexResult<Box<dyn LanguageParser>>（L34-L37）
- 目的と責務: Settings.debugに基づいてRustParserを生成し、トレイトオブジェクトとして返す
- アルゴリズム（ステップ）:
  1. settings.debugを取得
  2. RustParser::with_debug(debug)を呼ぶ
  3. 失敗時はIndexError::Generalにマップ（map_err）して返す
  4. 成功時はBoxに包んで返却
- 引数:
  | 引数 | 型 | 役割 |
  |------|----|------|
  | settings | &Settings | デバッグフラグ等の設定 |
- 戻り値:
  | 型 | 意味 |
  |----|------|
  | IndexResult<Box<dyn LanguageParser>> | 成功時はパーサ、失敗時はIndexError |
- 使用例（疑似コード: 型詳細はこのチャンクには現れない）:
  ```rust
  let lang = RustLanguage;
  let mut settings = Settings::default();
  settings.debug = true;
  let parser = lang.create_parser(&settings)?;
  // parser.parse(...);
  ```
- エッジケース:
  - RustParser::with_debugがエラーを返す
  - settings.debugが未設定（Default依存）
  - エラー変換で元のエラー文脈が失われる懸念（後述）

6) create_behavior(&self) -> Box<dyn LanguageBehavior>（L39-L41）
- 目的と責務: Rust用の振る舞いオブジェクトを生成
- アルゴリズム: RustBehavior::new()をBox化して返却
- 引数/戻り値:
  - 引数: &self
  - 戻り値: Box<dyn LanguageBehavior>
- 使用例:
  ```rust
  let behavior = RustLanguage.create_behavior();
  ```
- エッジケース: なし

7) default_enabled(&self) -> bool（L43-L45）
- 目的と責務: デフォルトでRust言語を有効化する
- アルゴリズム: trueを返す
- 引数/戻り値:
  - 引数: &self
  - 戻り値: true
- 使用例:
  ```rust
  assert!(RustLanguage.default_enabled());
  ```
- エッジケース: なし

8) is_enabled(&self, settings: &Settings) -> bool（L47-L53）
- 目的と責務: 設定に基づいてRust言語の有効/無効を判定
- アルゴリズム（ステップ）:
  1. settings.languagesからself.id().as_str()（"rust"）のエントリを取得
  2. 見つかればconfig.enabledを返す
  3. 見つからなければtrue（デフォルト有効）
- 該当コード（短いので全体引用）:
  ```rust
  fn is_enabled(&self, settings: &Settings) -> bool {
      settings
          .languages
          .get(self.id().as_str())
          .map(|config| config.enabled)
          .unwrap_or(true)
  }
  ```
- 引数:
  | 引数 | 型 | 役割 |
  |------|----|------|
  | settings | &Settings | languagesマップを含む設定 |
- 戻り値:
  | 型 | 意味 |
  |----|------|
  | bool | 有効/無効フラグ |
- 使用例（疑似コード）:
  ```rust
  let mut settings = Settings::default();
  // settings.languages.insert("rust".to_string(), LanguageConfig { enabled: false, ..Default::default() });
  assert!(!RustLanguage.is_enabled(&settings)); // 上のinsertが有効な場合
  ```
- エッジケース:
  - エントリなし → true（デフォルト有効）
  - キーの大小文字/表記揺れ → 一致しないとデフォルトtrue（IDは"rust"固定）

9) register(&mut LanguageRegistry)（L60-L62, pub(crate)）
- 目的と責務: レジストリへRustLanguageをArcで登録
- アルゴリズム: Arc::new(RustLanguage)をregistry.registerへ渡す
- 引数:
  | 引数 | 型 | 役割 |
  |------|----|------|
  | registry | &mut LanguageRegistry | 登録先 |
- 戻り値: なし
- 使用例（クレート内）:
  ```rust
  pub(crate) fn register(registry: &mut LanguageRegistry) {
      registry.register(Arc::new(RustLanguage));
  }
  ```
- エッジケース:
  - 重複登録時の挙動は不明（LanguageRegistryの実装次第）

データコントラクト（このチャンクで確定できるもの）:
- LanguageIdは文字列に基づくID（"rust"）
- Settingsはdebug(bool)とlanguages(キーが&str/ String)を持つ（型詳細は不明）

## Walkthrough & Data Flow

- パーサ生成フロー（create_parser: L34-L37）
  1. Settings.debugを読み出す
  2. RustParser::with_debug(debug)で具体パーサ生成（Result）
  3. 失敗時はIndexError::Generalにマップして返却
  4. 成功時はBox<dyn LanguageParser>として返す

- 振る舞い生成フロー（create_behavior: L39-L41）
  1. RustBehavior::new()で具体振る舞いを作成
  2. Box<dyn LanguageBehavior>として返す

- 有効化判定フロー（is_enabled: L47-L53）
  1. settings.languagesに"rust"キーがあればenabledフラグを反映
  2. なければtrue（デフォルト有効）

- 登録フロー（register: L60-L62）
  1. RustLanguageをArcでヒープ確保
  2. LanguageRegistry.registerへ渡し、グローバルレジストリに追加

Mermaid図は、条件分岐/アクターの数が少なく直線的な処理のため「使用しない」基準に該当します。

## Complexity & Performance

- id/name/extensions/default_enabled: 時間O(1)、空間O(1)
- is_enabled: HashMap等のlookupを仮定してO(1)平均、空間O(1)
- create_behavior: O(1)、空間O(1)
- create_parser: 呼び出しはO(1)だが、RustParser::with_debug内部のコストは不明（このチャンクには現れない）
- register: 呼び出し自体はO(1)だが、LanguageRegistry::registerのコストは不明

ボトルネック:
- 現状なし。唯一、create_parser内のRustParser初期化コストが潜在的に支配的になり得るが詳細不明。

スケール限界:
- 言語定義は定数的な情報を返すのみでスケール問題はない。
- レジストリへの登録重複や並行アクセスの方が運用での懸念点（詳細は不明）。

I/O/ネットワーク/DB:
- 本ファイルには該当なし。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト観点での評価:

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 安全なRustのみ、外部入力なし、該当なし
  - 所有権/借用/ライフタイム: 参照は&Settingsのみ、Box/Arcは標準的使用で問題なし
- インジェクション
  - SQL/Command/Path traversal: 該当なし
- 認証・認可
  - 権限チェック/セッション: 該当なし
- 秘密情報
  - ハードコード秘密/ログ漏えい: 該当なし（ログ自体なし）
- 並行性
  - Race/Deadlock: 本ファイル単体では該当なし。LanguageRegistry内の実装次第
  - Send/Sync: Arc<dyn LanguageDefinition>が並行に共有される可能性。LanguageDefinitionにSend+Sync境界が必要かは不明（このチャンクには現れない）

既知/推定エッジケースの詳細:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 設定に言語エントリなし | settings.languagesに"rust"がない | trueを返す | is_enabled(L47-L53)でunwrap_or(true) | OK |
| 明示的に無効化 | languages["rust"].enabled=false | falseを返す | is_enabledでmap(|c| c.enabled) | OK |
| デバッグ有効 | settings.debug=true | デバッグモードでRustParser生成 | create_parser(L34-L37) | OK（詳細はRustParser次第） |
| パーサ生成失敗 | with_debugがErr | IndexError::GeneralでErrを返す | create_parser(L34-L37) | OK（ただし文脈喪失の懸念） |
| 重複登録 | registerを複数回呼ぶ | 重複を防止/上書き等の一貫した挙動 | LanguageRegistry依存 | 不明 |

潜在バグ/懸念:
- エラー文脈の喪失: map_err(crate::IndexError::General)により元エラーの型・メッセージが失われる可能性
- 重複登録時の挙動不明: registerの多重呼び出しに対する保護が見えない
- 大小文字やID表記揺れに弱い: is_enabledは"rust"固定キーに依存（仕様として妥当だが柔軟性は低い）

Rust特有の観点（このチャンク基準）:
- 所有権: Arc::new(RustLanguage)で所有権はArcに移り、レジストリで共有（register: L60-L62）
- 借用/ライフタイム: &Settingsの一時借用のみ（create_parser/is_enabled）
- unsafe: 使用箇所なし
- 並行性: Arc利用から並行読み取り前提の可能性。LanguageDefinitionがSend+Syncかの保証はこのチャンクには現れない
- await/非同期: 該当なし
- エラー設計: Resultはcreate_parserのみ。unwrap/expect未使用でpanic無し

## Design & Architecture Suggestions

- エラー文脈の保持
  - Generalではなく、ソースエラーを包むVariant（e.g., IndexError::ParserInit { lang: "rust", source: anyhow::Error }）へ変換する
  - thiserrorを使ったエラー定義、map_err(|e| IndexError::parser_init("rust", e))のようなコンストラクタを用意

- レジストリ登録の重複制御
  - LanguageRegistry::registerが同一IDの重複を拒否/上書き/参照カウント増加などを明確化
  - register側で既登録チェック（registry.contains(LanguageId::new("rust"))）が可能なら追加

- 並行性境界の明確化
  - LanguageDefinitionにSend + Sync境界が必要であれば型定義/登録時に明示（Arc<dyn LanguageDefinition + Send + Sync>）
  - レジストリ内部の同期戦略（RwLockなど）の仕様文書化

- ユーティリティの追加
  - コンストラクタ/ファクトリ: fn arc() -> Arc<dyn LanguageDefinition + Send + Sync>で登録コードの定型化
  - extensions/IDの一元化: テスト/他コンポーネントでの誤記防止

- ドキュメント補強
  - is_enabledの仕様（デフォルト有効）をdocコメントで明記
  - create_parserのエラー契約（どんな状況でエラーか）を明記

## Testing Strategy (Unit/Integration) with Examples

現状のテスト:
- test_rust_definition（L68-L75）: id/name/extensionsの整合性
- test_rust_enabled_by_default（L77-L84）: デフォルト有効の確認

追加で推奨するテスト:

- 有効/無効の上書き
  ```rust
  #[test]
  fn test_rust_disabled_via_settings() {
      let lang = RustLanguage;
      let mut settings = Settings::default();
      // 型や挿入方法はこのチャンクには現れないため擬似コード
      // settings.languages.insert("rust".to_string(), LanguageConfig { enabled: false, ..Default::default() });
      assert_eq!(lang.is_enabled(&settings), false);
  }
  ```

- パーサ生成成功/失敗
  ```rust
  #[test]
  fn test_create_parser_ok() {
      let lang = RustLanguage;
      let settings = Settings::default();
      let parser = lang.create_parser(&settings).expect("parser should be created");
      // 追加の型検査や簡単なメソッド呼び出し（パーサAPIがわかれば）
  }

  #[test]
  fn test_create_parser_error_propagation() {
      let lang = RustLanguage;
      let mut settings = Settings::default();
      settings.debug = true; // 例: ある設定で失敗を誘発できるなら
      let result = lang.create_parser(&settings);
      // 期待: Err(IndexError::General(_)) ただしIndexErrorの詳細はこのチャンクには現れない
      assert!(result.is_err());
  }
  ```

- レジストリ登録の検証（クレート内）
  ```rust
  #[test]
  fn test_register_into_registry() {
      let mut registry = LanguageRegistry::new(); // 仮
      super::register(&mut registry);
      // assert!(registry.contains(LanguageId::new("rust"))); // 仮API
  }
  ```

- 安定性テスト
  - id/name/extensionsの再入可能性（複数回呼んでも同値）
  - is_enabledのキー不在でのデフォルト挙動

注: 設定やレジストリ型の詳細はこのチャンクには現れないため、上記は擬似コードを含みます。

## Refactoring Plan & Best Practices

- エラーラッピングの改善（thiserror/anyhow導入 or カスタムVariant）
- 型エイリアスの導入
  ```rust
  type DynLanguageDef = Arc<dyn LanguageDefinition + Send + Sync>;
  ```
  registerの実装簡素化とトレイト境界の明確化に寄与

- 文書化の拡充
  - is_enabledのデフォルト動作とキー一致仕様の明記
  - create_parser失敗時のシナリオをdocコメントで列挙

- 例外安全の強化
  - map_errでメッセージを付加
  ```rust
  let parser = RustParser::with_debug(settings.debug)
      .map_err(|e| IndexError::General(format!("rust parser init failed: {e}")))?;
  ```

- テストの充実
  - 失敗系テストの追加
  - レジストリ重複登録のテスト（実装に応じて）

## Observability (Logging, Metrics, Tracing)

- ロギング
  - create_parser失敗時にエラーをログ出力（lang="rust", debug=settings.debug）
  - register実行時に情報ログ（重複時の挙動を含め）

- メトリクス
  - parser_creation_total{lang="rust", outcome="ok|err"}
  - language_enabled{lang="rust"}ゲージ（設定ロード時に更新）

- トレーシング
  - create_parserのspan: name="create_parser", fields(lang="rust", debug)
  - 失敗時にエラーイベントを記録

例（tracing利用の擬似コード）:
```rust
tracing::info!(lang = "rust", "registering language");
let span = tracing::info_span!("create_parser", lang = "rust", debug = settings.debug);
let _enter = span.enter();
// ...
```

## Risks & Unknowns

- LanguageRegistryの重複登録時動作が不明（上書き/拒否/無視）
- LanguageDefinition/LanguageParser/LanguageBehaviorのSend/Sync要件が不明（並行実行時の安全性に影響）
- Settings.languagesの実際のキー型/LanguageConfig構造が不明（例示コードは仮）
- RustParser::with_debugの初期化コストやエラー条件が不明
- IndexError::Generalの実態（文字列/ソースエラー保持の有無）が不明

以上の不明点は、関連モジュール（このチャンクには現れない）を確認して仕様を固める必要があります。