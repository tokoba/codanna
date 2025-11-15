# parsing.mod.rs Review

## TL;DR

- このファイルは各言語向けのパーサと振る舞い、レジストリ、解決ロジック、ユーティリティ関数を「再エクスポート」する集約モジュール（APIゲートウェイ）です。
- 主要公開APIは、ParserFactory、LanguageParser、LanguageRegistry/get_registry、ParserContext、LanguageBehavior、各言語の Behavior/Parser、parser系ユーティリティ群（safe_substring_window 等）です。
- 実行時ロジックはなく、複雑性はほぼゼロ。ただし「API表面の広さ」「命名衝突」「依存関係の結合度」が運用上のリスクになります。
- Rust安全性・エラー・並行性観点では、このチャンクではunsafeや共有可変は登場せず安全ですが、詳細な実装は他モジュール側に依存し「不明」な点が多いです。
- 重大リスクは、再エクスポートの変更が下流利用者のビルドを破壊する可能性、シンボル名の衝突、SemVer上の破壊的変更の伝播です。

## Overview & Purpose

- 目的: 📦 parsing/mod.rs は、クレート内の多言語パーサ関連要素をひとつの名前空間（parsing）から参照できるようにする集約モジュールです。下流コードは `use crate::parsing::*;` 等で必要な型・関数・トレイトをまとめて取得できます。
- 範囲: 再エクスポートの対象は、言語別モジュール（c, cpp, csharp, gdscript, go, kotlin, php, python, rust, typescript）、パーサの共通インタフェースやユーティリティ（parser, context, language, language_behavior, method_call）、ファクトリ（factory）、レジストリ（registry）、型解決（resolution）、インポート解析（import）です。
- 実行ロジック: 本ファイルには実行関数・状態・アルゴリズムはありません。公開API整備とモジュール境界の整列が主です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Module | behavior_state | pub | 振る舞い状態（詳細不明） | 不明 |
| Module | c / cpp / csharp / gdscript / go / kotlin / php / python / rust / typescript | pub | 各言語固有のパーサと振る舞いを提供 | 不明 |
| Module | context | pub | パース時のコンテキスト管理 | 不明 |
| Module | factory | pub | パーサ生成のファクトリ | 不明 |
| Module | import | pub | インポート解析 | 不明 |
| Module | language | pub | 言語識別子 | 不明 |
| Module | language_behavior | pub | 言語の共通振る舞いメタデータ | 不明 |
| Module | method_call | pub | メソッド呼び出し表現 | 不明 |
| Module | parser | pub | パーサの共通トレイト/構造体/ユーティリティ | 不明 |
| Module | registry | pub | 言語レジストリとID、定義、エラー、取得関数 | 不明 |
| Module | resolution | pub | ジェネリクス/継承の解決スコープ | 不明 |
| Item（不明） | CBehavior / CParser | pub use | C言語の振る舞い・パーサ | 低 |
| Item（不明） | CppBehavior / CppParser | pub use | C++の振る舞い・パーサ | 低 |
| Item（不明） | CSharpBehavior / CSharpParser | pub use | C#の振る舞い・パーサ | 低 |
| Item（不明） | GdscriptBehavior / GdscriptParser | pub use | GDScriptの振る舞い・パーサ | 低 |
| Item（不明） | GoBehavior / GoParser | pub use | Goの振る舞い・パーサ | 低 |
| Item（不明） | KotlinBehavior / KotlinParser | pub use | Kotlinの振る舞い・パーサ | 低 |
| Item（不明） | PhpBehavior / PhpParser | pub use | PHPの振る舞い・パーサ | 低 |
| Item（不明） | PythonBehavior / PythonParser | pub use | Pythonの振る舞い・パーサ | 低 |
| Item（不明） | RustBehavior / RustParser | pub use | Rustの振る舞い・パーサ | 低 |
| Item（不明） | TypeScriptBehavior / TypeScriptParser | pub use | TypeScriptの振る舞い・パーサ | 低 |
| Item（不明） | ParserFactory / ParserWithBehavior | pub use | パーサ生成のファクトリと戻り型 | 中 |
| Item（不明） | ParserContext / ScopeType | pub use | パースコンテキスト、スコープ種別 | 中 |
| Item（不明） | Language | pub use | 対応言語の列挙/識別 | 低 |
| Item（不明） | LanguageBehavior / LanguageMetadata | pub use | 言語振る舞いトレイト/メタデータ | 中 |
| Item（不明） | MethodCall | pub use | メソッド呼び出しデータモデル | 低 |
| Item（不明） | HandledNode / LanguageParser / NodeTracker / NodeTrackingState | pub use | ノード処理、パーサトレイト、ノード追跡 | 中 |
| Function（不明） | safe_substring_window / safe_truncate_str / truncate_for_display | pub use | 文字列安全処理ユーティリティ | 低 |
| Item（不明） | LanguageDefinition / LanguageId / LanguageRegistry / RegistryError / get_registry | pub use | 言語定義、ID、レジストリ、エラー、取得関数 | 中 |
| Item（不明） | GenericInheritanceResolver / GenericResolutionContext / InheritanceResolver / ResolutionScope / ScopeLevel | pub use | ジェネリクス/継承解決の支援 | 中 |
| Item（不明） | Import | pub use | インポート表現 | 低 |

