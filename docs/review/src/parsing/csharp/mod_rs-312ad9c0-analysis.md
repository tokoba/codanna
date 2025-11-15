# parsing\csharp\mod.rs Review

## TL;DR

- ✅ 目的: codannaにおける**C#言語サポートの集約モジュール**。サブモジュール（parser/behavior/resolution/definition/audit）を公開し、主要型を再エクスポート。
- ✅ 公開API: **CSharpParser**, **CSharpBehavior**, **CSharpLanguage** を再エクスポートして、利用者がcsharpパーサと挙動を簡単に参照可能。
- ℹ️ コアロジック: このファイルには**処理ロジックなし**。解析・解決・ルール適用は各サブモジュールに分離（このチャンクには現れない）。
- ⚠️ 重大リスク（推定）: サブモジュール実装の**スレッド安全性**や**エラー設計**、**Tree-sitter依存**が本ファイルからは不明。登録関数の可視性（pub(crate)）により初期化の順序／到達性に注意。
- 🧪 テスト指針: 再エクスポートの**存在保証**と**言語登録の整合性**をコンパイル時・ドキュメントテストで確認。
- 🚀 パフォーマンス: 本モジュールは**定数時間**での再エクスポートのみ。実負荷はparser/resolution側（このチャンクには現れない）。

## Overview & Purpose

このファイルは codanna の C#言語サポートの「入口」として機能する*モジュールアグリゲータ*です。以下を提供します。

- C#の解析・関係検出・コードインテリジェンスを**担当するサブモジュールの公開**（audit, behavior, definition, parser, resolution）
- 使い勝手のための**主要型の再エクスポート**（CSharpParser, CSharpBehavior, CSharpLanguage）
- 内部用途の**言語登録関数の再エクスポート**（pub(crate) register）

モジュールドキュメントによると、対応範囲は以下（実装はこのチャンクには現れない）:
- シンボル抽出: クラス/構造体/インターフェイス/メソッド/プロパティ/フィールド/イベント/列挙型等、可視性修飾子、シグネチャ
- 関係検出: メソッド呼び出し、インターフェイス実装、using指令
- インテリジェンス: 名前空間追跡、スコープ解決、インポート解決
- アーキテクチャ: parser（Tree-sitter）, behavior（言語規則）, resolution（名前解決）, definition（登録）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Module | audit | pub | 監査・分析補助（詳細は不明） | 不明 |
| Module | behavior | pub | C#特有の振る舞い・処理規則の適用 | 不明 |
| Module | definition | pub | 言語登録・設定（レジストリ連携） | 不明 |
| Module | parser | pub | Tree-sitterによるAST走査とシンボル抽出 | 不明 |
| Module | resolution | pub | シンボル参照の解決（名前解決/インポート解決） | 不明 |
| Re-export | CSharpBehavior | pub use | 言語固有の振る舞いの型（詳細は不明） | 低 |
| Re-export | CSharpLanguage | pub use | C#言語の登録/メタ情報型（詳細は不明） | 低 |
| Re-export | CSharpParser | pub use | C#コードの解析器（詳細は不明） | 低 |
| Re-export | register | pub(crate) use | レジストリ登録関数（内部限定） | 低 |

参考コード（公開要素の宣言部。関数は存在しないため抜粋は全体）:
```rust
pub mod audit;
pub mod behavior;
pub mod definition;
pub mod parser;
pub mod resolution;

pub use behavior::CSharpBehavior;
pub use definition::CSharpLanguage;
pub use parser::CSharpParser;

// Re-export for registry registration
pub(crate) use definition::register;
```

### Dependencies & Interactions

- 内部依存（モジュール間の想定関係）
  - parser → behavior: AST抽出後に言語規則による正規化・補正（推定、ドキュメント記載に基づく）
  - parser/resolution 相互作用: 参照やインポートを解決（推定）
  - definition: 言語の登録と設定を提供
  - audit: 監査・補助的分析（このチャンクには現れない）
- 外部依存（表形式、推定含む）

| 依存 | 種別 | 用途 | 備考 |
|------|------|------|------|
| Tree-sitter | crate/ライブラリ | C#のAST生成・走査 | ドキュメントに明記（具体クレート名は不明） |
| codanna::parsing::LanguageParser | 内部トレイト（推定） | パーサの共通インターフェイス | ドキュメント例に登場 |
- 被依存推定（このモジュールを利用する箇所）
  - codannaの言語レジストリ
  - 解析パイプライン（C#解析が必要な機能）
  - シンボル抽出・可視化ツール

## API Surface (Public/Exported) and Data Contracts

