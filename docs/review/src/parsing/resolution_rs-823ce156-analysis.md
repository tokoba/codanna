# resolution.rs Review

## TL;DR

- 目的: 言語非依存のシンボル解決・継承解決を抽象化するための**トレイト群**と、デフォルト実装の**GenericResolutionContext**・**GenericInheritanceResolver**を提供。
- 主要公開API: ResolutionScope, ProjectResolutionEnhancer, InheritanceResolver とそのデフォルト実装。ScopeLevel/ImportOrigin/ImportBinding のデータ契約も含む。
- 複雑箇所: ResolutionScope::is_compatible_relationship の関係適合性判定（多数の分岐）、GenericInheritanceResolver::get_inheritance_chain（循環対策あり）と resolve_method（循環対策なし）。
- 重大リスク:
  - exit_scope のローカルクリア条件が誤り（ポップ後の先頭が関数ならクリア）で、本来は「退出対象が関数」であるべき。
  - 継承循環がある場合、GenericInheritanceResolver::resolve_method が無限再帰でスタックオーバーフロー。
  - is_external_import の既定判定が Internal かつ resolved_symbol=None を外部扱いにしてしまい、誤判定の可能性。
- Rust安全性: unsafeなし。Send + Sync 制約で並行使用の契約は明確だが、内部状態は HashMap でミュータブルなため、&mut self 経由の単一スレッド利用前提。
- エラー設計: Option中心で例外や panic はなし（テストで unwrap は使用）。未解決や未知は None として扱う。

## Overview & Purpose

このファイルは、プロジェクト全体で言語に依存しない形で「シンボル解決」と「継承解決」を行うための抽象トレイトと、その**ジェネリックな既定実装**を提供します。

- ResolutionScope: スコープにおけるシンボルの登録・検索、インポート起源の判断、関係の適合性判定など、言語固有の解決規則に対応可能な拡張ポイント。
- ProjectResolutionEnhancer: tsconfig.json などのプロジェクト設定を踏まえたインポートパスの変換を可能にする拡張ポイント。
- InheritanceResolver: 型の継承関係からメソッド解決やサブタイプ判定を行う抽象化。
- GenericResolutionContext/GenericInheritanceResolver: 上記トレイトの**言語非依存・簡易**な参照実装。

目的は、Rust/TypeScript/Python など各言語の固有ルールをこの抽象化に基づいて差し替え可能にすることで、**インデクサを言語非依存に保つ**ことです。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | ScopeLevel | pub | スコープ階層（Local/Module/Package/Global） | Low |
| Enum | ImportOrigin | pub | インポート起源（Internal/External/Unknown） | Low |
| Struct | ImportBinding | pub | インポートの公開名・起源・解決結果 | Low |
| Trait | ResolutionScope | pub | シンボル登録・解決、インポート処理、関係適合性など | High |
| Trait | ProjectResolutionEnhancer | pub | インポートパスのプロジェクト設定に基づく変換 | Med |
| Trait | InheritanceResolver | pub | 継承関係の管理・メソッド解決・サブタイプ判定 | Med |
| Struct | GenericResolutionContext | pub | 既定の解決コンテキスト（スコープごとに HashMap） | Med |
| Struct | GenericInheritanceResolver | pub | 既定の継承解決（子→親と型→メソッドの Map） | Med |

### Dependencies & Interactions

- 内部依存
  - GenericResolutionContext は ResolutionScope を実装。
  - GenericInheritanceResolver は InheritanceResolver を実装。
  - ResolutionScope::is_compatible_relationship は crate::RelationKind と crate::SymbolKind を参照して多数分岐。
  - is_external_import は import_binding（デフォルト未実装）と ImportBinding を用いた判定。
- 外部依存（標準・自前）
  - std::collections::HashMap
  - super::context::ScopeType（スコープスタック管理に使用）
  - crate::{FileId, SymbolId, parsing::Import}
  - crate::{SymbolKind, RelationKind}（関係適合性判定）
