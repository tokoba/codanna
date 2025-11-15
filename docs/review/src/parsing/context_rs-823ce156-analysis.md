# parsing/context.rs Review

## TL;DR

- 目的: **AST走査中のスコープ追跡**を統一し、シンボル解決に必要な**ScopeContext**を生成するためのユーティリティ。
- 主な公開API: **ParserContext::current_scope_context**（中核）、**enter_scope/exit_scope**（スタック操作）、**is_in_***（状態判定）、**set_current_* / current_***（クラス/関数名管理）、**ScopeType::function/hoisting_function**（スコープ種別作成）。
- 複雑箇所: **current_scope_context**でのスコープ種別に応じた分岐と、親情報（関数/クラス名・種別）の決定ロジック。
- 重大リスク: 親情報が**current_function**優先で**current_class**にフォールバックする実装のため、**クラス内メソッドでも関数名未設定時は親情報がNone**になり得る（テストで想定済）。スコープ入退出の漏れによるスタック不整合。
- 安全性: **unsafe未使用**、所有権/借用は**&mut self**中心で健全。並行性は**Send/Sync**的には安全だが、**同一インスタンスの並行ミューテーションには同期が必要**。
- パフォーマンス: 主要コストは**current_scope_context/is_in_***のスコープスタック走査（O(n)）。通常nは小さいが深くネストすると増加。
- 改善提案: **RAIIガード**でenter/exitの漏れ防止、**親情報の決定仕様の明文化/強化**、**StackのSmallVec化**、**観測用ログ**追加。

## Overview & Purpose

本モジュールは、言語横断のパーサーがASTを走査する際に、現在の**スコープ種別**と**親コンテキスト情報**（クラス名/関数名、hoisting有無）を追跡し、後段のシンボル解決器が利用する**ScopeContext**（crate::symbol）を生成するための**共有コンテキスト**を提供します。主に以下を担います。

- スコープの入れ子関係をスタックで管理（Moduleを起点）。
- クラス/関数名の現在値を管理。
- 現在位置に対応する**ScopeContext**を組み立てて返却。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | ScopeType | pub | スコープ種別の表現（Global/Module/Function{hoisting}/Class/Block/Package/Namespace） | Low |
| Impl | ScopeType::function | pub | 非hoisting関数スコープ生成（デフォルト） | Low |
| Impl | ScopeType::hoisting_function | pub | hoisting関数スコープ生成（JS/TS用） | Low |
| Struct | ParserContext | pub | スコープスタックと現在のクラス/関数名の追跡 | Med |
| Impl | ParserContext::new | pub | 初期化（Moduleスコープで開始） | Low |
| Impl | ParserContext::enter_scope | pub | スコープ入場（スタックpush） | Low |
| Impl | ParserContext::exit_scope | pub | スコープ退出（スタックpop、クラス/関数名のクリア） | Low |
| Impl | ParserContext::current_scope_context | pub | 現在位置のScopeContextを決定して返す（中核） | Med |
| Impl | ParserContext::is_in_class | pub | 現在Classスコープ内か判定 | Low |
| Impl | ParserContext::is_in_function | pub | 現在Functionスコープ内か判定 | Low |
| Impl | ParserContext::is_module_level | pub | Moduleレベルか判定（Class/Function外） | Low |
| Impl | ParserContext::set_current_class | pub | 現在クラス名の設定 | Low |
| Impl | ParserContext::set_current_function | pub | 現在関数名の設定 | Low |
| Impl | ParserContext::current_class | pub | 現在クラス名の参照取得（Option<&str>） | Low |
| Impl | ParserContext::current_function | pub | 現在関数名の参照取得（Option<&str>） | Low |
| Impl | ParserContext::parameter_scope_context | pub | パラメータ用ScopeContext（定数的） | Low |
| Impl | ParserContext::global_scope_context | pub | グローバル用ScopeContext（定数的） | Low |

### Dependencies & Interactions

- 内部依存
  - ParserContextは内部に`Vec<ScopeType>`と`Option<String>`を保持。
  - current_scope_contextは`self.scope_stack`を逆順走査し、必要に応じて`self.current_function`/`self.current_class`を参照。