公開API一覧（このファイルが直接輸出するもの）:

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| CSharpParser | 不明（このチャンクには現れない） | C#コードの解析 | 不明 | 不明 |
| CSharpBehavior | 不明（このチャンクには現れない） | C#特有の処理規則（振る舞い） | 不明 | 不明 |
| CSharpLanguage | 不明（このチャンクには現れない） | 言語登録・設定のための型 | 不明 | 不明 |
| audit(mod) | module公開 | 監査/補助分析 | O(1)（参照） | O(1) |
| behavior(mod) | module公開 | 言語規則適用 | O(1)（参照） | O(1) |
| definition(mod) | module公開 | 言語登録/設定 | O(1)（参照） | O(1) |
| parser(mod) | module公開 | AST走査と抽出 | O(1)（参照） | O(1) |
| resolution(mod) | module公開 | 名前解決 | O(1)（参照） | O(1) |

各API詳細（利用者視点の補足。実装はこのチャンクには現れないため不明点を明記）:

- CSharpParser
  1. 目的と責務: C#ソースからASTを生成し、**シンボル抽出**と**関係検出**を行うためのフロントエンド。
  2. アルゴリズム: *Tree-sitterベースのAST走査*（詳細は不明）。
  3. 引数: 不明。
  4. 戻り値: 不明。ただしドキュメント例より、`new()`が`Result`を返す可能性が高い。
  5. 使用例:
     ```rust
     use codanna::parsing::csharp::{CSharpParser, CSharpBehavior};
     // traitはドキュメント例に基づく
     use codanna::parsing::LanguageParser;

     // パーサ生成（失敗時はエラー）
     let mut parser = CSharpParser::new().expect("Failed to create parser");
     let behavior = CSharpBehavior::new();
     // parser + behavior を用いた抽出・解析（詳細APIはこのチャンクには現れない）
     ```
  6. エッジケース:
     - パーサ初期化失敗（Tree-sitterの文法ロード失敗など）: エラーを返すべき
     - 無効なC#コード: 部分的抽出またはエラー
     - 非対応構文: フォールバック動作（不明）

- CSharpBehavior
  1. 目的と責務: 言語特有の**可視性/スコープ/呼び出しコンテキスト**などの補正・ルール適用。
  2. アルゴリズム: 不明。
  3. 引数/戻り値: 不明。
  4. 使用例:
     ```rust
     let behavior = CSharpBehavior::new();
     // behavior によるポリシー適用（詳細APIはこのチャンクには現れない）
     ```
  5. エッジケース:
     - 修飾子解釈の不一致（protected internal 等）: 解決規則の優先度衝突（不明）

- CSharpLanguage
  1. 目的と責務: 言語登録・構成情報の保持、レジストリに渡すための型。
  2. アルゴリズム: 不明。
  3. 引数/戻り値: 不明。
  4. 使用例:
     ```rust
     use codanna::parsing::csharp::CSharpLanguage;
     let lang = CSharpLanguage::default(); // 署名は不明。例示のみ
     ```
  5. エッジケース:
     - レジストリとの互換性不一致: 登録失敗（不明）

- register（内部）
  - 可視性: `pub(crate)`。レジストリ登録に使用。外部からの直接呼び出し不可。

## Walkthrough & Data Flow

このmod.rs自体にはデータフローは存在しませんが、公開モジュール群の**想定的な流れ**は以下の通り（ドキュメント記載に基づく。実装詳細はこのチャンクには現れない）:

1. ユーザーが**CSharpParser**を生成して、C#ソースコードを解析（AST生成・走査）。
2. **CSharpBehavior**が言語特有のルール（可視性、シグネチャ、呼び出しコンテキスト）を適用し、抽出結果を補正。
3. **resolution**が**名前解決**、**using指令**の**インポート解決**、**参照解決**を実施。
4. **definition**が言語をレジストリへ登録し、**CSharpLanguage**で構成を提供。
5. 必要に応じて**audit**が監査/計測的な補助（不明）。

根拠（このファイルの要素）:
- `pub mod parser;` / `pub use parser::CSharpParser;` によりパーサの公開が確認可能。
- `pub mod behavior;` / `pub use behavior::CSharpBehavior;` により言語規則モジュールの公開が確認可能。
- `pub mod resolution;` により解決モジュールの公開が確認可能。
- `pub mod definition;` / `pub use definition::CSharpLanguage;` と `pub(crate) use definition::register;` で登録面が確認可能。

このモジュールは全体の**ファサード**として、利用者が csharp コンポーネントへ容易にアクセスできるようにしています。

## Complexity & Performance

- 本ファイルは**モジュール宣言と再エクスポートのみ**で、計算コストは事実上**O(1)**、追加メモリも**O(1)**。
- 実運用の負荷要因（I/O/ネットワーク/DB）は**このチャンクには現れない**。実際の負荷はparser（AST生成・走査）やresolution（名前解決）に依存します。
- スケール限界やボトルネックの評価は、各サブモジュールの実装が必要（不明）。

## Edge Cases, Bugs, and Security

セキュリティチェックリストの評価（このファイルに限定）:
- メモリ安全性: このファイルには**所有権移動/借用/unsafe**は登場しない。再エクスポートのみで安全性問題は見当たらない。
- インジェクション（SQL/Command/Path traversal）: 該当なし（ロジックなし）。
- 認証・認可: 該当なし。
- 秘密情報: ハードコードやログ漏えいの懸念はこのファイルには該当なし。
- 並行性: スレッド安全性、Race/Deadlock は**このチャンクには現れない**。CSharpParserなどの内部設計次第（不明）。

