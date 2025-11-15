# parsing/typescript/definition.rs Review

## TL;DR

- ✅ 本ファイルは、TypeScript向けの言語定義を提供する単機能モジュール。主要公開APIは**TypeScriptLanguage**（LanguageDefinition実装）と内部公開の**register**関数。
- ✅ **拡張子**は"ts" / "tsx" / "mts" / "cts"をサポート。デフォルトで有効（default_enabled: true）。
- ✅ **パーサ生成**は`TypeScriptParser::new()`を呼び、失敗時は`IndexError::General`に変換して返す（エラー文脈の損失に注意）。
- ✅ **有効化判定**は`Settings.languages["typescript"].enabled`を優先し、なければデフォルト有効を使用。ハードコーディングされたキー"typescript"と`id()`の重複により将来の不整合のリスクあり。
- ✅ 設計はシンプルで、Box/Arcによる安全な所有権管理。unsafeなし、O(1)で軽量。
- ⚠️ 重大リスク/改善点：エラー型の粗さ、ID文字列の重複、設定構造（Settings/languages/config）の契約が外部に依存し不透明。

## Overview & Purpose

このファイルは、TypeScript言語に対する以下の定義と登録機能を提供します。

- 言語ID・表示名・拡張子の提供
- パーサ（TypeScriptParser）・振る舞い（TypeScriptBehavior）のファクトリ
- 言語のデフォルト有効化と設定に基づく有効/無効判定
- レジストリ（LanguageRegistry）への登録

これにより、プロジェクトの言語解析フレームワークへTypeScriptを統合します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | TypeScriptLanguage | pub | LanguageDefinitionの実装（ID/名前/拡張子/パーサ/振る舞い/有効判定） | Low |
| Function | register | pub(crate) | LanguageRegistryにTypeScriptLanguageを登録 | Low |
| Impl | impl LanguageDefinition for TypeScriptLanguage | crate内から利用（トレイト経由で実質公開） | LanguageDefinitionの各メソッド具体化 | Low |

### Dependencies & Interactions

- 内部依存（関数/構造体間の呼び出し関係）
  - TypeScriptLanguage::create_parser → super::TypeScriptParser::new（失敗をIndexErrorに変換）
  - TypeScriptLanguage::create_behavior → super::TypeScriptBehavior::new
  - TypeScriptLanguage::is_enabled → Settings.languages.get("typescript") → config.enabled
  - register → LanguageRegistry::register(Arc<dyn LanguageDefinition>)
- 外部依存（クレート/モジュール）
  
  | 依存名 | 種別 | 用途 |
  |--------|------|------|
  | crate::parsing::{LanguageBehavior, LanguageDefinition, LanguageId, LanguageParser, LanguageRegistry} | トレイト/型 | 言語統合の基盤トレイト・レジストリ |
  | crate::{IndexError, IndexResult, Settings} | エラー/設定 | エラー伝播・設定読み取り |
  | super::{TypeScriptBehavior, TypeScriptParser} | 同階層モジュール | 実際のパーサと振る舞い |
  | std::sync::Arc | 標準ライブラリ | レジストリへの共有所有権登録 |
- 被依存推定（このモジュールを使用する箇所）
  - 言語レジストリ初期化コード（全体の言語サポート組み立て時）
  - プロジェクトの解析実行パイプライン（言語ID/拡張子によるルーティング）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| TypeScriptLanguage (LanguageDefinition) | struct TypeScriptLanguage; | TypeScript言語定義の実体 | O(1) | O(1) |
| id | fn id(&self) -> LanguageId | 言語ID（"typescript"）の提供 | O(1) | O(1) |
| name | fn name(&self) -> &'static str | 表示名（"TypeScript"）の提供 | O(1) | O(1) |
| extensions | fn extensions(&self) -> &'static [&'static str] | 対応拡張子リストの提供 | O(1) | O(1) |
| create_parser | fn create_parser(&self, settings: &Settings) -> IndexResult<Box<dyn LanguageParser>> | TypeScriptパーサの生成 | O(1) | O(1) |
| create_behavior | fn create_behavior(&self) -> Box<dyn LanguageBehavior> | TypeScriptの言語振る舞い生成 | O(1) | O(1) |
| default_enabled | fn default_enabled(&self) -> bool | デフォルト有効状態の提供（true） | O(1) | O(1) |
| is_enabled | fn is_enabled(&self, settings: &Settings) -> bool | 設定に基づく有効/無効判定 | O(1) | O(1) |
| register | pub(crate) fn register(registry: &mut LanguageRegistry) | レジストリへTypeScript言語を登録 | O(1) | O(1) |