- 被依存推定
  - 言語ごとのインデクサ実装（Rust/Python/TS/PHP/Go等）が ResolutionScope/IneritanceResolver を実装またはラップ。
  - インポート解析フェーズ（populate_imports、register_import_binding）。
  - 関係エッジ生成（Calls/Defines/Implements/Extends 等）時の適合性チェック。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ScopeLevel | enum ScopeLevel { Local, Module, Package, Global } | スコープ階層の表現 | N/A | O(1) |
| ImportOrigin | enum ImportOrigin { Internal, External, Unknown } | インポート起源の分類 | N/A | O(1) |
| ImportBinding | struct ImportBinding { import: Import, exposed_name: String, origin: ImportOrigin, resolved_symbol: Option<SymbolId> } | インポートのバインディング情報 | N/A | O(1) |
| ResolutionScope::add_symbol | fn add_symbol(&mut self, name: String, symbol_id: SymbolId, scope_level: ScopeLevel) | シンボルをスコープへ登録 | O(1) 平均 | O(1) |
| ResolutionScope::resolve | fn resolve(&self, name: &str) -> Option<SymbolId> | 名前からシンボル ID を解決 | O(1) 平均 | O(1) |
| ResolutionScope::clear_local_scope | fn clear_local_scope(&mut self) | ローカルスコープのクリア | O(n_local) | O(1) |
| ResolutionScope::enter_scope | fn enter_scope(&mut self, scope_type: ScopeType) | スコープスタックへ入る | O(1) | O(1) |
| ResolutionScope::exit_scope | fn exit_scope(&mut self) | スコープスタックから出る | O(1) | O(1) |
| ResolutionScope::symbols_in_scope | fn symbols_in_scope(&self) -> Vec<(String, SymbolId, ScopeLevel)> | 全スコープのシンボル列挙 | O(N) | O(N) |
| ResolutionScope::as_any_mut | fn as_any_mut(&mut self) -> &mut dyn std::any::Any | ダウンキャスト支援 | O(1) | O(1) |
| ResolutionScope::resolve_relationship | fn resolve_relationship(&self, from_name: &str, to_name: &str, kind: crate::RelationKind, from_file: FileId) -> Option<SymbolId> | 関係種別に応じた解決（既定は resolve 委譲） | O(1) 平均 | O(1) |
| ResolutionScope::populate_imports | fn populate_imports(&mut self, imports: &[crate::parsing::Import]) | インポート記録の投入（既定は無処理） | O(n) | O(n) |
| ResolutionScope::register_import_binding | fn register_import_binding(&mut self, binding: ImportBinding) | 処理済みインポートの登録 | O(1) 平均 | O(1) |
| ResolutionScope::import_binding | fn import_binding(&self, name: &str) -> Option<ImportBinding> | インポートバインディングの取得 | O(1) 平均 | O(1) |
| ResolutionScope::resolve_expression_type | fn resolve_expression_type(&self, expr: &str) -> Option<String> | 式の型解決（既定は None） | O(1) | O(1) |
| ResolutionScope::is_external_import | fn is_external_import(&self, name: &str) -> bool | 外部インポート由来かの判定 | O(1) 平均 | O(1) |
| ResolutionScope::is_compatible_relationship | fn is_compatible_relationship(&self, from_kind: crate::SymbolKind, to_kind: crate::SymbolKind, rel_kind: crate::RelationKind) -> bool | 関係の適合性判定 | O(1) | O(1) |
| ProjectResolutionEnhancer::enhance_import_path | fn enhance_import_path(&self, import_path: &str, from_file: FileId) -> Option<String> | プロジェクト設定によるインポートパス変換 | O(1)〜O(k) | O(1) |
| ProjectResolutionEnhancer::get_import_candidates | fn get_import_candidates(&self, import_path: &str, from_file: FileId) -> Vec<String> | インポート候補パスの列挙（既定は1件） | O(k) | O(k) |
| InheritanceResolver::add_inheritance | fn add_inheritance(&mut self, child: String, parent: String, kind: &str) | 継承関係の追加 | O(1) 平均 | O(1) |
| InheritanceResolver::resolve_method | fn resolve_method(&self, type_name: &str, method: &str) -> Option<String> | メソッドの提供元型を解決 | O(h·m) 最悪 | O(1) |
| InheritanceResolver::get_inheritance_chain | fn get_inheritance_chain(&self, type_name: &str) -> Vec<String> | 継承チェーンの列挙（循環検出あり） | O(V+E) | O(V) |
| InheritanceResolver::is_subtype | fn is_subtype(&self, child: &str, parent: &str) -> bool | サブタイプ判定 | O(V+E) | O(V) |
| InheritanceResolver::add_type_methods | fn add_type_methods(&mut self, type_name: String, methods: Vec<String>) | 型のメソッド登録 | O(m) | O(m) |
| InheritanceResolver::get_all_methods | fn get_all_methods(&self, type_name: &str) -> Vec<String> | 継承含めたメソッド集合 | O(Σm) | O(Σm) |
| GenericResolutionContext::new | pub fn new(file_id: FileId) -> Self | 既定の解決コンテキスト生成 | O(1) | O(1) |
| GenericResolutionContext::from_existing | pub fn from_existing(file_id: FileId) -> Self | 既存コンテキストラップ（現在は new と同義） | O(1) | O(1) |
| GenericInheritanceResolver::new | pub fn new() -> Self | 既定の継承解決生成 | O(1) | O(1) |

