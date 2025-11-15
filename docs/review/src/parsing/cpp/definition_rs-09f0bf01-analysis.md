# parsing\cpp\definition.rs Review

## TL;DR

- 目的: **C++言語定義**を実装し、グローバルレジストリに自己登録することで、C++向けのパーサーとビヘイビアを提供する
- 主要公開API: **CppLanguage**（pub struct）およびその**LanguageDefinition**トレイト実装、内部登録関数**register**（pub(crate)）
- コアロジック: **create_parser**で`CppParser`を生成し、**create_behavior**で`CppBehavior`を返す。**is_enabled**で設定から有効/無効を判定
- 複雑箇所: エラー変換`map_err(crate::IndexError::General)?`の扱いと、設定マップからの言語有効判定（デフォルト`true`）
- 重大リスク: エラーを**一般化**しすぎて詳細が失われる可能性、**Observability不足**（ログ/メトリクスなし）
- Rust安全性: **unsafeなし**、**Arc/Box**の安全な使用、**借用は不変参照のみ**、スレッド安全性は概ね良好
- 並行性: `Arc<CppLanguage>`で共有可能だが、**Send/Sync保証はこのチャンクには現れない**

## Overview & Purpose

このファイルは、C++言語の定義をレジストリに登録するための**言語定義モジュール**です。以下を提供します。

- C++言語識別子（ID）とメタ情報（名前、拡張子）
- C++向けのパーサー`CppParser`およびビヘイビア`CppBehavior`のファクトリ
- 設定`Settings`に基づく言語の有効/無効判定
- グローバルレジストリへの登録関数（内部API）

これにより、システムは拡張子に基づいてC++ファイルを認識し、適切なパーサー/ビヘイビアを生成して処理できます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | CppLanguage | pub | C++言語の定義本体（ID, 名前, 拡張子, ファクトリ） | Low |
| Const | CppLanguage::ID | pub | 言語識別子（"cpp"） | Low |
| Impl (Trait) | LanguageDefinition for CppLanguage | - | 言語定義トレイトの実装（メタ情報、ファクトリ、設定判定） | Low |
| Function | register | pub(crate) | レジストリへ`CppLanguage`を登録する | Low |

### Dependencies & Interactions

- 内部依存
  - CppParser（super::CppParser）を`create_parser`で生成
  - CppBehavior（super::CppBehavior）を`create_behavior`で生成
  - Settings（crate::Settings）を`is_enabled`で参照
  - IndexResult / IndexError（crate::IndexResult, crate::IndexError）を`create_parser`の戻り値で使用
  - LanguageRegistry（crate::parsing::LanguageRegistry）を`register`で使用
  - LanguageId / LanguageParser / LanguageBehavior / LanguageDefinition（crate::parsing）を型/トレイトとして使用
- 外部依存（標準/クレート）
  | 名称 | 種別 | 用途 |
  |------|------|------|
  | std::sync::Arc | 標準ライブラリ | 言語定義の共有参照管理（レジストリ格納時） |
- 被依存推定
  - レジストリ初期化処理（例: initialize_registry()）から`register`が呼ばれる（このチャンクには現れない）
  - ファイル拡張子解決とパース処理フローで、レジストリから`LanguageDefinition`が取得され、`create_parser`/`create_behavior`が呼ばれる（このチャンクには現れない）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| CppLanguage::ID | pub const ID: LanguageId | 言語識別子（"cpp"）の提供 | O(1) | O(1) |