注:
- 公開範囲として外部に直接見えるのは構造体TypeScriptLanguage（pub）のみ。registerはcrate限定（pub(crate)）。
- 上記メソッドはLanguageDefinitionトレイトの一部として実装され、トレイト経由で利用されます。

### 各APIの詳細

1) id
- 目的と責務: 言語の一意IDを返す（"typescript"）
- アルゴリズム: LanguageId::new("typescript")を返却
- 引数: なし
- 戻り値: LanguageId（所有）
- 使用例:
  ```rust
  let lang = TypeScriptLanguage;
  assert_eq!(lang.id().as_str(), "typescript"); // as_strは推測。実APIはこのチャンクには現れない
  ```
- エッジケース:
  - 特になし。定数返却。

2) name
- 目的と責務: 表示用の言語名を返す
- アルゴリズム: "TypeScript"のリテラルを返却
- 引数/戻り値:
  - 引数: なし
  - 戻り値: &'static str
- 使用例:
  ```rust
  let lang = TypeScriptLanguage;
  assert_eq!(lang.name(), "TypeScript");
  ```
- エッジケース: なし

3) extensions
- 目的と責務: 拡張子の静的配列を返す
- データ: &["ts", "tsx", "mts", "cts"]
- 使用例:
  ```rust
  let exts = TypeScriptLanguage.extensions();
  assert!(exts.contains(&"ts"));
  ```
- エッジケース: なし

4) create_parser
- 目的と責務: TypeScriptParserインスタンスを生成し、LanguageParserトレイトオブジェクトとして返す
- アルゴリズム:
  1. TypeScriptParser::new()を呼ぶ
  2. Errの場合、IndexError::General(e.to_string()) にマップ
  3. Okの場合、Box<dyn LanguageParser>で包んで返却
- 引数:
  
  | 名前 | 型 | 意味 |
  |------|----|------|
  | settings | &Settings | パーサ初期化用設定（未使用。引数名は`_settings`として未使用） |
- 戻り値:
  
  | 型 | 意味 |
  |----|------|
  | IndexResult<Box<dyn LanguageParser>> | 成功時はパーサ、失敗時はエラー |
- 使用例（擬似・Settingsは不明のためコメント参照）:
  ```rust
  // 注意: Settingsの具体型はこのチャンクには現れない
  let lang = TypeScriptLanguage;
  // let settings = Settings::default(); // 仮
  // let parser = lang.create_parser(&settings).expect("parser init");
  ```
- エッジケース:
  - TypeScriptParser::new()の失敗 → IndexError::Generalに変換され、元エラー型情報は失われる

5) create_behavior
- 目的と責務: TypeScriptBehaviorインスタンスを生成し、LanguageBehaviorトレイトオブジェクトとして返す
- アルゴリズム: TypeScriptBehavior::new()を呼び、Box化して返却
- 引数/戻り値: 引数なし、戻り値はBox<dyn LanguageBehavior>
- 使用例:
  ```rust
  let behavior = TypeScriptLanguage.create_behavior();
  ```
- エッジケース: なし（new()がpanicしない前提）

6) default_enabled
- 目的: 言語のデフォルト有効状態を返す（true）
- 使用例:
  ```rust
  assert!(TypeScriptLanguage.default_enabled());
  ```

7) is_enabled
- 目的と責務: Settings内の言語設定から有効/無効を判定。なければdefault_enabled()を返す。
- アルゴリズム:
  1. settings.languages.get("typescript")
  2. あれば config.enabled を返す
  3. なければ default_enabled() を返す
- 引数/戻り値:
  - 引数: &Settings
  - 戻り値: bool
- 使用例（擬似）:
  ```rust
  // let mut settings = Settings::default();
  // settings.languages.insert("typescript".into(), LangConfig { enabled: false });
  // assert!(!TypeScriptLanguage.is_enabled(&settings));
  ```
- エッジケース:
  - languagesに"typescript"がない → true（デフォルト）
  - 設定が明示的にfalse → false

8) register
- 目的と責務: レジストリにTypeScriptLanguageを登録
- アルゴリズム: Arc::new(TypeScriptLanguage) を registry.register(...) に渡す
- 引数/戻り値:
  - 引数: &mut LanguageRegistry
  - 戻り値: ()