以下、主なAPIの詳細。

### ResolutionScope::resolve

1. 目的と責務
   - 名前からシンボルIDを解決。解決順は**Local→Module→Package→Global**。

2. アルゴリズム（ステップ）
   - レベル配列 [Local, Module, Package, Global] を順に走査。
   - 各レベルの HashMap から name を検索。
   - 見つかれば Some(SymbolId)、全レベルで見つからなければ None。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| name | &str | 解決するシンボル名 |

4. 戻り値

| 型 | 説明 |
|----|------|
| Option<SymbolId> | 解決成功なら Some、未解決なら None |

5. 使用例
```rust
let mut ctx = GenericResolutionContext::new(FileId::new(1).unwrap());
ctx.add_symbol("x".into(), SymbolId::new(42).unwrap(), ScopeLevel::Local);
assert_eq!(ctx.resolve("x"), Some(SymbolId::new(42).unwrap()));
assert_eq!(ctx.resolve("y"), None);
```

6. エッジケース
- 同名シンボルが複数レベルに存在
  - Local が最優先で選ばれる。
- 空文字列
  - None（未解決）を返す想定。

### ResolutionScope::is_compatible_relationship

1. 目的と責務
   - 言語一般に妥当と思われる関係種別（Calls/Defines/Implements/Extends 等）の**適合性チェック**。

2. アルゴリズム（ステップ）
   - rel_kind で分岐し、from_kind/to_kind の許可セットに対して matches! 判定。
   - Calls/CalledBy, Implements/ImplementedBy, Uses/UsedBy, Defines/DefinedIn, Extends/ExtendedBy, References/ReferencedBy を網羅。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| from_kind | crate::SymbolKind | 元シンボルの種別 |
| to_kind | crate::SymbolKind | 先シンボルの種別 |
| rel_kind | crate::RelationKind | 関係種別 |

4. 戻り値

| 型 | 説明 |
|----|------|
| bool | 適合すれば true |

5. 使用例
```rust
let ctx = GenericResolutionContext::new(FileId::new(1).unwrap());
assert!(ctx.is_compatible_relationship(
    crate::SymbolKind::Function,
    crate::SymbolKind::Function,
    crate::RelationKind::Calls
));
assert!(!ctx.is_compatible_relationship(
    crate::SymbolKind::Function,
    crate::SymbolKind::Constant,
    crate::RelationKind::Calls
));
```