- 外部依存（表）
  | 依存先 | 種別 | 用途 | 備考 |
  |--------|------|------|------|
  | crate::symbol::ScopeContext | 型 | 返却するスコープコンテキスト | 具体的な定義はこのチャンクには現れない |
  | crate::types::SymbolKind | 型 | 親種別（Function/Class）の指定 | 値はFunction/Classを使用 |

- 被依存推定
  - 各言語のパーサー（JS/TS、Rust、Javaなど）がAST走査で利用。
  - 後段の**シンボル解決（resolver）**が、返されるScopeContextをもとに名前解決・可視性判断を行う。
  - 具体的な呼び出し箇所はこのチャンクには現れない。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ScopeType::function | `pub fn function() -> Self` | 非hoisting関数スコープ生成 | O(1) | O(1) |
| ScopeType::hoisting_function | `pub fn hoisting_function() -> Self` | hoisting関数スコープ生成 | O(1) | O(1) |
| ParserContext::new | `pub fn new() -> Self` | Moduleスコープで初期化 | O(1) | O(1) |
| ParserContext::enter_scope | `pub fn enter_scope(&mut self, scope_type: ScopeType)` | スコープ入場 | O(1) | O(1) |
| ParserContext::exit_scope | `pub fn exit_scope(&mut self)` | スコープ退出（Moduleは保持） | O(1) | O(1) |
| ParserContext::current_scope_context | `pub fn current_scope_context(&self) -> ScopeContext` | 現在スコープのScopeContext生成 | O(n) | O(1) |
| ParserContext::is_in_class | `pub fn is_in_class(&self) -> bool` | Classスコープ内判定 | O(n) | O(1) |
| ParserContext::is_in_function | `pub fn is_in_function(&self) -> bool` | Functionスコープ内判定 | O(n) | O(1) |
| ParserContext::is_module_level | `pub fn is_module_level(&self) -> bool` | Moduleレベル判定 | O(n) | O(1) |
| ParserContext::set_current_class | `pub fn set_current_class(&mut self, name: Option<String>)` | 現在クラス名設定 | O(1) | O(1) |
| ParserContext::set_current_function | `pub fn set_current_function(&mut self, name: Option<String>)` | 現在関数名設定 | O(1) | O(1) |
| ParserContext::current_class | `pub fn current_class(&self) -> Option<&str>` | 現在クラス名参照取得 | O(1) | O(1) |
| ParserContext::current_function | `pub fn current_function(&self) -> Option<&str>` | 現在関数名参照取得 | O(1) | O(1) |
| ParserContext::parameter_scope_context | `pub fn parameter_scope_context() -> ScopeContext` | パラメータ用ScopeContext生成 | O(1) | O(1) |
| ParserContext::global_scope_context | `pub fn global_scope_context() -> ScopeContext` | グローバル用ScopeContext生成 | O(1) | O(1) |

以下、主要APIの詳細。

1) ParserContext::current_scope_context
- 目的と責務
  - 現在のスコープスタックから最も具体的なスコープを選び、**ScopeContext**（Local/ClassMember/Package/Global/Module）を返す。
  - 関数・ブロックスコープの場合、親情報（関数名またはクラス名）と**hoisted**フラグを組み合わせて返却。
- アルゴリズム（ステップ分解）
  1. スコープスタックを逆順で走査（最内側から）。
  2. マッチに応じて分岐:
     - Function{hoisting}: 親は`current_function`優先、なければ`current_class`、両方なければNone。`ScopeContext::Local{hoisted, parent_*}`を返す。
     - Block: 同上だが`hoisted=false`固定。
     - Class: `ScopeContext::ClassMember`を返す。
     - Package/Namespace: `ScopeContext::Package`を返す。
     - Global: `ScopeContext::Global`を返す。
     - Module: より具体的なスコープを探すため走査続行。
  3. 走査で適合がない場合、`ScopeContext::Module`を返す。