- 使用例（擬似）:
  ```rust
  // let mut registry = LanguageRegistry::new();
  parsing::typescript::definition::register(&mut registry);
  ```
- エッジケース: なし（registerの実装詳細はこのチャンクには現れない）

データ契約（Contracts）:
- Settings.languages: "typescript"キーで{ enabled: bool }等の設定を持つことを前提とするが、具体型はこのチャンクには現れない。

根拠の所在:
- 各主張は該当メソッド実装（関数名のみ。行番号はこのチャンクに存在しない）に基づく。

## Walkthrough & Data Flow

- 初期化時:
  - register(registry): Arc< TypeScriptLanguage >を生成し、LanguageRegistryに登録。これにより、フレームワークはTypeScript言語を認識できる。
- 解析時（推定フロー）:
  - フレームワークがファイル拡張子を見て言語を選択 → extensions()で一致判定。
  - 設定による有効判定 → is_enabled(&Settings)でtrueなら解析続行。
  - パーサ生成 → create_parser(&Settings)でBox<dyn LanguageParser>を取得。
  - 振る舞い生成 → create_behavior()で言語固有の処理方針を取得。

このファイル内のデータフローは直線的かつO(1)で、I/Oは関与しません。

## Complexity & Performance

- すべてのメソッドはO(1)時間・O(1)空間。
- メモリ確保:
  - create_parser: Box化によりヒープ確保が発生。
  - create_behavior: 同上。
  - register: Arc生成により参照カウント用のヒープ確保が発生。
- スケール限界:
  - 登録は1回・定数コスト。大量言語の登録時にも各言語あたりO(1)。
- 実運用負荷要因:
  - なし（I/O/ネットワーク/DBなし）。エラー発生時のto_string()コストは微小。

## Edge Cases, Bugs, and Security

- 既知のエッジケース

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| Parser初期化失敗 | TypeScriptParser::new()がErr | エラーを上位へ伝播 | IndexError::Generalに変換 | 対応済 |
| 設定未指定 | languagesに"typescript"なし | デフォルトtrueで有効 | unwrap_or(self.default_enabled()) | 対応済 |
| 明示的に無効 | languages["typescript"].enabled=false | 無効として扱う | map(...).unwrap_or(...) | 対応済 |
| 拡張子未対応 | ex: ".d.ts"など | フレームワーク側でマッチしない | extensions()に依存 | 対応済（.d.tsは.ts扱い想定だが拡張子は"ts"でカバー） |

- バグ/設計上の懸念
  - ハードコーディングされたID文字列の重複:
    - id()は"typescript"、is_enabled()も"typescript"を直書き。将来IDを変更した場合に不整合のリスク。
  - エラー型の情報損失:
    - create_parserでIndexError::General(e.to_string())に変換し、元エラー型の区別が消失。診断性が低下。
  - Settingsの構造に対する暗黙依存:
    - languagesマップのスキーマがこのチャンクには現れないため、契約の明確化が必要。

- セキュリティチェックリスト
  - メモリ安全性: unsafeなし、Buffer overflow/Use-after-free/Integer overflowの懸念はない。
  - インジェクション: SQL/Command/Path traversalの入力取り扱いなし。
  - 認証・認可: 関連処理なし。
  - 秘密情報: ハードコードされた秘密・ログ漏えいなし。
  - 並行性: registerは&mut LanguageRegistryで単一スレッド登録。Arcはスレッド間共有を可能にするがデータ競合はなし（このチャンク内）。

根拠:
- 各判定は該当メソッド（create_parser, is_enabled 等）の実装に基づく（行番号なし）。

## Design & Architecture Suggestions

- 定数化/単一情報源の原則:
  - "typescript"のID文字列をconst LANG_ID: &str = "typescript"として定義し、id()とis_enabled()で共用するか、is_enabled()でself.id()を参照してキー生成する。
- エラー設計の改善:
  - IndexErrorにParserInitなどのバリアントを設け、元エラーの種類を保持（Box<dyn std::error::Error + Send + Sync>等）。
- ログ/トレースの追加:
  - create_parser失敗時にログを残す（Observabilityセクション参照）。
- 拡張性のための設定利用:
  - create_parserに_settingsが引数で渡るが未使用。将来的にTS特有設定（型定義解析有無、JSXフラグ等）を読み取る余地あり。
- 一貫した登録パターン:
  - 複数言語で同パターンを踏襲するため、マクロ化やヘルパー関数化を検討。

## Testing Strategy (Unit/Integration) with Examples