6. エッジケース
- React/TS などで定数が呼び出し可能（ファクトリ/コンポーネント）などは既定では非対応。言語側でオーバーライドが必要。

### ResolutionScope::is_external_import

1. 目的と責務
   - 名前が**外部依存**に由来し、ローカル解決を行うべきでないかを判定。

2. アルゴリズム（既定）
   - import_binding(name) 取得。
   - origin が External → true。
   - origin が Internal/Unknown → resolved_symbol が None なら true、Some なら false。
   - import_binding が None → false。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| name | &str | 判定する公開名 |

4. 戻り値

| 型 | 説明 |
|----|------|
| bool | 外部由来なら true |

5. 使用例
```rust
let mut ctx = GenericResolutionContext::new(FileId::new(1).unwrap());
ctx.register_import_binding(ImportBinding {
    import: crate::parsing::Import { /* フィールドはこのチャンクに現れない */ },
    exposed_name: "ProgressBar".into(),
    origin: ImportOrigin::External,
    resolved_symbol: None,
});
assert!(ctx.is_external_import("ProgressBar"));
```

6. エッジケース
- Internal だが未解決（resolved_symbol=None）→ true となるため、誤判定の可能性。言語実装で要オーバーライド。

### InheritanceResolver::resolve_method

1. 目的と責務
   - 型のメソッドがどの祖先型で定義されているかを解決。

2. アルゴリズム（既定）
   - 自型の登録メソッドに含まれていれば自型名を返す。
   - そうでなければ親型群を走査し、親へ再帰的に委譲。
   - 見つからなければ None。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| type_name | &str | 型名 |
| method | &str | メソッド名 |

4. 戻り値

| 型 | 説明 |
|----|------|
| Option<String> | 提供元の型名（Some）または未解決（None） |

5. 使用例
```rust
let mut inh = GenericInheritanceResolver::new();
inh.add_inheritance("Child".into(), "Parent".into(), "extends");
inh.add_type_methods("Parent".into(), vec!["m".into()]);
assert_eq!(inh.resolve_method("Child", "m"), Some("Parent".into()));
```

6. エッジケース
- 継承に循環がある場合、無限再帰の危険あり（このチャンクの既定実装は訪問済みチェックなし）。

### GenericResolutionContext::{new, from_existing}

1. 目的と責務
   - 既定の汎用解決コンテキストを初期化（4スコープの HashMap、および Global を初期スコープ）。

2. 引数/戻り値

| 関数 | 引数 | 戻り値 |
|------|------|--------|
| new | file_id: FileId | Self |
| from_existing | file_id: FileId | Self（現時点では new と同義） |

3. 使用例
```rust
let ctx = GenericResolutionContext::new(FileId::new(1).unwrap());
let ctx2 = GenericResolutionContext::from_existing(FileId::new(2).unwrap());
```

### GenericInheritanceResolver::new

- 単純な初期化（空の継承・メソッドマップ）。

```rust
let resolver = GenericInheritanceResolver::new();
```

## Walkthrough & Data Flow

- シンボル解決フロー（GenericResolutionContext）
  - add_symbol で ScopeLevel 単位の HashMap に登録。
  - resolve は Local→Module→Package→Global の順に検索。
  - enter_scope/exit_scope は ScopeType をスタック管理し、exit_scope 時にローカルクリア判定（ただし現実装は誤判定の可能性、後述）。
  - import_binding は register_import_binding で登録した Map から取得。
  - is_external_import は import_binding の origin と resolved_symbol に基づき判定。

- 継承解決フロー（GenericInheritanceResolver）
  - add_inheritance で child→[(parent, kind)] を蓄積。
  - add_type_methods で型→メソッド集合を登録。
  - resolve_method は自己に無ければ親へ再帰。
  - get_inheritance_chain は BFS 的に親を辿り visited で循環検出。
  - is_subtype は chain に parent が含まれるかで判定。
  - get_all_methods は chain 上の全祖先からメソッドを重複除去し集約。