Dependencies & Interactions
- 内部依存（推定・このチャンクには実装詳細なし）:
  - ParserFactory → LanguageRegistry / Language → 各言語の Parser/Behavior を選択して構築
  - LanguageParser（トレイト） ← 各言語の Parser が実装
  - ParserContext / NodeTracker → パース中の状態/スコープ追跡に利用
  - resolution.* → AST/型情報からジェネリクス/継承関係の解決に利用
  - method_call / import → パース結果の高レベル表現に利用
- 外部依存: このチャンクには現れない（不明）
- 被依存推定:
  - クレート外/上位層から `crate::parsing::*` に依存し、コード解析、言語横断の機能を呼び出す可能性が高い

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ParserFactory / ParserWithBehavior | 不明 | 言語に応じたパーサと振る舞いの組を生成 | N/A | N/A |
| LanguageParser | 不明（トレイト推定） | パーサの共通インタフェース | N/A | N/A |
| LanguageRegistry / get_registry | 不明 | 利用可能な言語や定義を登録・取得 | N/A | N/A |
| Language / LanguageId | 不明 | 言語の識別 | N/A | N/A |
| LanguageBehavior / LanguageMetadata | 不明 | 言語固有の振る舞い/メタデータの抽象化 | N/A | N/A |
| ParserContext / ScopeType | 不明 | パース時のスコープ・状態管理 | N/A | N/A |
| HandledNode | 不明 | パース済みノードの表現 | N/A | N/A |
| NodeTracker / NodeTrackingState | 不明 | ノード追跡のための状態 | N/A | N/A |
| MethodCall | 不明 | メソッド呼び出しの表現 | N/A | N/A |
| Import | 不明 | インポート表現 | N/A | N/A |
| safe_substring_window | 不明 | サブストリング取得の安全版 | O(n)（推定: 入力長に比例） | O(1)〜O(n)（推定） |
| safe_truncate_str | 不明 | 文字列の安全な切り詰め | O(n)（推定） | O(1) |
| truncate_for_display | 不明 | 表示用の安全な切り詰め | O(n)（推定） | O(1) |
| GenericResolutionContext | 不明 | ジェネリクス解決コンテキスト | N/A | N/A |
| InheritanceResolver / GenericInheritanceResolver | 不明 | 継承/ジェネリクス解決ロジック | N/A | N/A |
| ResolutionScope / ScopeLevel | 不明 | 解決処理のスコープ指定 | N/A | N/A |
| CBehavior/CParser, CppBehavior/CppParser, ...（各言語） | 不明 | 各言語の振る舞い/パーサ | N/A | N/A |

主要APIの詳細（推定と不明を明記）

1) ParserFactory / ParserWithBehavior
- 目的と責務: 言語指定に応じた LanguageParser と対応する LanguageBehavior の組を返すファクトリ。
- アルゴリズム: 不明（このチャンクには現れない）。おそらく LanguageRegistry に登録済み定義から選択。
- 引数: 不明
- 戻り値: 不明（ParserWithBehavior が複合型である可能性）
- 使用例（擬似コード・インタフェース不明のため参考のみ）:
```rust
use crate::parsing::{ParserFactory, Language};

fn create_parser_for(lang: Language) {
    // 具体シグネチャ不明。以下は概念例。
    // let factory = ParserFactory::new();
    // let pwb = factory.create(lang).expect("parser available");
    // let parser = pwb.parser;
    // let behavior = pwb.behavior;
}
```
- エッジケース:
  - 未対応言語を指定した場合
  - レジストリ未初期化/空の状態
  - バージョン不一致による取得失敗

2) LanguageParser
- 目的と責務: 各言語パーサの共通インタフェース（トレイト推定）。
- アルゴリズム: 不明
- 引数/戻り値: 不明
- 使用例（概念例）:
```rust
use crate::parsing::LanguageParser;

// fn run<P: LanguageParser>(parser: P, src: &str) {
//     let nodes = parser.parse(src); // 仮
// }
```
- エッジケース:
  - 無効な入力
  - 巨大入力に対するメモリ負荷
  - 不正な文字列境界（UTF-8）