ユニットテスト観点（このチャンク内の契約に限定）:
- id/name/extensionsの定数性と値の正当性
- default_enabledがtrue
- is_enabledの分岐（なし/true/false）
- create_parserの成功/失敗（失敗はTypeScriptParser::newのモックが必要、または注入設計に変更しやすく）

設定/レジストリなどの具体型はこのチャンクには現れないため、以下は擬似コードの例です。

```rust
#[test]
fn ts_language_basics() {
    let lang = TypeScriptLanguage;
    assert_eq!(lang.name(), "TypeScript");
    // LanguageId APIは不明なため文字列比較は擬似
    // assert_eq!(lang.id().as_str(), "typescript");

    let exts = lang.extensions();
    assert!(exts.contains(&"ts"));
    assert!(exts.contains(&"tsx"));
    assert!(exts.contains(&"mts"));
    assert!(exts.contains(&"cts"));

    assert!(lang.default_enabled());
}

#[test]
fn ts_language_is_enabled_default_true_when_missing() {
    let lang = TypeScriptLanguage;
    // let settings = Settings::default(); // 具体型不明
    // assert!(lang.is_enabled(&settings));
}

#[test]
fn ts_language_is_enabled_respects_settings() {
    let lang = TypeScriptLanguage;
    // let mut settings = Settings::default();
    // settings.languages.insert("typescript".to_string(), LangConfig { enabled: false });
    // assert!(!lang.is_enabled(&settings));
}

#[test]
fn ts_language_create_parser_maps_error() {
    let lang = TypeScriptLanguage;
    // 失敗注入が難しいため、TypeScriptParser::new()の注入可能設計にするか、インテグレーションテストで検証
    // let settings = Settings::default();
    // let res = lang.create_parser(&settings);
    // assert!(res.is_ok() || matches!(res, Err(IndexError::General(_))));
}
```

インテグレーションテスト観点:
- registerがLanguageRegistryにエントリを追加すること（LanguageRegistryのAPIに依存、ここには未掲載）。
- 拡張子によるファイルルーティングがTypeScriptLanguageに到達すること（上位層テスト）。

## Refactoring Plan & Best Practices

- 定数IDの集中管理:
  - const LANG_ID: &str = "typescript"; を導入し、id()/is_enabled()で使用。
- エラー型の表現力強化:
  - IndexError::Generalへの圧縮をやめ、原因（初期化失敗、依存欠如、バージョン不一致など）を表す列挙型バリアントを追加。
- 設定引数の活用/インタフェース改善:
  - create_parser(&Settings)で実際に設定を参照する、または未使用であれば引数を外す（互換性/今後の拡張性とトレードオフ）。
- テスト容易性の向上:
  - Parser/Behaviorの生成を関数ポインタやFactoryトレイトで注入できるようにし、失敗パスのユニットテストを実現。
- API一貫性:
  - is_enabledでself.id()を用いてキー文字列を生成し、重複定義を防ぐ。

## Observability (Logging, Metrics, Tracing)

- ログ:
  - create_parser失敗時: warn!やerror!でTypeScriptParser初期化失敗を記録（エラー詳細含む）。
  - register実行時: debug!で登録完了を記録（言語ID・拡張子）。
- メトリクス:
  - 言語別パーサ初期化成功/失敗カウンタ（counter: parser_init_success{lang="typescript"}, parser_init_failure{...}）
- トレーシング:
  - create_parser内でspanを開始し、初期化に時間がかかる将来の変更に備え可視化。

これらの仕組みはこのチャンクには存在しないため、追加実装が必要です。

## Risks & Unknowns

- Unknowns:
  - Settings構造・LanguageRegistry API・LanguageIdの詳細API（as_str等）はこのチャンクには現れない。
  - TypeScriptParser::new/TypeScriptBehavior::newの挙動・エラー型も不明。
- Risks:
  - 文字列キーの重複に起因する不整合（id()とis_enabled()）。
  - エラー情報の損失により、運用時の障害解析が困難。
  - 設定仕様の変更がis_enabled()のロジックに影響する可能性。

## Walkthrough & Data Flow

- TypeScriptLanguageの各メソッドは、主に定数返却と簡単なファクトリ処理で構成され、分岐も単純。
- 外部への影響はLanguageRegistryへの登録と、呼び出し側によるパーサ/振る舞いの取得に限定。

（注: 重要な主張は各メソッド実装に基づくが、行番号はこのチャンクには現れない）