### Mermaid: is_compatible_relationship の主要分岐

```mermaid
flowchart TD
    A[Start: rel_kind] --> B{Calls?}
    B -- Yes --> C[caller: Function|Method|Macro|Module]
    C --> D[callee: Function|Method|Macro|Class]
    B -- No --> E{CalledBy?}
    E -- Yes --> F[callee: Function|Method|Macro|Class]
    F --> G[caller: Function|Method|Macro|Module]
    E -- No --> H{Implements?}
    H -- Yes --> I[from: Struct|Enum|Class; to: Trait|Interface]
    H -- No --> J{ImplementedBy?}
    J -- Yes --> K[from: Trait|Interface; to: Struct|Enum|Class]
    J -- No --> L{Uses?}
    L -- Yes --> M[from: Func/Method/Struct/Class/Trait/Interface/Module/Enum]
    M --> N[to: Struct/Enum/Class/Trait/Interface/TypeAlias/Constant/Variable/Function/Method]
    L -- No --> O{UsedBy?}
    O -- Yes --> P[from: Struct/Enum/Class/Trait/Interface/TypeAlias/Constant/Variable/Function/Method]
    P --> Q[to: Func/Method/Struct/Class/Trait/Interface/Module/Enum]
    O -- No --> R{Defines?}
    R -- Yes --> S[from: Trait/Interface/Module/Struct/Enum/Class]
    S --> T[to: Method/Function/Constant/Field/Variable]
    R -- No --> U{DefinedIn?}
    U -- Yes --> V[from: Method/Function/Constant/Field/Variable]
    V --> W[to: Trait/Interface/Module/Struct/Enum/Class]
    U -- No --> X{Extends?}
    X -- Yes --> Y[from: Class/Interface/Trait/Struct/Enum]
    Y --> Z[to: Class/Interface/Trait/Struct/Enum]
    X -- No --> AA{ExtendedBy?}
    AA -- Yes --> AB[from/to: Class/Interface/Trait/Struct/Enum]
    AA -- No --> AC{References?}
    AC -- Yes --> AD[true]
    AC -- No --> AE{ReferencedBy?}
    AE -- Yes --> AF[true]
    AE -- No --> AG[false]
```

上記の図は is_compatible_relationship 関数（行番号不明）の主要分岐を示す。

## Complexity & Performance

- add_symbol: HashMap への挿入で平均 O(1)。
- resolve: 4レベルの HashMap から平均 O(1) で照会。最悪でも定数回の照会。
- symbols_in_scope: 全スコープ合計 N の反復で O(N)、文字列の clone が発生。
- is_external_import: Map 検索 O(1) 平均。
- 継承系:
  - resolve_method: 深さ h、親あたり m の探索で O(h·m) 最悪。循環検出なしのため循環があると停止しない。
  - get_inheritance_chain: BFS で O(V+E)（訪問済みセットあり）。
  - is_subtype: chain の生成 + 包含判定で O(V+E)。
  - get_all_methods: 祖先全体のメソッドを重複確認しながら収集するため O(Σm)（HashSet を使わず Vec.contains による O(n^2) 的重複チェックの可能性）。

実運用負荷要因:
- 大規模プロジェクトでは symbols_in_scope の大量 clone とベクタ返却が負荷。
- 継承チェーン・メソッド集約は線形〜二次的重複チェックのため、型数・メソッド数が多いほど負荷増。
- インポート判定は Map 参照中心で軽量。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| exit_scope のローカルクリア条件 | スタック: [Global, Function, Block] で Block を exit | 退出したのが Function のときのみ Local をクリア | `exit_scope` で pop 後の last を関数か判定 | バグ（誤判定） |
| 継承循環でのメソッド解決 | A extends B, B extends A, resolve_method(A, "m") | 無限ループ回避（None または検出） | resolve_method に visited がない | バグ（スタックオーバーフローの危険） |
| Internal 未解決インポートの扱い | ImportOrigin::Internal, resolved_symbol=None | 「外部扱い」ではなく「未解決」扱い | is_external_import で true を返す | 設計上の注意（誤判定の可能性） |
| 重複メソッドの収集 | 祖先複数に同名メソッド | 1回のみ返却 | Vec.contains による重複排除 | 設計妥当だが O(n^2) 傾向 |
| symbols_in_scope の負荷 | 大量のシンボル | メモリ負荷・clone を抑える | 文字列 clone で収集 | 最適化余地あり |
| 空文字列の解決 | resolve("") | None | 直列検索 | 想定通り（問題なし） |