| CppLanguage（LanguageDefinition impl）: id | fn id(&self) -> LanguageId | 言語IDを返す | O(1) | O(1) |
| CppLanguage（LanguageDefinition impl）: name | fn name(&self) -> &'static str | 表示名を返す | O(1) | O(1) |
| CppLanguage（LanguageDefinition impl）: extensions | fn extensions(&self) -> &'static [&'static str] | 対応拡張子一覧を返す | O(1) | O(1) |
| CppLanguage（LanguageDefinition impl）: create_parser | fn create_parser(&self, _settings: &Settings) -> IndexResult<Box<dyn LanguageParser>> | C++パーサーを生成 | O(1) | O(1) (+確保) |
| CppLanguage（LanguageDefinition impl）: create_behavior | fn create_behavior(&self) -> Box<dyn LanguageBehavior> | C++ビヘイビアを生成 | O(1) | O(1) (+確保) |
| CppLanguage（LanguageDefinition impl）: default_enabled | fn default_enabled(&self) -> bool | デフォルト有効フラグ | O(1) | O(1) |
| CppLanguage（LanguageDefinition impl）: is_enabled | fn is_enabled(&self, settings: &Settings) -> bool | 設定に基づく有効/無効判定 | O(1)平均 | O(1) |
| register | pub(crate) fn register(registry: &mut crate::parsing::LanguageRegistry) | レジストリへC++言語を登録 | O(1) | O(1) |

以下、主要APIの詳細を記載します（行番号はこのチャンクでは不明）。

### CppLanguage::ID

1) 目的と責務
- **目的**: C++言語の一意の識別子を提供
- **責務**: 他モジュールがC++言語を参照する際のキーとなる

2) アルゴリズム（ステップ）
- `LanguageId::new("cpp")`で初期化された定数を返すだけ

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| なし | - | - | - |

4) 戻り値
| 型 | 説明 |
|----|------|
| LanguageId | "cpp"を内包する識別子 |

5) 使用例
```rust
use crate::parsing::LanguageId;
use crate::parsing::cpp::definition::CppLanguage;

let id: LanguageId = CppLanguage::ID;
assert_eq!(id.as_str(), "cpp"); // as_str()はこのチャンクには現れないが一般的な例
```

6) エッジケース
- 特になし（定数）

### id

1) 目的と責務
- **目的**: 言語IDを返す
- **責務**: トレイト`LanguageDefinition`準拠

2) アルゴリズム
- `Self::ID`を返す

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| self | &Self | 必須 | CppLanguageの参照 |

4) 戻り値
| 型 | 説明 |
|----|------|
| LanguageId | CppLanguage::ID |

5) 使用例
```rust
let lang = crate::parsing::cpp::definition::CppLanguage;
let id = lang.id();
```

6) エッジケース
- なし

### name

1) 目的と責務
- **目的**: 表示名"**C++**"を返す

2) アルゴリズム
- リテラル文字列を返す

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| self | &Self | 必須 | 参照 |

4) 戻り値
| 型 | 説明 |
|----|------|
| &'static str | "C++" |

5) 使用例
```rust
let name = crate::parsing::cpp::definition::CppLanguage.name();
assert_eq!(name, "C++");
```

6) エッジケース
- なし

### extensions

1) 目的と責務
- **目的**: 対応拡張子セット（"cpp", "hpp", "cc", "cxx", "hxx"）の提供

2) アルゴリズム
- 静的スライスを返す

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| self | &Self | 必須 | 参照 |

4) 戻り値
| 型 | 説明 |
|----|------|
| &'static [&'static str] | 拡張子の静的配列スライス |

5) 使用例
```rust
let exts = crate::parsing::cpp::definition::CppLanguage.extensions();
assert!(exts.contains(&"cpp"));
```

6) エッジケース
- 大文字拡張子の扱いは不明（このチャンクには現れない）

### create_parser

1) 目的と責務
- **目的**: C++向けの`LanguageParser`実装（`CppParser`）を構築し返す
- **責務**: 初期化失敗時に適切な`IndexResult`エラーを返す

2) アルゴリズム
- `CppParser::new()`を呼ぶ
- 失敗なら`map_err(crate::IndexError::General)?`で`IndexError::General`へ変換し、早期return
- 成功なら`Box::new(parser)`でトレイトオブジェクト化して返す

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| _settings | &Settings | 必須 | 現状未使用（将来設定に応じた初期化に備えた拡張ポイント） |

4) 戻り値
| 型 | 説明 |
|----|------|
| IndexResult<Box<dyn LanguageParser>> | 成功時はパーサーのトレイトオブジェクト、失敗時はエラー |