- 引数
  | 引数 | 型 | 説明 |
  |------|----|------|
  | self | `&self` | 現在のコンテキスト |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | `ScopeContext` | 現在位置に対応するスコープコンテキスト |
- 使用例
  ```rust
  let mut ctx = ParserContext::new();
  ctx.enter_scope(ScopeType::Function { hoisting: false });
  ctx.set_current_function(Some("f".to_string()));
  let sc = ctx.current_scope_context();
  // => ScopeContext::Local { hoisted: false, parent_name: Some("f".into()), parent_kind: Some(SymbolKind::Function) }
  ```
- エッジケース
  - スタックがModuleのみ: Moduleを返す。
  - クラス内メソッドだが`current_function`/`current_class`未設定: 親情報NoneでLocalを返す（テストで期待値化）。
  - 複数ネスト（Block内のFunction等）: 最内側に基づいて返す。

2) ParserContext::enter_scope
- 目的と責務
  - 指定スコープ種別をスタックへpush。クラス/関数名は別メソッドで設定する前提。
- アルゴリズム
  1. スコープ種別に応じたコメント上の注意（名前設定は別途）。
  2. `self.scope_stack.push(scope_type)`。
- 引数
  | 引数 | 型 | 説明 |
  |------|----|------|
  | self | `&mut self` | ミューテーション |
  | scope_type | `ScopeType` | 入場するスコープ |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | `()` | なし |
- 使用例
  ```rust
  let mut ctx = ParserContext::new();
  ctx.enter_scope(ScopeType::Class);
  ctx.set_current_class(Some("C".to_string()));
  ```
- エッジケース
  - 連続push（深いネスト）：問題なし。
  - 名前設定忘れ：親情報がNoneになる可能性。

3) ParserContext::exit_scope
- 目的と責務
  - 現在スコープをpop。ただし**Moduleは決してpopしない**。クラス/関数スコープを脱出する際は、対応する名前を**Noneへクリア**。
- アルゴリズム
  1. `scope_stack.len() > 1`の時のみpop。
  2. 退出したスコープがClassなら`current_class=None`、Functionなら`current_function=None`。
- 引数
  | 引数 | 型 | 説明 |
  |------|----|------|
  | self | `&mut self` | ミューテーション |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | `()` | なし |
- 使用例
  ```rust
  ctx.exit_scope(); // クラス/関数脱出時は名前もクリア
  ```
- エッジケース
  - Moduleのみの状態で呼ぶ: 何もしない（安全）。
  - 入退出不整合（push漏れ/過剰pop）: Module保護により致命的にはならないが、状態不一致の可能性。

4) ParserContext::is_in_class / is_in_function / is_module_level
- 目的と責務
  - 現在が各スコープ内かの判定。`is_module_level`はクラス/関数外であることを確認。
- アルゴリズム
  - `scope_stack.iter().any(...)`で判定。`is_module_level`は`!is_in_class() && !is_in_function()`。
- 引数/戻り値
  | API | 引数 | 戻り値 |
  |-----|------|--------|
  | is_in_class | `&self` | `bool` |
  | is_in_function | `&self` | `bool` |
  | is_module_level | `&self` | `bool` |
- 使用例
  ```rust
  assert!(ctx.is_in_class());
  assert!(!ctx.is_module_level());
  ```
- エッジケース
  - 深いネストでも最内側に依存せず存在判定のみ。

5) ParserContext::set_current_class / set_current_function
- 目的
  - 現在のクラス/関数名を設定（入退出と連動して明示的に呼ぶ）。
- 引数/戻り値
  | API | 引数 | 戻り値 |
  |-----|------|--------|
  | set_current_class | `&mut self, Option<String>` | `()` |
  | set_current_function | `&mut self, Option<String>` | `()` |
- 使用例
  ```rust
  ctx.set_current_function(Some("foo".to_string()));
  ```

6) ParserContext::current_class / current_function
- 目的
  - 現在名の参照を取得（コピー不要）。
- 引数/戻り値
  | API | 引数 | 戻り値 |
  |-----|------|--------|
  | current_class | `&self` | `Option<&str>` |
  | current_function | `&self` | `Option<&str>` |