セキュリティチェックリスト:
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（安全な Rust、unsafe 不使用）。
  - 所有権/借用: &mut self と &self の明確な使い分けで安全。
- インジェクション（SQL/Command/Path）
  - 該当なし（文字列分類・Map 操作のみ）。
- 認証・認可
  - 該当なし（論理レイヤ）。
- 秘密情報
  - Hard-coded secrets / Log leakage: 該当なし。
- 並行性
  - Race condition / Deadlock: Send + Sync のトレイト制約あり。可変操作は &mut self（単一スレッド/排他前提）。内部に同期原語はなし。

Rust特有の観点（詳細チェックリスト）:
- 所有権: 文字列やベクタは所有権移動（例: add_symbol(name: String)）。Map に move される。
- 借用/ライフタイム: 返却は Option<SymbolId> とコピー（値型想定）。明示的ライフタイムなし。
- unsafe境界: なし。
- Send/Sync: トレイトに Send + Sync 制約。GenericResolutionContext/GenericInheritanceResolver はフィールドが Send + Sync（HashMap<K,V> は K,V が Send+Sync なら自動導出）である限り満たす。
- 非同期/await: 該当なし。
- エラー設計: Result より Option 中心。未解決/未知は None。panic はテスト内の unwrap のみ。

## Design & Architecture Suggestions

- exit_scope のロジック修正
  - 退出したスコープ（pop の戻り値）が Function なら clear_local_scope を呼ぶようにする。
  - 例: `if matches!(self.scope_stack.pop(), Some(ScopeType::Function { .. })) { self.clear_local_scope(); }`
- resolve_method の循環対策
  - visited セットを導入して無限再帰を防止。
  - もしくは get_inheritance_chain を活用して安全な探索に置換。
- is_external_import の判定ポリシー明確化
  - Internal かつ unresolved は「未解決」扱いにし、原則 false として言語実装側で上書きする方針に変更。
  - あるいは ImportOrigin::Unknown のみに unresolved→true を適用。
- ScopeType と ScopeLevel の整合
  - enter_scope/exit_scope の ScopeType と add_symbol の ScopeLevel の対応関係を明示し、現在スコープレベルに自動割当可能にする（例: enter_scope(Function) なら add_symbol の既定を Local に）。
- get_all_methods の重複排除に HashSet を使用
  - Vec.contains を HashSet に変更して O(Σm) を O(Σm)（重複判定 O(1)）に改善。
- symbols_in_scope の戻り値をイテレータにするか、デバッグ専用 API として明示
  - 大量プロジェクトでの clone 負荷を低減。

## Testing Strategy (Unit/Integration) with Examples

既存テストは関係適合性/解決順序/継承解決の基本をカバー。以下の追加を推奨。

- exit_scope のローカルクリア検証（バグ回避テスト）
```rust
#[test]
fn test_exit_scope_clears_local_only_on_function_exit() {
    let mut ctx = GenericResolutionContext::new(FileId::new(1).unwrap());
    ctx.add_symbol("x".into(), SymbolId::new(1).unwrap(), ScopeLevel::Local);
    ctx.enter_scope(ScopeType::Function { /* フィールドはこのチャンクには現れない */ });
    ctx.enter_scope(ScopeType::Block);
    // exit block: 本来 local をクリアすべきでない
    ctx.exit_scope();
    assert_eq!(ctx.resolve("x"), Some(SymbolId::new(1).unwrap()));

    // exit function: ここで local をクリアすべき
    ctx.exit_scope();
    assert_eq!(ctx.resolve("x"), None);
}
```