3) LanguageRegistry / get_registry
- 目的と責務: 利用可能な言語一覧や定義を提供し、ファクトリや上位コードから参照可能にする。
- アルゴリズム: 不明
- 引数/戻り値: 不明
- 使用例（概念例）:
```rust
use crate::parsing::get_registry;

fn list_languages() {
    // let reg = get_registry(); // シグネチャ不明
    // for def in reg.definitions() { /* ... */ }
}
```
- エッジケース:
  - 登録なし/重複登録
  - スレッドセーフな初期化（推定）

4) ParserContext / ScopeType
- 目的と責務: パース中のスコープ/状態を管理。
- アルゴリズム・引数・戻り値: 不明
- 使用例（概念例）:
```rust
use crate::parsing::{ParserContext, ScopeType};
// let mut ctx = ParserContext::new(ScopeType::File);
// ctx.enter(ScopeType::Function("foo".into()));
```
- エッジケース:
  - スコープの不整合（push/popのミス）
  - ネスト過多によるスタック増大

5) safe_substring_window / safe_truncate_str / truncate_for_display
- 目的と責務: 文字列操作の安全化（UTF-8境界、範囲外アクセス防止）。
- アルゴリズム: 不明（名称から境界チェックとサロゲート対応が推定される）
- 引数/戻り値: 不明
- 使用例（概念例）:
```rust
use crate::parsing::{safe_truncate_str, truncate_for_display};

// let s = "こんにちは世界";
// let short = safe_truncate_str(s, 5);
// let display = truncate_for_display(s, 20);
```
- エッジケース:
  - サロゲートペア/結合文字の途中切断
  - 0長・負値（無効）入力
  - 非UTF-8（&[u8]からの誤用）

6) Resolution（GenericResolutionContext / InheritanceResolver 等）
- 目的と責務: 型パラメータや継承関係の解決。
- アルゴリズム/引数/戻り値: 不明
- 使用例（概念例）:
```rust
use crate::parsing::{GenericResolutionContext, InheritanceResolver};
// let ctx = GenericResolutionContext::new();
// let base = InheritanceResolver::resolve(&ctx, current_type);
```
- エッジケース:
  - 多重継承/ダイヤモンド問題
  - 未束縛ジェネリクス

7) MethodCall / Import
- 目的と責務: コード中のメソッド呼び出し/インポート構造を表現。
- 詳細: 不明

## Walkthrough & Data Flow

概念的データフロー（このチャンクにはコアロジック非掲載・推定）:
- 入力: ソースコード文字列、対象言語（Language/LanguageId）
- レジストリ参照: get_registry → LanguageDefinition を取得
- ファクトリ: ParserFactory → 言語に対応する Parser/Behavior を組み立て（ParserWithBehavior）
- パース: LanguageParser 実装がソースを解析し HandledNode（や AST）を生成
- 追跡: NodeTracker/NodeTrackingState がノードやスコープを追跡
- 解決: resolution.* により型/継承/ジェネリクスを解決
- 出力: MethodCall/Import 等の高レベル表現、表示用には truncate_for_display 等で整形

上記は典型的構成の推定であり、このチャンクには実装詳細は現れません。

## Complexity & Performance

- 本ファイル自体は宣言と再エクスポートのみで、実行時コストはゼロ（O(1)）。メモリも追加負荷なし。
- コンパイル時:
  - 再エクスポートの網羅により、ビルド時に多数のシンボルが可視化されます。モジュール数の増加はコンパイル時間に影響する可能性があります。
- 実運用負荷要因（推定・他モジュール依存）:
  - I/O/ネットワーク/DBはこのチャンクに現れない。
  - 文字列ユーティリティ（safe_*）は入力長に比例した O(n) 処理が想定されます。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト（このチャンクでは該当箇所なし/不明が多数）
- メモリ安全性: このファイルに unsafe はなく、バッファオーバーフロー/Use-after-free/整数オーバーフローは「該当なし」。文字列ユーティリティの詳細は parser モジュール側で「不明」。
- インジェクション: SQL/Command/Path traversal 等の外部I/Oは「該当なし」。
- 認証・認可: 機構は「該当なし」。
- 秘密情報: ハードコード/ログ漏洩は「該当なし」。
- 並行性: スレッド/タスク管理や共有状態はこのファイルには「該当なし」。

エッジケース一覧（このファイルの性質上、再エクスポートに関するもの）
| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| シンボル名の衝突 | 新たなモジュールに Language という型追加 | 再エクスポート時に曖昧性なくコンパイル | このチャンクでは解決策記載なし | 不明 |
| 誤った再エクスポート | 存在しない型を pub use | コンパイルエラーで検出される | Rustコンパイラが検出 | OK（ビルド時） |
| 破壊的変更の伝播 | 下位モジュールの型名変更 | 上位の再エクスポートも更新が必要 | 手動同期が必要 | リスクあり |
| 可視性の不整合 | 下位が pub(crate) の型を pub use | コンパイルエラー | コンパイラが弾く | OK（ビルド時） |
| 依存循環 | 下位が parsing に依存し、parsing が下位を再エクスポート | 循環回避 | このチャンクに対策なし | 不明 |

