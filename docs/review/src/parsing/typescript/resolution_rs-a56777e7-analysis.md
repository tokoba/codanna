# resolution.rs Review

## TL;DR

- 目的: **TypeScriptの解決（名前解決）と継承**をRustで表現。TS特有の**ホイスティング**、**ブロックスコープ**、**型空間/値空間の分離**、**名前空間/パスエイリアス**を扱う。
- 主要公開API:
  - Context: **TypeScriptResolutionContext::resolve**（名前解決）、**add_symbol_with_context**（スコープに応じた登録）
  - Inheritance: **TypeScriptInheritanceResolver::add_inheritance / is_subtype / get_all_methods**
  - Project: **TypeScriptProjectEnhancer::enhance_import_path / get_import_candidates**
- 複雑箇所: **resolveの分岐**（7段階＋`.`の特別扱い）、**継承ツリー探索**（クラスとインタフェース混在）、**エイリアス→モジュールパス変換**。
- 重大リスク:
  - 「qualified_names」が**未使用**（登録のみで解決に使われない）。
  - **ホイスティングの誤実装/誤テスト**（テストは`hoisted_scope`を期待するがコードは`local_scope`に入れる）。
  - **名前空間エイリアスの解決が不正確**（メンバー名をスコープ横断探索してしまう）。
  - **AST情報なしのヒューリスティック**に依存（インタフェース判定など）。
- パフォーマンス: HashMap中心で**平均O(1)**、継承探索は**O(V+E)**。解決時の`.`分岐で**余分な探索**があり得る。
- セキュリティ/安全性: **unsafeなし**、メモリ安全。並行性は**Sync/Send未保証**、共有利用時は要同期。ログは`debug_print!`のみ。
- 改善案: AST統合、**qualified_namesの活用**、**モジュールパス正規化**、`namespace_aliases`の厳密実装、テスト修正。

## Overview & Purpose

このファイルは、TypeScript固有の名前解決と継承をRustの抽象に落とし込むモジュールです。

- TypeScriptResolutionContext: TSの**スコープルール**（ホイスティング・ブロックスコープ）、**型空間と値空間**の分離、**名前空間/インポート解決**を扱う。
- TypeScriptInheritanceResolver: TSの**クラス継承（extends）**、**インタフェース実装（implements）**、**インタフェース継承（複数extends）**を解決、**メソッド探索**や**サブタイプ判定**を提供。
- TypeScriptProjectEnhancer: tsconfig.jsonの**paths/baseUrl**を解釈し、**インポートパスのエイリアス解決**を行う。

現状は**アーキテクチャ基盤**としての実装で、AST統合は未了。TypeScriptの詳細な言語仕様準拠は部分的で、**ヒューリスティック**に頼っています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | TypeScriptResolutionContext | pub | 名前解決（ローカル/ホイスト/モジュール/グローバル/型空間、インポート、名前空間、関係解決） | High |
| Struct | TypeScriptInheritanceResolver | pub | 継承関係（extends/implements）とメソッド/サブタイプ解決 | Med |
| Struct | TypeScriptProjectEnhancer | pub | tsconfigパスエイリアス解決 | Low |
| Trait impl | ResolutionScope for TypeScriptResolutionContext | pub | 名前追加/解決/スコープ出入り/関係解決/互換性判定 | High |
| Trait impl | InheritanceResolver for TypeScriptInheritanceResolver | pub | 継承追加/メソッド解決/継承鎖/サブタイプ判定/メソッド集約 | Med |
| Trait impl | ProjectResolutionEnhancer for TypeScriptProjectEnhancer | pub | インポートパス強化/候補列挙 | Low |

フィールド（TypeScriptResolutionContext）:
- local_scope, hoisted_scope, module_symbols, imported_symbols, global_symbols, type_space: 各スコープのシンボル表
- scope_stack: スコープスタック
- imports, namespace_aliases, qualified_names, import_bindings: インポートと名前空間の補助情報

### Dependencies & Interactions