- 継承循環の検出/防止
```rust
#[test]
fn test_resolve_method_handles_cycles() {
    let mut inh = GenericInheritanceResolver::new();
    inh.add_inheritance("A".into(), "B".into(), "extends");
    inh.add_inheritance("B".into(), "A".into(), "extends");
    inh.add_type_methods("B".into(), vec!["m".into()]);
    // 期待: 無限ループしない（現実装は危険）
    assert_eq!(inh.resolve_method("A", "m"), Some("B".into()));
}
```

- is_external_import のポリシー検証
```rust
#[test]
fn test_is_external_import_internal_unresolved_policy() {
    let mut ctx = GenericResolutionContext::new(FileId::new(1).unwrap());
    ctx.register_import_binding(ImportBinding {
        import: crate::parsing::Import { /* 不明 */ },
        exposed_name: "Foo".into(),
        origin: ImportOrigin::Internal,
        resolved_symbol: None,
    });
    // 現実装では true だが、言語固有にオーバーライドして false を期待する場合の例
    assert!(ctx.is_external_import("Foo"));
}
```

- get_all_methods の重複排除効率
```rust
#[test]
fn test_get_all_methods_dedup() {
    let mut inh = GenericInheritanceResolver::new();
    inh.add_inheritance("C".into(), "B".into(), "extends");
    inh.add_inheritance("B".into(), "A".into(), "extends");
    inh.add_type_methods("A".into(), vec!["m".into()]);
    inh.add_type_methods("B".into(), vec!["m".into(), "n".into()]);
    let mut all = inh.get_all_methods("C");
    all.sort();
    assert_eq!(all, vec!["m".to_string(), "n".to_string()]);
}
```

## Refactoring Plan & Best Practices

- 修正ポイント
  - exit_scope: pop の戻り値で関数退出を判定。
  - resolve_method: visited（HashSet<String>）導入。
  - get_all_methods: HashSet<String> で重複排除。
  - is_external_import: Internal unresolved の扱い方針を明文化、既定をより保守的に。

- 構造改善
  - ScopeType⇄ScopeLevel のマッピング関数を設け、enter_scope で現在レベルを更新。
  - symbols_in_scope はデバッグビルドのみで活用するか、ページング/フィルタを追加。

- ベストプラクティス
  - トレイトの既定実装は「最小限安全・保守的」にする（誤判定より未解決を優先）。
  - 循環構造への防御（継承・依存）を一貫して導入。

## Observability (Logging, Metrics, Tracing)

- ログ
  - 重要イベント（シンボル未解決、外部インポート判定、継承循環検出）を debug/info レベルで記録。
- メトリクス
  - 解決成功率、未解決数、外部判定比率。
  - 継承チェーン長の分布、メソッド解決の平均深さ。
- トレーシング
  - resolve 呼び出しチェーンに span を追加（from_name/to_name/rel_kind をタグとして付与）してボトルネック可視化。

このチャンクには具体的なロギング実装は現れない。

## Risks & Unknowns

- ScopeType の詳細（構造体フィールド）はこのチャンクには現れないため、enter_scope/exit_scope の完全な期待動作は不明。
- crate::parsing::Import のフィールド構造は不明のため、populate_imports の期待ロジックは言語側依存。
- FileId/SymbolId の new/unwrap の安全性はテストで使用されているが、構造詳細は不明。
- is_compatible_relationship の既定ポリシーは多言語で完全に妥当とは限らないため、言語固有のオーバーライドが前提。

以上により、公開APIとコアロジックは明確で拡張容易だが、2つのバグ（exit_scope 条件、resolve_method 循環）といくつかの設計上の注意点（外部インポート判定、重複排除効率）が実運用上の品質に影響し得るため、早期の修正を推奨します。