- 使用例
  ```rust
  if let Some(name) = ctx.current_function() { /* ... */ }
  ```

7) ScopeType::function / ScopeType::hoisting_function
- 目的
  - Functionスコープ種別のショートカット作成（hoisting有無の違い）。
- 使用例
  ```rust
  ctx.enter_scope(ScopeType::hoisting_function());
  ```

8) ParserContext::parameter_scope_context / global_scope_context
- 目的
  - 固定のScopeContextを返すヘルパー。
- 引数/戻り値
  | API | 引数 | 戻り値 |
  |-----|------|--------|
  | parameter_scope_context | なし | `ScopeContext::Parameter` |
  | global_scope_context | なし | `ScopeContext::Global` |
- 使用例
  ```rust
  let param_ctx = ParserContext::parameter_scope_context();
  ```

データ契約（ScopeContextの期待形）
- Local: `{ hoisted: bool, parent_name: Option<...>, parent_kind: Option<SymbolKind> }`
- ClassMember, Package, Global, Module: 列挙体バリアント
- 具体的な内部型（parent_nameの型など）はこのチャンクには現れない。

重要主張の根拠（関数名:行番号）
- 例: current_scope_contextの分岐仕様（関数名: current_scope_context, 行番号: 不明）

## Walkthrough & Data Flow

典型フロー（クラス→メソッド→ブロック→脱出）:
1. `ParserContext::new()`でModule開始。
2. `enter_scope(ScopeType::Class)`でクラスに入る。必要なら`set_current_class(Some("C"))`。
3. `enter_scope(ScopeType::function())`でメソッドに入る。`set_current_function(Some("m"))`。
4. `current_scope_context()`を呼ぶと`Local{ hoisted: false, parent_name:"m", parent_kind:Function }`。
5. `enter_scope(ScopeType::Block)`でブロックに入る。
6. `current_scope_context()`は`Local{ hoisted:false, parent_name:"m", parent_kind:Function }`。
7. `exit_scope()`でブロック脱出、次に関数脱出（`current_function=None`になる）、最後にクラス脱出（`current_class=None`）。

Mermaidフローチャート（current_scope_contextの主要分岐）
```mermaid
flowchart TD
  A[Start: reverse iterate scope_stack] --> B{scope}
  B -->|Function{hoisting}| C[Determine parent: current_function? else current_class? else None]
  C --> D[Return ScopeContext::Local{hoisted, parent_*}]
  B -->|Block| E[Determine parent (same as Function)]
  E --> F[Return ScopeContext::Local{hoisted:false, parent_*}]
  B -->|Class| G[Return ScopeContext::ClassMember]
  B -->|Package/Namespace| H[Return ScopeContext::Package]
  B -->|Global| I[Return ScopeContext::Global]
  B -->|Module| J{Continue to next outer scope}
  J --> B
  B -->|No match found| K[Return ScopeContext::Module]
```
上記の図は`current_scope_context`関数（行番号: 不明）の主要分岐を示す。

## Complexity & Performance

- 時間計算量
  - enter_scope/exit_scope: O(1)
  - current_scope_context: O(n)（スタック深さnに比例）
  - is_in_class/is_in_function: O(n)
  - is_module_level: O(n)（内部でis_in_*を呼ぶため）
  - set_current_*/current_*: O(1)
- 空間計算量
  - スタックはO(n)、nはネスト深さ。
- ボトルネック
  - 深いネストで`current_scope_context`と`is_in_*`が線形走査。通常のASTでは深さは限定的だが、生成コードやDSLで深くなる可能性。
- スケール限界
  - 極端なネスト（>数百）で微小なオーバーヘッド増加。
- 実運用負荷要因
  - I/O/ネットワーク/DBは関与なし。計算中心。