- 内部依存:
  - TypeScriptResolutionContext → ResolutionScope（trait実装）
  - TypeScriptInheritanceResolver → InheritanceResolver（trait実装）
  - TypeScriptProjectEnhancer → ProjectResolutionEnhancer（trait実装）
  - TypeScriptProjectEnhancer → crate::parsing::typescript::tsconfig::{TsConfig, CompilerOptions, PathAliasResolver}
  - Contextで`debug_print!`マクロ使用

- 外部依存（クレート・モジュール・型）
  
  | モジュール/型 | 用途 |
  |---------------|------|
  | crate::parsing::resolution::{ImportBinding, ProjectResolutionEnhancer} | インポートバインディング管理、プロジェクト解決強化 |
  | crate::parsing::{InheritanceResolver, ResolutionScope, ScopeLevel, ScopeType} | トレイトとスコープ型 |
  | crate::project_resolver::persist::ResolutionRules | tsconfig相当のルール入力 |
  | crate::{FileId, SymbolId} | 識別子 |
  | crate::parsing::typescript::tsconfig::* | パスエイリアス解決器 |

- 被依存推定:
  - コードベースの**名前解決フェーズ**、**関係グラフ構築**、**インポート正規化**で使用される可能性が高い。
  - フロントエンド（UI）や解析パイプラインでの**記号リンク**、**ジャンプ（Go to Definition）**など。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| TypeScriptResolutionContext::new | fn new(file_id: FileId) -> Self | 文脈生成 | O(1) | O(1) |
| TypeScriptResolutionContext::add_import | fn add_import(path: String, alias: Option<String>) | インポート登録 | O(1) | O(1) |
| TypeScriptResolutionContext::add_namespace_alias | fn add_namespace_alias(alias: String, target_module: String) | 名前空間エイリアス登録 | O(1) | O(1) |
| TypeScriptResolutionContext::add_qualified_name | fn add_qualified_name(qualified: String, symbol_id: SymbolId) | 事前計算済み修飾名登録 | O(1) | O(1) |
| TypeScriptResolutionContext::add_import_symbol | fn add_import_symbol(name: String, id: SymbolId, is_type_only: bool) | 型空間/値空間へのインポート登録 | O(1) | O(1) |
| TypeScriptResolutionContext::add_symbol_with_context | fn add_symbol_with_context(name: String, id: SymbolId, scope_context: Option<&crate::symbol::ScopeContext>) | スコープ文脈に基づく登録 | O(1) | O(1) |
| ResolutionScope::add_symbol | fn add_symbol(name: String, id: SymbolId, level: ScopeLevel) | スコープレベルに基づく登録 | O(1) | O(1) |
| ResolutionScope::resolve | fn resolve(name: &str) -> Option<SymbolId> | 名前解決（多段階＋`.`対応） | 平均O(1)、`.`時O(k) | O(1) |
| ResolutionScope::clear_local_scope | fn clear_local_scope(&mut self) | ローカルスコープ消去 | O(n) | O(1) |
| ResolutionScope::enter_scope | fn enter_scope(scope_type: ScopeType) | スコープ入場 | O(1) | O(1) |
| ResolutionScope::exit_scope | fn exit_scope(&mut self) | スコープ退出（条件で消去） | O(n) | O(1) |
| ResolutionScope::symbols_in_scope | fn symbols_in_scope(&self) -> Vec<(String, SymbolId, ScopeLevel)> | 可視シンボル列挙 | O(n) | O(n) |
| ResolutionScope::resolve_relationship | fn resolve_relationship(&self, _from_name: &str, to_name: &str, kind: crate::RelationKind, _from_file: FileId) -> Option<SymbolId> | 関係種別に応じた解決 | O(1)〜O(k) | O(1) |
| ResolutionScope::is_compatible_relationship | fn is_compatible_relationship(&self, from_kind: crate::SymbolKind, to_kind: crate::SymbolKind, rel_kind: crate::RelationKind) -> bool | 関係互換性判定 | O(1) | O(1) |
| ResolutionScope::populate_imports | fn populate_imports(&mut self, imports: &[crate::parsing::Import]) | インポート一括登録 | O(n) | O(n) |
| ResolutionScope::register_import_binding | fn register_import_binding(&mut self, binding: ImportBinding) | バインディング登録 | O(1) | O(1) |
| ResolutionScope::import_binding | fn import_binding(&self, name: &str) -> Option<ImportBinding> | バインディング取得 | O(1) | O(1) |
| TypeScriptInheritanceResolver::new | fn new() -> Self | 継承解決器生成 | O(1) | O(1) |
| InheritanceResolver::add_inheritance | fn add_inheritance(child: String, parent: String, kind: &str) | 継承/実装追加 | O(1) | O(1) |
| InheritanceResolver::resolve_method | fn resolve_method(type_name: &str, method_name: &str) -> Option<String> | メソッドの実体探索 | O(h·m) | O(1) |
| InheritanceResolver::get_inheritance_chain | fn get_inheritance_chain(type_name: &str) -> Vec<String> | 継承鎖取得 | O(V+E) | O(V) |
| InheritanceResolver::is_subtype | fn is_subtype(child: &str, parent: &str) -> bool | サブタイプ判定 | O(V+E) | O(1) |
| InheritanceResolver::add_type_methods | fn add_type_methods(type_name: String, methods: Vec<String>) | メソッド追加 | O(m) | O(m) |
| InheritanceResolver::get_all_methods | fn get_all_methods(type_name: &str) -> Vec<String> | 継承/実装含むメソッド集約 | O(V+E+M) | O(M) |
| TypeScriptInheritanceResolver::add_class_extends | fn add_class_extends(child: String, parent: String) | クラスextends登録 | O(1) | O(1) |
| TypeScriptInheritanceResolver::add_class_implements | fn add_class_implements(class_name: String, interface_name: String) | implements登録 | O(1) | O(1) |
| TypeScriptInheritanceResolver::add_interface_extends | fn add_interface_extends(child: String, parents: Vec<String>) | インタフェースextends登録 | O(k) | O(k) |
| TypeScriptInheritanceResolver::get_all_interfaces | fn get_all_interfaces(class_name: &str) -> Vec<String> | 全インタフェース取得 | O(V+E) | O(V) |
| TypeScriptProjectEnhancer::new | fn new(rules: ResolutionRules) -> Self | パスエイリアス解決器の生成 | O(P) | O(P) |
| ProjectResolutionEnhancer::enhance_import_path | fn enhance_import_path(&self, import_path: &str, _from_file: FileId) -> Option<String> | エイリアス解決（非相対のみ） | O(candidates) | O(1) |
| ProjectResolutionEnhancer::get_import_candidates | fn get_import_candidates(&self, import_path: &str, _from_file: FileId) -> Vec<String> | 候補列挙 | O(candidates) | O(candidates) |