5) 使用例
```rust
use crate::parsing::LanguageDefinition;
use crate::parsing::cpp::definition::CppLanguage;
use crate::Settings;

let lang = CppLanguage;
// Settingsの生成方法はこのチャンクには現れない
let settings: &Settings = /* 既存の設定参照 */;
let parser = lang.create_parser(settings)?;
```

6) エッジケース
- `CppParser::new()`が失敗するケース
  - 変換で`IndexError::General`になるため詳細なエラー型が失われる可能性
- `_settings`未使用
  - 将来設定差分が必要になった場合は拡張が必要

### create_behavior

1) 目的と責務
- **目的**: C++の解析中挙動（`LanguageBehavior`）を提供

2) アルゴリズム
- `CppBehavior::new()`を呼び、`Box`で返す

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| self | &Self | 必須 | 参照 |

4) 戻り値
| 型 | 説明 |
|----|------|
| Box<dyn LanguageBehavior> | C++向けビヘイビア |

5) 使用例
```rust
use crate::parsing::LanguageDefinition;
let behavior = crate::parsing::cpp::definition::CppLanguage.create_behavior();
```

6) エッジケース
- 生成失敗の経路は実装上なし（`new()`が失敗する場合の仕様はこのチャンクには現れない）

### default_enabled

1) 目的と責務
- **目的**: C++をデフォルトで有効にするかの指針を返す

2) アルゴリズム
- `true`を返す

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| self | &Self | 必須 | 参照 |

4) 戻り値
| 型 | 説明 |
|----|------|
| bool | デフォルト有効（true） |

5) 使用例
```rust
assert!(crate::parsing::cpp::definition::CppLanguage.default_enabled());
```

6) エッジケース
- なし

### is_enabled

1) 目的と責務
- **目的**: 設定に基づいてC++言語の有効/無効を判定
- **責務**: `settings.languages`マップから`"cpp"`キーを参照し、存在しない場合は`true`

2) アルゴリズム
- `settings.languages.get(self.id().as_str())`で設定取得
- `map(|config| config.enabled).unwrap_or(true)`で有効フラグを返す

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| settings | &Settings | 必須 | 言語のオン/オフ設定を保持する構造体 |

4) 戻り値
| 型 | 説明 |
|----|------|
| bool | 有効ならtrue、無効ならfalse（設定が無ければtrue） |

5) 使用例
```rust
use crate::parsing::LanguageDefinition;
use crate::parsing::cpp::definition::CppLanguage;
use crate::Settings;

let settings: &Settings = /* 既存の設定参照 */;
let enabled = CppLanguage.is_enabled(settings);
```

6) エッジケース
- `settings.languages`に`"cpp"`が存在しない場合はtrue
- 言語設定構造（`config.enabled`）の仕様はこのチャンクには現れない

### register（内部API）

1) 目的と責務
- **目的**: グローバルレジストリへC++言語定義を登録
- **責務**: `Arc::new(CppLanguage)`で参照を共有しつつレジストリに追加

2) アルゴリズム
- `registry.register(Arc::new(CppLanguage));`の一行

3) 引数
| 名前 | 型 | 必須 | 説明 |
|------|----|------|------|
| registry | &mut LanguageRegistry | 必須 | レジストリ本体 |

4) 戻り値
| 型 | 説明 |
|----|------|
| なし | - |

5) 使用例
```rust
use crate::parsing::LanguageRegistry;
// LanguageRegistryの生成APIはこのチャンクには現れない
let mut registry: LanguageRegistry = /* 生成 */;
crate::parsing::cpp::definition::register(&mut registry);
```

6) エッジケース
- レジストリが`Arc<CppLanguage>`を受け付けない場合は型不一致（このチャンクには現れない）

## Walkthrough & Data Flow