Rust特有の観点（このチャンクに限る）
- 所有権/借用/ライフタイム: 該当なし（型宣言・関数なし）
- unsafe境界: unsafe ブロックなし
- 並行性・非同期（Send/Sync、データ競合、await境界、キャンセル）: 該当なし
- エラー設計（Result/Option、panic、エラー変換）: RegistryError が再エクスポートされるが詳細は「不明」

## Design & Architecture Suggestions

- プレリュード導入: `parsing::prelude` を用意し、よく使う型（Language、ParserFactory、LanguageParser、ParserContext、safe_* 等）を厳選して再エクスポート。API表面の過広化を抑制。
- フィーチャーフラグ: 言語ごとに `feature = "lang_rust"` 等のゲートを設け、不要な言語パーサをビルドから除外。コンパイル時間短縮と依存削減。
- ドキュメント整備: 各 `pub use` に `//!` でクレート/モジュールレベルの説明、`#[doc(inline)]` の方針を明示。下流利用者の探索性を向上。
- 名前空間の明確化: `parsing::languages::{RustParser,...}` のようにグルーピングし、トップレベルのシンボル密度を低減。
- 互換性ポリシー: 再エクスポートの変更は SemVer の「破壊的変更」に相当しうるため、変更管理を厳格化。

## Testing Strategy (Unit/Integration) with Examples

- コンパイル確認テスト（doctest/ユニット）
  - 再エクスポートの存在確認（型がインポートできること）
```rust
// tests/parsing_exports.rs
use crate::parsing::{
    Language, ParserFactory, LanguageParser, ParserContext, LanguageRegistry, get_registry,
    safe_truncate_str, RustParser, RustBehavior,
};

#[test]
fn parsing_exports_are_visible() {
    // 参照できればOK（型/関数の存在確認）。具体メソッドは不明。
    let _ = get_registry(); // シグネチャ不明・呼び出し可能かは実装次第
    let _ = safe_truncate_str; // 関数シンボルが見えること
}
```
- 統合テスト（概念）
  - レジストリ → ファクトリ → パーサ生成 → 簡単な入力の解析 → 出力が期待形式であること（詳細は他モジュールに依存し、このチャンクでは不明）。
- プロパティテスト（ユーティリティ）
  - safe_* 系は UTF-8 境界や結合文字を含む入力で「不正なパニック/不正境界アクセスがない」ことを検証（関数シグネチャ不明につき概念のみ）。

## Refactoring Plan & Best Practices

- 再エクスポートの層構造化:
  - `parsing::core`（Language、ParserFactory、LanguageParser、Context、safe_*）
  - `parsing::languages`（各言語の Parser/Behavior）
  - `parsing::resolution`（ジェネリクス/継承解決）
  - `parsing::registry`（レジストリ）
- 明示的な `pub use` の整理:
  - 下流でほぼ使われない内部型は公開しない（情報隠蔽）。
- 一貫した命名規約:
  - `Gdscript` → `GDScript` などの頭字語整合性を検討（ただし既存API破壊の可能性あり、慎重に）。
- ドキュメント/例の追加:
  - 主要API（Factory、Parser、Registry）に「短い使用例」を添付し探索性を向上。

## Observability (Logging, Metrics, Tracing)

- 本ファイルには観測コードなし。実際のパーサ・レジストリ・解決処理で以下を推奨:
  - ロギング: パーサ生成、言語選択、解析失敗時の要約ログ（PII/秘密情報は除外）。
  - メトリクス: 解析時間、ノード数、失敗率、ユーティリティ関数のエラー比率。
  - トレーシング: ファクトリ→パーサ→解決のスパンを関連付け、遅延箇所の特定を容易に。

## Risks & Unknowns

- Unknowns:
  - 各APIの具体的シグネチャ、戻り値、エラー型、所有権/借用の詳細、並行性対応はこのチャンクには現れない。
  - safe_* ユーティリティの具体的な境界処理、UTF-8対応詳細は不明。
  - Registry の初期化方法（静的/動的/スレッドセーフ）は不明。
- Risks:
  - 再エクスポートの変更が下流コードを破壊する（ビルド/ランタイム）可能性。
  - シンボル密度の高さによる探索性低下、誤用。
  - 言語モジュールの増加に伴いコンパイル時間とメンテナンス負荷が上昇。
  - Feature gating がない場合、不要な言語まで常にビルド対象になる。