以下、主要APIの詳細。

### TypeScriptResolutionContext::resolve

1. 目的と責務
   - 複数スコープと型空間/値空間、インポート、グローバル、修飾名（`.`）を考慮した**総合名前解決**。

2. アルゴリズム（ステップ分解）
   - 順序で探索:
     1. local_scope
     2. hoisted_scope
     3. imported_symbols
     4. module_symbols
     5. type_space
     6. global_symbols
     7. nameに`.`が含まれる場合の特別処理:
        - まず完全修飾名としてimported/module/globalを直接探索
        - 2部構成なら`class_or_module.member`をケース別に試行
          - namespace_aliasesにaliasがあればmember名をスコープ横断探索
          - `class_or_module`が解決可能なら`member`を再帰的に解決
          - その他は未解決

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| name | &str | 解決したい名前（修飾名含む可能性あり） |

4. 戻り値

| 型 | 説明 |
|----|------|
| Option<SymbolId> | 見つかった場合Some(id)、なければNone |

5. 使用例

```rust
let ctx = TypeScriptResolutionContext::new(FileId::new(1).unwrap());
ctx.add_import_symbol("React".to_string(), SymbolId::new(10).unwrap(), false);
assert_eq!(ctx.resolve("React"), Some(SymbolId::new(10).unwrap()));
```

6. エッジケース
- `"A.B.C"`の3段以上の修飾は特別処理が限定的（2部想定）。完全一致が事前登録されていないと解決困難。
- `namespace_aliases`がある場合にmemberを**スコープ横断**するため、誤解決/影響範囲拡大の恐れ。
- `qualified_names`は登録されるがresolveで未使用（機能不全）。