- 起動時（推定）にレジストリ初期化で🧩`register`が呼ばれ、`CppLanguage`が登録される
- 拡張子解決で"cpp"/"cc"/"cxx"などのファイルに対し📁レジストリから`CppLanguage`が選択される
- 解析開始時に⚙️`create_parser`が呼ばれ、`CppParser`が生成される。並行に（必要に応じて）`create_behavior`で`CppBehavior`が取得される
- 実行の可否は🔧`is_enabled(settings)`で判定され、無効ならスキップ、有効なら解析が継続される

この一連の制御のうち、レジストリ検索やファイル処理はこのチャンクには現れないため詳細は不明。

## Complexity & Performance

- 時間計算量
  - `id/name/extensions/default_enabled`: O(1)
  - `is_enabled`: ハッシュマップ参照が平均O(1)
  - `create_parser/create_behavior`: O(1)（内部`new()`のコストは微小、確保あり）
- 空間計算量
  - いずれもO(1)。`Box`と`Arc`のメタデータと、パーサー/ビヘイビアのインスタンス分が加算
- ボトルネック
  - このファイル単体ではボトルネックなし。`CppParser::new()`の初期化コストは外部依存
- スケール限界
  - 多数言語の同時登録・利用時でもレジストリのハッシュ参照が主要コスト（このチャンクには現れない）

## Edge Cases, Bugs, and Security

- メモリ安全性
  - **unsafeなし**。`Arc`/`Box`の使用は安全。所有権は明確（構造体はゼロフィールドで不変）
- インジェクション
  - SQL/Command/Path traversalの入力は扱っていないため該当なし
- 認証・認可
  - 機能なし（該当なし）
- 秘密情報
  - ハードコード秘密なし。ログ出力もなし
- 並行性
  - `Arc<CppLanguage>`により共有は安全。`CppLanguage`はステートレスで**データ競合の可能性は低い**
  - `Send/Sync`境界の明示はこのチャンクには現れない

エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 言語設定が存在しない | settings.languagesに"cpp"キーなし | 有効（true）を返す | unwrap_or(true) | 実装済 |
| パーサー生成失敗 | CppParser::new() -> Err(e) | IndexErrorへ変換してErrを返す | map_err(IndexError::General)? | 実装済 |
| エラー詳細の損失 | eが細かな情報を含む | 上位で識別可能な詳細が保持される | Generalに包むのみ | 注意必要 |
| 大文字拡張子 | "CPP"や"c++"など | 仕様に基づき一致判定 | 拡張子は小文字固定 | 不明 |
| 設定構造の不整合 | config.enabledが欠落 | 安全にデフォルトへ | map/unwrap_orでフォールバック | 実装済 |

Rust特有の観点

- 所有権
  - `register`で`Arc::new(CppLanguage)`を作成し、レジストリに移動（所有権移譲）
  - `create_parser/create_behavior`で`Box`に包んだトレイトオブジェクトを返却（所有権を呼び出し側へ）
- 借用
  - `is_enabled`と`create_parser`は`&Settings`の不変参照のみ使用
- ライフタイム
  - 明示的なライフタイム指定は不要。返却する`Box<dyn Trait>`は所有権型でライフタイム独立
- unsafe境界
  - なし
- 並行性・非同期
  - 非同期処理・await境界はなし
  - `Arc`により参照カウントはスレッド安全。内部ミューテーションは行わない
- エラー設計
  - `IndexResult`で戻す。`?`で早期リターン
  - `map_err(IndexError::General)`は情報粒度の低下に注意
  - `unwrap`/`expect`未使用でpanic無し

## Design & Architecture Suggestions

- エラー詳細の保持
  - `map_err(IndexError::General)`の代わりに、元エラーをラップする構造（例: General(Box<dyn Error + Send + Sync>))や`thiserror`の`source`活用で**原因の連鎖**を保持
- 設定利用の拡張
  - `_settings`を`create_parser`へ適用（例: 方言、マクロ処理、インクルードパス）し、**構成駆動**の初期化にする
- 拡張子の扱い
  - 大文字や別名拡張子の取り扱い方針を明示（正規化）し、**一貫性**を担保