## Edge Cases, Bugs, and Security

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| Module単独でexit_scope | 初期状態でexit_scope | 何もしない（Module維持） | `if len>1`でpop制限 | OK |
| クラス名未設定 | Classに入るがset_current_classしない | ClassMemberは返るが親名は不明 | Classは直にClassMember返却 | OK |
| 関数名未設定 | Functionに入るがset_current_functionしない | Localで親情報None | 親決定はfunction→class→None | OK（意図どおり） |
| クラス内関数でクラス名未設定 | Class入り後Function入り、両名未設定 | Localで親情報None | 上記同様 | OK |
| 深いネストでの判定 | [Class→Function→Block→...n] | 最内側優先で正しい種別 | 逆順走査で対応 | OK |
| 連続exitで過剰pop | Moduleに到達後さらにexit | Moduleは保持される | len>1ガード | OK |

- セキュリティチェックリスト
  - メモリ安全性
    - Buffer overflow: 該当なし（Rust安全、Vec操作は安全API）。
    - Use-after-free: 該当なし（所有権モデルにより防止）。
    - Integer overflow: 該当なし（インデックス未直接使用）。
  - インジェクション（SQL/Command/Path traversal）: 該当なし（I/Oや外部コマンド未使用）。
  - 認証・認可: 該当なし（スコープ管理のみ）。
  - 秘密情報: ハードコード秘密なし。ログ漏えいなし（ログ機能なし）。
  - 並行性
    - データ競合: `&mut self`メソッドでミューテーションを限定。共有には同期が必要。
    - Deadlock: 該当なし（ロック未使用）。

- Rust特有の観点
  - 所有権: すべての状態変更は`&mut self`に限定（enter_scope/exit_scope/set_current_*）（行番号: 不明）。
  - 借用/ライフタイム: `current_*`は`Option<&str>`返却で所有権移動なし。明示的ライフタイム不要。
  - unsafe境界: なし。
  - Send/Sync
    - フィールド（Vec<ScopeType>, Option<String>）はいずれも標準型で**Send+Sync**。型自体は自動的にSend/Syncを満たす。
    - ただし**同一インスタンスを複数スレッドで可変操作**する場合は`Mutex`などが必要。
  - 非同期/await: 該当なし。
  - エラー設計
    - `Result`は使用せず、状態は明示的APIで管理。
    - panic箇所なし（unwrap/expect未使用）。

## Design & Architecture Suggestions

- スコープガード（RAII）
  - enterの直後に自動でexitする**スコープガード**（例: `ScopeGuard<'a>`）を導入することで、例外経路や早期return時の**pop漏れ**を防止。
  ```rust
  struct ScopeGuard<'a> {
      ctx: &'a mut ParserContext,
  }
  impl<'a> Drop for ScopeGuard<'a> {
      fn drop(&mut self) { self.ctx.exit_scope(); }
  }
  // 使い方
  let _g = ctx.enter_with_guard(ScopeType::Function { hoisting: false }); // 新API
  ```
- 親情報の仕様明文化
  - 現在は**current_function優先→current_class**。クラス内関数で関数名未設定時に親情報Noneとなる。仕様として妥当なら明記、必要なら**クラス名へ確実にフォールバック**するオプションを追加。
- 参照型の安定化
  - `parent_name`の`into()`先の型（Symbol名ラッパー?）を型エイリアスで明示すると移行容易（このチャンクには定義不明）。
- 小さな最適化
  - 典型ネストが浅い前提ならそのままで十分。深いネストが常態なら`SmallVec<[ScopeType; 8]>`でアロケーション削減。
  - `is_module_level`を`scope_stack.last()`などで高速化（ただしModule以外の判定に注意）。
- ユーティリティの追加
  - `peek_scope()`や`current_scope_type()`の提供で状態確認を簡便化。
  - `reset()`で初期状態へ戻すAPI。

## Testing Strategy (Unit/Integration) with Examples

- 既存テスト（このチャンク内）
  - default_context（Module開始、判定の正しさ）
  - class_scope（ClassMember、退出時のクリア）
  - function_scope（Localの親情報、退出）
  - nested_scopes（クラス→関数→退出のロジック）
  - hoisted_function（hoisted=trueの伝播）