Mermaid（分岐が多いため図示）:

```mermaid
flowchart TD
  A[resolve(name)] --> B{local_scope contains?}
  B -- Yes --> Z1[return id]
  B -- No --> C{hoisted_scope contains?}
  C -- Yes --> Z2[return id]
  C -- No --> D{imported_symbols contains?}
  D -- Yes --> Z3[return id]
  D -- No --> E{module_symbols contains?}
  E -- Yes --> Z4[return id]
  E -- No --> F{type_space contains?}
  F -- Yes --> Z5[return id]
  F -- No --> G{global_symbols contains?}
  G -- Yes --> Z6[return id]
  G -- No --> H{name contains '.'?}
  H -- No --> Z7[return None]
  H -- Yes --> I{full qualified found? (imported/module/global)}
  I -- Yes --> Z8[return id]
  I -- No --> J{parts.len()==2?}
  J -- No --> Z9[return None]
  J -- Yes --> K{namespace_aliases contains class_or_module?}
  K -- Yes --> L{member in any scope?}
  L -- Yes --> Z10[return id]
  L -- No --> M{resolve(class_or_module) exists?}
  M -- Yes --> N[return resolve(member)]
  M -- No --> Z11[return None]
```

上記の図は`resolve`関数の主要分岐（行番号: 不明）を示す。

---

### TypeScriptResolutionContext::add_symbol_with_context

1. 目的と責務
   - **ScopeContext**に基づき、**ホイスティング**や**ブロックスコープ**などTS特性に合わせて適切なテーブルへ登録。

2. アルゴリズム
   - `ScopeContext::Local{hoisted: true}` → hoisted_scope
   - `ScopeContext::Local{hoisted: false}` / Parameter / ClassMember → local_scope
   - Module → module_symbols
   - Package → imported_symbols
   - Global → global_symbols
   - None → local_scope

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| name | String | シンボル名（場合によりモジュールパスも） |
| symbol_id | SymbolId | 識別子 |
| scope_context | Option<&crate::symbol::ScopeContext> | スコープ文脈（TSの宣言種別など） |

4. 戻り値
- なし

5. 使用例

```rust
use crate::symbol::ScopeContext;
let mut ctx = TypeScriptResolutionContext::new(FileId::new(1).unwrap());
ctx.add_symbol_with_context("myFunc".to_string(), SymbolId::new(1).unwrap(),
    Some(&ScopeContext::Local{ hoisted: true, /* ... */ }));
```

6. エッジケース
- `ScopeContext`がNoneの場合、**local_scope**に入るためホイスティングされない。
- Parser未統合だと**誤ったスコープ配置**の可能性（コメントにも負債として記載）。

---

### ResolutionScope::resolve_relationship

1. 目的と責務
   - 関係種別（Calls/Uses/Extends/Implements等）に応じた**解決戦略の選択**。

2. アルゴリズム（主な分岐）
   - Implements/Extends → `resolve(to_name)`
   - Calls → `to_name`に`.`があればまず`resolve(to_name)`、なければ末尾部分で再解決、最後に通常解決
   - Uses → まず`type_space`を見る、その後通常解決
   - その他 → 通常解決

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| _from_name | &str | 呼び出し元名（未使用） |
| to_name | &str | 対象名 |
| kind | crate::RelationKind | 関係種別 |
| _from_file | FileId | 呼び出し元ファイル（未使用） |

4. 戻り値

| 型 | 説明 |
|----|------|
| Option<SymbolId> | 解決結果 |

5. 使用例

```rust
let resolved = ctx.resolve_relationship("Comp", "Button", crate::RelationKind::Calls, FileId::new(2).unwrap());
```

6. エッジケース
- `Calls`で`.`ありの場合、**末尾名のみ**の再解決は**誤解決**が起きうる（同名関数が他スコープにある場合）。

---

### ResolutionScope::is_compatible_relationship

1. 目的と責務
   - 関係の**型互換性**をTSの慣習に合わせて判定。特に**Calls**で定数/変数をCallableとみなす。

2. ポイント
   - 呼び出し可能側: Function, Method, Macro, Module, Constant, Variable
   - 呼び出され得る側: Function, Method, Macro, Class, Constant, Variable
   - 他の関係は一般的なルール（Trait/Interfaceなど）を採用