- レジストリ登録の結果確認
  - `register`で重複登録検知を設計側で定義（このチャンクには現れない）。重複時の挙動（上書き/拒否）を決める

## Testing Strategy (Unit/Integration) with Examples

ユニットテスト（このチャンクにテストはないため例示）

- id/name/extensionsの基本確認
```rust
#[test]
fn cpp_language_metadata() {
    let lang = crate::parsing::cpp::definition::CppLanguage;
    assert_eq!(lang.id().as_str(), "cpp"); // as_str()はこのチャンクには現れない場合あり
    assert_eq!(lang.name(), "C++");
    let exts = lang.extensions();
    assert!(exts.contains(&"cpp"));
    assert!(exts.contains(&"cc"));
}
```

- default_enabled/is_enabledの判定
```rust
#[test]
fn cpp_language_enabled_logic() {
    use crate::parsing::LanguageDefinition;
    let lang = crate::parsing::cpp::definition::CppLanguage;
    // Settingsの具体型/生成はこのチャンクには現れないため擬似コード
    let settings: &crate::Settings = /* 既存の設定 */;
    let enabled = lang.is_enabled(settings);
    // 設定に依存するため、期待値はテスト用のsettings構築に合わせる
}
```

- create_parserの成功/失敗
```rust
#[test]
fn cpp_language_create_parser() {
    use crate::parsing::LanguageDefinition;
    let lang = crate::parsing::cpp::definition::CppLanguage;
    let settings: &crate::Settings = /* 既存の設定 */;
    let parser_res = lang.create_parser(settings);
    match parser_res {
        Ok(p) => { /* pの型はdyn LanguageParser。基本的なメソッドが呼べるかはこのチャンクには現れない */ }
        Err(e) => { /* IndexErrorのバリアント/メッセージ確認はこのチャンクには現れない */ }
    }
}
```

- registerの挙動
```rust
#[test]
fn cpp_language_registers() {
    let mut registry: crate::parsing::LanguageRegistry = /* 生成 */;
    crate::parsing::cpp::definition::register(&mut registry);
    // registryから"cpp"の言語が取得できるかの検証はこのチャンクには現れない
}
```

統合テスト（構造はこのチャンクには現れない）
- レジストリ初期化→C++ファイル投入→パーサー/ビヘイビアが期待通りに呼ばれるか
- 設定でC++を無効にした場合にスキップされるか

## Refactoring Plan & Best Practices

- エラーハンドリング改善
  - `map_err(IndexError::General)`を**より表現力のあるエラー型**に変更し、元エラーを`source`として保持
- `_settings`の活用
  - `create_parser`に設定を渡して構成可能にする（未使用の警告抑止よりも意味のある使用へ）
- API明確化
  - `is_enabled`で`default_enabled()`を利用して**デフォルト値の一貫性**表現（e.g., `unwrap_or(self.default_enabled())`)
- ドキュメンテーション
  - `extensions`の仕様（大小文字、重複、別名）を明記
- テストの充実
  - 設定に基づく`is_enabled`の境界ケースを網羅（キー無し、キー有りtrue/false）

## Observability (Logging, Metrics, Tracing)

- ロギング
  - `create_parser`失敗時に**WARN/ERRORログ**を出す（現在はエラー返却のみ）
- メトリクス
  - パーサー生成失敗回数、言語別有効/無効判定結果のカウンタを導入
- トレーシング
  - レジストリ登録、パーサー生成の**スパン**を付与して初期化フェーズの可視性を向上

このチャンクにはロギング/メトリクスの実装は現れないため、提案のみ。

## Risks & Unknowns

- `IndexError::General`の**具体的仕様が不明**（元エラーの保持可否）
- `Settings`/`LanguageRegistry`の**詳細APIが不明**（テスト/使用例の具体化が困難）
- `CppParser::new()`がどのような条件で失敗するか**不明**
- `LanguageId::as_str()`など補助APIの存在は**不明**（例示では仮定）
- `Send/Sync`境界の明示が**不明**。ただし`Arc`とステートレス設計により大きな問題は想定しにくい