- 追加推奨ユニットテスト
  1. Blockスコープの親情報
     ```rust
     #[test]
     fn test_block_scope_parent_info() {
         let mut ctx = ParserContext::new();
         ctx.enter_scope(ScopeType::Function { hoisting: false });
         ctx.set_current_function(Some("f".to_string()));
         ctx.enter_scope(ScopeType::Block);
         assert_eq!(
             ctx.current_scope_context(),
             ScopeContext::Local {
                 hoisted: false,
                 parent_name: Some("f".to_string().into()),
                 parent_kind: Some(SymbolKind::Function),
             }
         );
     }
     ```
  2. Moduleを超えるexit（安全性）
     ```rust
     #[test]
     fn test_exit_does_not_pop_module() {
         let mut ctx = ParserContext::new();
         ctx.exit_scope(); // no-op
         assert_eq!(ctx.current_scope_context(), ScopeContext::Module);
     }
     ```
  3. Package/Namespace
     ```rust
     #[test]
     fn test_package_namespace_scope() {
         let mut ctx = ParserContext::new();
         ctx.enter_scope(ScopeType::Namespace);
         assert_eq!(ctx.current_scope_context(), ScopeContext::Package);
     }
     ```
  4. クラス名設定とクリア
     ```rust
     #[test]
     fn test_class_name_set_and_clear() {
         let mut ctx = ParserContext::new();
         ctx.enter_scope(ScopeType::Class);
         ctx.set_current_class(Some("C".to_string()));
         assert_eq!(ctx.current_class(), Some("C"));
         ctx.exit_scope();
         assert_eq!(ctx.current_class(), None);
     }
     ```
  5. 連続ネストの健全性（push/pop整合）
     ```rust
     #[test]
     fn test_deep_nesting_consistency() {
         let mut ctx = ParserContext::new();
         for _ in 0..16 { ctx.enter_scope(ScopeType::Block); }
         for _ in 0..16 { ctx.exit_scope(); }
         assert_eq!(ctx.current_scope_context(), ScopeContext::Module);
     }
     ```

- 統合テスト（推奨）
  - 実際の言語パーサーでAST走査しながら本コンテキストを利用し、生成される**ScopeContext**が期待どおりか検証（このチャンクには対象コード不明）。

## Refactoring Plan & Best Practices

- ステップ
  1. `enter_with_guard`の導入と徐々な置き換え（スコープ漏れ防止）。
  2. `peek_scope/current_scope_type`の追加で判定簡略化。
  3. 必要に応じ`SmallVec`導入（featureで切替可能に）。
  4. `parent_name`型の型エイリアス導入（例: `type SymbolName = ...`）（このチャンクには現れない）。
  5. ドキュメントコメントに親決定の優先順位を明記。
- ベストプラクティス
  - **enter→set_name→処理→exit**の順序を徹底。
  - テストで**早期return**や**エラー経路**でもexitされることを確認（RAII推奨）。
  - `current_*`は参照返しでコピー回避済み。外部へ所有権を渡す必要がある場合のみ明示的クローン。

## Observability (Logging, Metrics, Tracing)

- ログ
  - `trace!`レベルで`enter_scope/exit_scope`、`current_scope_context`の返却値を記録できるようfeatureフラグで有効化。
- メトリクス
  - 最大スコープ深さ、`current_scope_context`呼び出し回数/時間計測。
- トレーシング
  - `tracing`クレートでスコープ入出のspanを付与し、AST走査と紐づけ。

このチャンクにはログ/メトリクス/トレーシングの実装は現れない。

## Risks & Unknowns

- Unknowns
  - `ScopeContext`と`parent_name`の具体型/表現はこのチャンクには現れない。
  - 本コンテキストの使用箇所（どのパーサー、どのresolver）も不明。
- リスク
  - **名前設定の漏れ**により、親情報がNoneとなり解決精度が下がる可能性。
  - **enter/exitの整合性破綻**で誤ったスコープ解釈。RAII導入で緩和可。
  - 将来のスコープ種別追加時に**current_scope_contextの分岐更新漏れ**（回帰の恐れ）。単体テストの拡充で軽減。