3. エッジケース
- 定数/変数のCallable扱いは**TS/JSの実例（関数値/Arrow関数）**を反映。型厳密性は低い。

---

### InheritanceResolver::resolve_method / get_inheritance_chain / is_subtype / get_all_methods

1. 目的と責務
   - メソッドがどこで定義されたかを上位親や実装インタフェース、拡張インタフェースまで探索。
   - 継承鎖を列挙し、サブタイプ関係を判定。
   - すべての継承/実装起源のメソッドを集約。

2. アルゴリズム
   - resolve_method:
     - 自型→親クラス→実装インタフェース→拡張インタフェースの順に探索（再帰）
   - get_inheritance_chain:
     - Setで訪問済みを管理しながら、親クラス、実装インタフェース、その親インタフェースを再帰収集
   - is_subtype:
     - 直接extends/implementsを確認し、さらに親連鎖へ再帰
   - get_all_methods:
     - DFSでクラス親、実装インタフェース、拡張インタフェースを辿りメソッド重複を除外して収集

3. 引数・戻り値（代表例: is_subtype）

| 引数名 | 型 | 説明 |
|--------|----|------|
| child | &str | 子型名 |
| parent | &str | 親型名 |

| 戻り値型 | 説明 |
|----------|------|
| bool | サブタイプかどうか |

4. 使用例

```rust
let mut inh = TypeScriptInheritanceResolver::new();
inh.add_inheritance("Child".to_string(), "Base".to_string(), "extends");
assert!(inh.is_subtype("Child", "Base"));
```

5. エッジケース
- **インタフェース判定がヒューリスティック**（`is_interface`）。誤分類のリスクあり。
- **循環継承**時は訪問Setにより無限ループは回避。ただし**意味的整合性**は別途要検討。

---

### TypeScriptProjectEnhancer::{enhance_import_path, get_import_candidates}

1. 目的と責務
   - tsconfigの**paths/baseUrl**でインポートパスを**解決/候補化**。相対パスはスキップ。

2. アルゴリズム
   - enhance_import_path:
     - 相対ならNone
     - resolverがあれば候補の先頭を返却
   - get_import_candidates:
     - 相対なら元パスのみ
     - resolverが候補を返せばそれを返し、空なら元パス

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| import_path | &str | インポートパス |
| _from_file | FileId | ファイルID（未使用） |

4. 戻り値
- enhance_import_path: Option<String>
- get_import_candidates: Vec<String>

5. 使用例

```rust
let enhancer = TypeScriptProjectEnhancer::new(rules);
let p = enhancer.enhance_import_path("@/components/Button", FileId::new(1).unwrap());
```

6. エッジケース
- **変換後パスの正規化**と**モジュールパスへの写像**はこのコンテキスト外。解決器側での対応が必要。

## Walkthrough & Data Flow

- フロー（名前解決）
  - シンボルは`add_import_symbol`や`add_symbol_with_context`/`add_symbol`で各テーブルに格納。
  - `resolve`は優先順にHashMapを**O(1)**で検索し、`.`を含む場合は特別ロジックへ。
  - `resolve_relationship`は関係種別により`resolve`または`type_space`を事前参照。
  - `symbols_in_scope`は各テーブルの内容をScopeLevel付きで集約。

- データ契約
  - ImportBinding: `register_import_binding`で`exposed_name`をキーに保持し、`import_binding(name)`で取得。
  - tsconfig→PathAliasResolver: `ResolutionRules`から最小構成の`TsConfig`を組み立て、resolver生成。

- スコープ遷移
  - `enter_scope`は`scope_stack`へpush。
  - `exit_scope`はpop後、スタック頂点が`None | Module | Global`なら`clear_local_scope()`と`hoisted_scope.clear()`を実行。
    - 関数スコープ離脱時のクリアを意図しているが、`ScopeType`の具体値はこのチャンクにないため、正確性は「不明」。

## Complexity & Performance