Rust特有の観点（このファイルに限定）:
- 所有権/借用/ライフタイム: 言及箇所なし（不明）。
- unsafe境界: **unsafe未使用**。
- Send/Sync: トレイト境界情報は不明。
- 非同期/await: 該当なし。
- エラー設計: `CSharpParser::new()`が`Result`を返す示唆はドキュメントにあるが、詳細は不明。

詳細エッジケース一覧（このモジュール視点の運用上のシナリオ）:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| パーサ初期化失敗 | Grammar未ロード | Err(CreateParser) | このチャンクには現れない | 不明 |
| レジストリ未登録 | register未呼び出し | 言語が利用不可 | このチャンクには現れない | 不明 |
| 再エクスポート不整合 | CSharpParser型名変更 | コンパイルエラーで検出 | このチャンクには現れない | 不明 |
| ユーザーが内部APIを誤用 | pub(crate) registerへ外部からアクセス | 不可能（可視性で遮断） | 宣言済み | 良好 |

## Design & Architecture Suggestions

- 再エクスポートの**プレリュードモジュール**提供（例: `parsing::csharp::prelude`）で頻用型を一括公開。
- 仕様ドキュメントと実装の**整合性検証**をドキュメントテストで担保（`CSharpParser::new()`の戻り型など）。
- **Feature gate**の整理（例: `csharp`機能フラグ）により、依存最小化とビルド時間短縮。
- **Send/Sync保証**が必要なら型レベルで明示し、並行解析の安全性を明確化（このチャンクには現れない）。
- **Error型の共通化**（解析/解決/挙動適用間で`thiserror`等による一貫性）を推奨。

## Testing Strategy (Unit/Integration) with Examples

このファイル自体に対するテストは主に**公開要素の存在と到達性**の検証になります。以下は例です。

- コンパイルテスト（re-exportの存在確認）
```rust
// tests/csharp_exports.rs
use codanna::parsing::csharp::{CSharpParser, CSharpBehavior, CSharpLanguage};

#[test]
fn csharp_reexports_exist() {
    // 型の到達性のみ確認。インスタンス生成はこのチャンクには現れないため省略。
    fn _use_types(_: Option<CSharpParser>, _: Option<CSharpBehavior>, _: Option<CSharpLanguage>) {}
}
```

- ドキュメントテスト（モジュールコメントの例に準拠）
```rust
/// ```no_run
/// use codanna::parsing::csharp::{CSharpParser, CSharpBehavior};
/// use codanna::parsing::LanguageParser;
///
/// let mut parser = CSharpParser::new().expect("Failed to create parser");
/// let behavior = CSharpBehavior::new();
/// ```
```

- レジストリ到達性（内部APIの可視性を確認）
```rust
// このテストはcrate内でのみ可能（pub(crate)）
use crate::parsing::csharp::register;

#[test]
fn csharp_register_is_internal() {
    // 到達できるのはcrate内のみ。振る舞いは不明。
    let _ = &register;
}
```

- 統合テスト（parser/behavior/resolutionの連携）
  - 実装はこのチャンクには現れないため、具体コードは不明。AST抽出→解決→検証の**ゴールデンファイル**テストを推奨。

## Refactoring Plan & Best Practices

- mod.rsは**薄いファサード**のまま維持し、ロジックはサブモジュールへ集約。
- 再エクスポートする型は**中核の利用導線**（Parser/Behavior/Language）に限定し、命名整合性を保つ。
- 将来的に**prelude**導入で頻用型を一括公開、ユーザーのimport記述の簡略化。
- クレート外へ出すAPIは**最小**に保ち、内部実装詳細（registerなど）は`pub(crate)`で遮断。

## Observability (Logging, Metrics, Tracing)

- 本ファイルには観測コードは**存在しない**。
- サブモジュール側での**構文解析エラー率、解決失敗件数、処理時間**などのメトリクス発行を推奨。
- トレーシング（`tracing` crate等）の**span設計**は parser→behavior→resolution の主要フェーズに付与すると効果的（このチャンクには現れない）。

## Risks & Unknowns

- 実装詳細の**不明点**: CSharpParser/CSharpBehavior/CSharpLanguageの型詳細、エラー型、スレッド安全性、ツールチェーン（具体的なTree-sitterクレート）。
- **初期化順序**: registerの呼び出し時機が外部からは見えない（pub(crate)）。起動時登録の失敗がどこで検知されるかは不明。
- **仕様の拡張**: C#の新構文（record structなど）対応状況はドキュメント上で言及あるが、実装の範囲は**このチャンクには現れない**。
- **互換性**: Tree-sitterのバージョン差異、C#言語仕様バージョンとの整合性の維持が必要（詳細不明）。