- `resolve`: 平均O(1)。`.`パスの分解時にO(k)（k=パーツ数、実装は2部想定＋完全一致）。
- `symbols_in_scope`: O(n)（n=全テーブル合計エントリ）。
- `get_inheritance_chain`/`get_all_methods`/`is_subtype`: グラフ探索でO(V+E)。メソッド数M分の重複除外は追加O(M)。
- `enhance_import_path`/`get_import_candidates`: resolverの候補生成に依存（外部）、通常は候補数に線形。
- スケール限界/ボトルネック
  - **修飾名の不完全処理**により、長いパスの解決で**失敗/再試行**が発生。
  - **namespace_aliasesのメンバー横断探索**は誤解決を招き得るが計算量は小さい。
  - 大規模プロジェクトでの`symbols_in_scope`は**メモリコピー**（Vec生成）が増加。

- 実運用負荷要因
  - I/O/ネットワーク/DBなし。純CPU/メモリ。
  - tsconfig resolverが大規模エイリアスを持つ場合、候補列挙が増える。

## Edge Cases, Bugs, and Security

- エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | None | `resolve`がHashMapに無い→最後None | 動作する |
| 3部以上の修飾名 | "A.B.C" | 完全一致登録があれば解決、なければ階層処理 | 2部のみ特別処理、完全一致のみ | 制限あり |
| namespace alias誤解決 | alias="React", name="React.useEffect" | alias配下のメンバーに限定 | スコープ横断で`useEffect`を探索 | バグの可能性 |
| qualified_names未使用 | 登録済み"Utils.helper" | 事前計算の即解決 | `resolve`はqualified_namesを参照しない | バグ |
| ホイスティング誤テスト | test_typescript_hoisting | hoisted_scopeへ登録を期待 | `add_symbol(Local)`はlocal_scopeに入る | テスト誤認 |
| インタフェース判定 | "IFoo"やクラス未登録名 | Parser情報で判定 | `I`接頭辞や非クラスヒューリスティック | 不安定 |
| スコープクリア時機 | exit_scope | 関数離脱時にlocal/hoistedをクリア | `ScopeType`詳細不明 | 不明 |

- バグ詳細
  - **qualified_names未使用**: `add_qualified_name`はあるが`resolve`内で未参照。事前計算の利点が活かされない。
  - **ホイスティングテストの仮定違い**: `test_typescript_hoisting`は「ホイストされた」とコメントするが、実コードは`local_scope`に追加（`ResolutionScope::add_symbol`はホイスティング情報にアクセスしないとコメントあり）。修正必要。
  - **namespace_aliasesの挙動**: aliasがあるとメンバー名を**すべてのスコープ**から探すため、同名のローカル定義を誤って拾う可能性。
  - **エンハンス後のパス→モジュールパス変換欠如**: テスト`test_enhanced_import_resolution_workflow`が示す通り、強化されたインポートパスと内部`module_path`のマッピングが未実装。

- セキュリティチェックリスト
  - メモリ安全性: 
    - Buffer overflow / Use-after-free / Integer overflow: HashMapとStringのみ。**unsafe無し**、Rustの所有権で安全。
  - インジェクション:
    - SQL/Command/Path traversal: 不該当。このモジュールはパス文字列のみ扱う。外部I/O無し。
  - 認証・認可: 不該当。
  - 秘密情報:
    - Hard-coded secrets: なし。
    - Log leakage: `debug_print!`は名前を出力。機密の取り扱いは**不明**（このチャンクには現れない）。
  - 並行性:
    - Race condition / Deadlock: この構造体は**内部可変**でスレッドセーフ設計無し。並行共用時は**Mutex/RwLock**が必要（このチャンクには同期化なし）。

## Design & Architecture Suggestions

- **AST統合**（必須）
  - ホイスティング、型空間、インタフェース/クラス判定を**AST/型情報**から決定し、`add_symbol_with_context`に正確な`ScopeContext`を供給。

- **qualified_namesの活用**
  - `resolve`の`.`分岐で、最初に`qualified_names`を参照するロジックを追加。完全修飾の高速解決を実現。

- **モジュールパス正規化層**
  - `TypeScriptProjectEnhancer`出力（強化後パス）→**モジュールパス**への変換関数を導入し、`add_symbol`時に両方のキーで登録。
  - パスの**ドット表現**への正規化規約をプロジェクト全体で統一。

- **名前空間エイリアスの厳密解決**
  - alias→モジュールの**表引き**を保持し、`alias.member`はその**モジュール内のエクスポート**に限定して解決。現在のスコープ横断探索をやめる。

- **スコープスタックの意味付け**
  - `ScopeType`に基づき関数スコープ離脱時のみ`hoisted_scope.clear()`させるよう**厳密化**（このチャンクでは`ScopeType`の詳細不明）。

- **エラーモデル整備**
  - `resolve`に**診断情報**（なぜ解決できないか）を返す設計（例えば`Result<SymbolId, ResolutionError>`）。現在は`None`のみ。

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト（既存＋追加）
  - 修飾名解決
    - 完全修飾名が`qualified_names`にある場合の解決
    ```rust
    // 例: qualified_namesを使うテスト（仮、実装追加後）
    let mut ctx = TypeScriptResolutionContext::new(FileId::new(1).unwrap());
    let id = SymbolId::new(100).unwrap();
    ctx.add_qualified_name("Utils.helper".to_string(), id);
    // ctx.resolveがqualified_namesを見るようにした後:
    assert_eq!(ctx.resolve("Utils.helper"), Some(id));
    ```
  - 名前空間エイリアス
    - alias="React" → React配下の`useEffect`のみ解決、他スコープの同名は解決しない
  - ホイスティング
    - `add_symbol_with_context`で`ScopeContext::Local{hoisted: true}`を入れて`hoisted_scope`に入ることを検証
  - モジュールパス
    - **強化後パス**から**モジュールパス**へ変換ロジック追加後、`resolve`がモジュールパスで解決可能か検証

- 統合テスト
  - tsconfigの`paths/baseUrl`を読み込んだ`TypeScriptProjectEnhancer`を使い、複数候補から正しいモジュールにリンクできるか。
  - 継承解決
    - `get_all_methods`がクラス親・実装インタフェース・拡張インタフェースのメソッドを重複なしで集約できるか。

- 既存テストの見直し
  - `test_typescript_hoisting`: コメントと期待がコード実態と不一致。`add_symbol_with_context`を用いる形へ変更。

## Refactoring Plan & Best Practices

- `resolve`の`.`処理を**関数抽出**（例: `resolve_qualified(name)`）して責務分離。
- `qualified_names`と`namespace_aliases`のデータ構造を**一元化**し、修飾メンバー探索の**正規経路**を構築。
- `symbols_in_scope`の出力でソートや重複排除などの**表示整形**を任意化（大規模時の観測性向上）。
- `import_bindings`の活用
  - import `{ Button as B }`のようなケースを**データ契約**で明確化し、`resolve`に統合。
- **Result vs Option**の適用
  - 解決失敗理由（未登録/空間衝突/曖昧参照）を識別するため`ResolutionError`導入。
- **ドキュメント整備**
  - スコープ優先順を**明記**（コメントに既にある順序をAPIドキュメントへ反映）。

## Observability (Logging, Metrics, Tracing)

- 現状: `debug_print!(self, "...")`による軽量ログ。解決パスの**探索ログ**あり。
- 提案:
  - **レベル化**（trace/debug/info）と**フィルタリング**。
  - ヒット/ミスの**メトリクス**（カウンタ）。
  - 解決時間の**プロファイル**（ヒートマップ）で重いパスを特定。
  - エイリアス解決の**候補数分布**の計測。

## Risks & Unknowns

- AST未統合のため、**型空間やホイスティング情報**が不完全（コメントにも技術的負債として記載、時期情報は不明）。
- `ScopeType`の詳細定義が**このチャンクには現れない**ため、`exit_scope`のクリア条件の正確性は不明。
- `PathAliasResolver::resolve_import`の候補生成アルゴリズムは**外部**で、詳細は不明。
- インタフェース判定が**ヒューリスティック**（`is_interface`）。誤分類が継承解決に影響。
- 並行利用時の**Sync/Send**要件が未定義。現状は**シングルスレッド前提**が安全。

以上を踏まえ、まずはASTとパス正規化の統合、`qualified_names`と名前空間の厳密解決の導入、テストの整合性確保が優先です。