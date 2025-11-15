# resolution.rs Review

## TL;DR

- 目的: Python特有のスコープ解決（LEGB）と継承（MRO）をRustで実装し、記号解決・継承関係・メソッド解決を提供
- 主要公開API: PythonResolutionContext（ResolutionScope実装）、PythonInheritanceResolver（InheritanceResolver実装）。特に resolve、resolve_relationship、populate_imports、add_inheritance、resolve_method が中心
- コアロジックの複雑箇所: ResolutionScope::resolve の段階的解決とドット区切り名の処理、MRO計算の再帰
- 重大リスク:
  - add_symbol_python のグローバル判定（scope_stack.len() == 1）が誤解決の可能性
  - resolve の2部構成名（module.method）をモジュール関連付けなしに単体名で再解決する挙動
  - populate_imports は内部の imports にのみ蓄積し imported_symbols に連動しないため、解決に反映されない
  - PythonInheritanceResolver の mro_cache が読み取りのみでキャッシュされない（効果ゼロ）

## Overview & Purpose

このモジュールは Python 言語のスコープ解決と継承モデルを解析・表現するためのコンテキストを提供します。

- スコープ解決（LEGB）: Local、Enclosing、Global、Built-in の順序で名前解決
- インポート: モジュール単位の import と from-import（別名付け含む）を追跡
- 継承: Python の MRO（Method Resolution Order）を簡略化モデルで算出し、メソッド検索・継承チェーン確認・サブタイプ判定を提供

このモジュールは crate::parsing の ResolutionScope および InheritanceResolver トレイトに準拠する具体実装で、Python 解析に特化した解決規則を持ちます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | PythonResolutionContext | pub | LEGB準拠の記号解決、インポート管理、スコープ管理 | High |
| Struct | PythonInheritanceResolver | pub | クラス継承管理、簡略MRO計算、メソッド解決 | Med |
| Type alias | ImportInfo | private | (name, optional_alias) | Low |
| Type alias | ModuleImports | private | Vec<(module_path, Vec<ImportInfo>)> | Low |
| Trait impl | ResolutionScope for PythonResolutionContext | pub(トレイト準拠) | 記号追加/解決、スコープ入退出、関係解決、インポート処理 | High |
| Trait impl | InheritanceResolver for PythonInheritanceResolver | pub(トレイト準拠) | 継承追加、メソッド解決、継承チェーン等 | Med |

### Dependencies & Interactions

- 内部依存
  - PythonResolutionContext
    - local_scope/enclosing_scope/global_scope/imported_symbols/builtin_scope: 名前→SymbolId のマップ
    - scope_stack: 現在のスコープ種別（ScopeType）を積む
    - imports: from-import の生データ追跡（ImportInfo）。後続解決に未統合
    - import_bindings: ImportBinding（crate::parsing::resolution）による見える名→バインディング
  - PythonInheritanceResolver
    - class_bases/class_methods: クラス階層・メソッド集合
    - mro_cache: MROのキャッシュを想定（現状未保存）

- 外部依存（推奨表）

| 依存 | 用途 | 備考 |
|------|------|------|
| crate::parsing::ResolutionScope | トレイト実装 | 公開API面に関与 |
| crate::parsing::InheritanceResolver | トレイト実装 | 公開API面に関与 |
| crate::parsing::Import | populate_imports の入力データ | 構造は不明（このチャンクに現れない） |
| crate::parsing::resolution::ImportBinding | import_binding 登録/取得 | 具体構造は不明 |
| crate::{FileId, SymbolId} | ファイルID、シンボルID | SymbolIdはCopyの可能性が高い（コードからの使用推測） |
| crate::RelationKind | resolve_relationship で使用 | Defines/Calls/Extends 他 |

- 被依存推定
  - Pythonコード解析器（AST→シンボルテーブル構築）
  - 参照解決フェーズ（名称→シンボルIDの紐付け）
  - 継承解析・型関連付け（クラス間関係、メソッド探索）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| PythonResolutionContext::new | fn new(file_id: FileId) -> Self | コンテキスト初期化 | O(1) | O(1) |
| PythonResolutionContext::add_import | fn add_import(&mut self, module: String, name: String, alias: Option<String>) | import情報を内部追跡に追加 | O(m) 探索＋挿入 | O(1) |
| PythonResolutionContext::add_symbol_python | fn add_symbol_python(&mut self, name: String, symbol_id: SymbolId, is_global: bool) | Python語義に沿ったシンボル追加 | O(1) | O(1) |
| PythonResolutionContext::push_enclosing_scope | fn push_enclosing_scope(&mut self) | ローカルから囲みスコープへ移動 | O(n) | O(n) |
| PythonResolutionContext::pop_enclosing_scope | fn pop_enclosing_scope(&mut self) | 囲みスコープのクリア | O(1) | O(1) |
| ResolutionScope::resolve | fn resolve(&self, name: &str) -> Option<SymbolId> | LEGB＋ドット名による解決 | O(k) | O(1) |
| ResolutionScope::enter_scope | fn enter_scope(&mut self, scope_type: ScopeType) | スコープ入場、必要な移動処理 | O(n) | O(n) |
| ResolutionScope::exit_scope | fn exit_scope(&mut self) | スコープ退出、クリーンアップ | O(n) | O(1) |
| ResolutionScope::resolve_relationship | fn resolve_relationship(&self, _from_name: &str, to_name: &str, kind: RelationKind, _from_file: FileId) -> Option<SymbolId> | 関係種別に応じた解決 | O(k) | O(1) |
| ResolutionScope::populate_imports | fn populate_imports(&mut self, imports: &[crate::parsing::Import]) | Importレコードから内部追跡構築 | O(n) | O(n) |
| ResolutionScope::register_import_binding | fn register_import_binding(&mut self, binding: ImportBinding) | 露出名→バインディング登録 | O(1) | O(1) |
| ResolutionScope::import_binding | fn import_binding(&self, name: &str) -> Option<ImportBinding> | バインディング取得 | O(1) | O(1) |
| PythonInheritanceResolver::new | fn new() -> Self | 継承リゾルバ初期化 | O(1) | O(1) |
| PythonInheritanceResolver::add_class | fn add_class(&mut self, class_name: String, bases: Vec<String>) | クラスと基底追加（MRO無効化） | O(1) | O(b) |
| PythonInheritanceResolver::add_class_methods | fn add_class_methods(&mut self, class_name: String, methods: Vec<String>) | メソッド登録 | O(1) | O(m) |
| InheritanceResolver::add_inheritance | fn add_inheritance(&mut self, child: String, parent: String, kind: &str) | 継承追加（extends/inherits） | O(1) | O(1) |
| InheritanceResolver::resolve_method | fn resolve_method(&self, type_name: &str, method_name: &str) -> Option<String> | MRO順でメソッド検索 | O(M) | O(1) |
| InheritanceResolver::get_inheritance_chain | fn get_inheritance_chain(&self, type_name: &str) -> Vec<String> | MRO取得 | O(M) | O(M) |
| InheritanceResolver::is_subtype | fn is_subtype(&self, child: &str, parent: &str) -> bool | サブタイプ判定 | O(M) | O(1) |
| InheritanceResolver::add_type_methods | fn add_type_methods(&mut self, type_name: String, methods: Vec<String>) | メソッド登録（型） | O(1) | O(m) |
| InheritanceResolver::get_all_methods | fn get_all_methods(&self, type_name: &str) -> Vec<String> | MRO合成で重複除去 | O(M+T^2) 重複チェック | O(T) |

注:
- k はスコープ数（最大 5: Local/Enclosing/Global/Imported/Builtin）＋ドット名処理の追加分岐
- n はローカル変数数（push/pop で移動・クリア）
- M は MRO内のクラス数
- T は全メソッド数（重複除去が線形探索のため O(T^2) 的側面あり）

以下、主要APIの詳細。

### PythonResolutionContext::new

1. 目的と責務
   - 新しい Python 解決コンテキストの初期化（file_id の保持、各スコープの空初期化）

2. アルゴリズム
   - 全 HashMap/Vec を new() で初期化

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| file_id | FileId | 対象ファイルID |

4. 戻り値

| 型 | 説明 |
|----|------|
| Self | 初期化済みのコンテキスト |

5. 使用例
```rust
let mut ctx = PythonResolutionContext::new(file_id);
```

6. エッジケース
- 特になし（初期化のみ）

### PythonResolutionContext::add_import

1. 目的と責務
   - from-import 形式の内部追跡（module_path とその配下の name, alias の組を保持）

2. アルゴリズム（関数名:行番号不明）
   - imports に module が存在すれば追記、なければ新規追加

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| module | String | モジュールパス（例: "myapp.utils"） |
| name | String | インポートされた名前（例: "helper"、シンプル import 時は空文字が可能） |
| alias | Option<String> | 別名（as alias） |

4. 戻り値

| 型 | 説明 |
|----|------|
| () | なし |

5. 使用例
```rust
ctx.add_import("myapp.utils".to_string(), "helper".to_string(), Some("h".to_string()));
```

6. エッジケース
- "import os" のように name が空文字になるケースに注意（後続解決がこの imports を使わないため意味が限定的）

### PythonResolutionContext::add_symbol_python

1. 目的と責務
   - Python語義（global宣言やモジュールレベル）に基づき、適切なスコープへシンボル追加

2. アルゴリズム（関数名:行番号不明）
   - is_global または scope_stack が空 あるいは len()==1 のとき global_scope、それ以外は local_scope に挿入

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| name | String | シンボル名 |
| symbol_id | SymbolId | シンボルID |
| is_global | bool | グローバル宣言（Pythonのglobalキーワード相当） |

4. 戻り値

| 型 | 説明 |
|----|------|
| () | なし |

5. 使用例
```rust
ctx.add_symbol_python("x".to_string(), sym_x, false);
```

6. エッジケース
- scope_stack.len()==1 の場合に global_scope に入るロジックは誤解決の可能性（トップレベル関数内のローカルをグローバル扱いする恐れ）

### ResolutionScope::resolve

1. 目的と責務
   - LEGB順＋インポート＋ビルトイン＋ドット区切り名の特例処理で SymbolId を返す

2. アルゴリズム（関数名:行番号不明）
   - 順序: local → enclosing → global → imported → builtin
   - name に '.' を含む場合:
     - 完全修飾名として imported_symbols / global_scope のフルパスをまず検索
     - 2部構成（A.B）なら A を解決し、B を単体名として再解決（A に紐づけずに B の可視域から解決する）

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| name | &str | 解決対象名 |

4. 戻り値

| 型 | 説明 |
|----|------|
| Option<SymbolId> | 見つかれば Some、なければ None |

5. 使用例
```rust
let id_opt = ctx.resolve("myapp.utils.helper.process");
let id_opt2 = ctx.resolve("json.loads");
let id_opt3 = ctx.resolve("func_in_local");
```

6. エッジケース
- "A.B" で B を A に紐づけず単体名として再解決するため誤解決の可能性（別スコープの B に一致し得る）
- 完全修飾名の格納が imported_symbols/global_scope にない場合は 2部構成以外は None

### ResolutionScope::enter_scope

1. 目的と責務
   - スコープ入場時に必要な状態遷移（関数入場時、既存のローカルを囲みスコープへ移動）

2. アルゴリズム（関数名:行番号不明）
   - scope_stack が空でない状態で関数スコープに入ると push_enclosing_scope を呼び、scope_stack に積む

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| scope_type | ScopeType | Function/Class/Module 等 |

4. 戻り値

| 型 | 説明 |
|----|------|
| () | なし |

5. 使用例
```rust
ctx.enter_scope(ScopeType::Function { /* 不明 */ });
```

6. エッジケース
- 最初の関数入場時の挙動（scope_stackが空）では enclosing への移動が行われない設計

### ResolutionScope::exit_scope

1. 目的と責務
   - スコープ退出時のクリーンアップ（関数スコープ終了でローカルクリアと囲みスコープクリア）

2. アルゴリズム（関数名:行番号不明）
   - Function: clear_local_scope と pop_enclosing_scope を実行
   - Class: current_class を None に戻す（設定はこのチャンクでは行われていない）
   - その他: 何もしない

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| なし | なし | なし |

4. 戻り値

| 型 | 説明 |
|----|------|
| () | なし |

5. 使用例
```rust
ctx.exit_scope();
```

6. エッジケース
- pop_enclosing_scope が囲みスコープを全消去するため、ネストが深い場合に外側のキャプチャが失われる可能性

### ResolutionScope::resolve_relationship

1. 目的と責務
   - 関係種別（Defines/Calls/Extends など）に応じて最適化された名前解決

2. アルゴリズム（関数名:行番号不明）
   - Defines: そのまま resolve
   - Calls: ドット名ならまず完全修飾で resolve、失敗なら末尾要素だけを再解決
   - Extends: 親クラス名を resolve
   - その他: resolve

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| _from_name | &str | 呼び出し元名（未使用） |
| to_name | &str | 対象名 |
| kind | RelationKind | 関係種別 |
| _from_file | FileId | 呼び出し元ファイル（未使用） |

4. 戻り値

| 型 | 説明 |
|----|------|
| Option<SymbolId> | 解決結果 |

5. 使用例
```rust
let callee = ctx.resolve_relationship("caller", "json.loads", RelationKind::Calls, file_id);
```

6. エッジケース
- Calls で末尾だけを再解決するため、誤った関数に合致する可能性

### ResolutionScope::populate_imports

1. 目的と責務
   - Import レコード（path＋alias）から内部の imports 形式に変換・追加

2. アルゴリズム（関数名:行番号不明）
   - path に '.' が含まれる場合は末尾を name として抽出、前方を module として add_import
   - '.' がない場合は module=path, name=""（空文字）として add_import

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| imports | &[crate::parsing::Import] | インポートレコード群 |

4. 戻り値

| 型 | 説明 |
|----|------|
| () | なし |

5. 使用例
```rust
ctx.populate_imports(&[
    crate::parsing::Import { path: "myapp.utils.helper".to_string(), alias: Some("h".to_string()) },
]);
```

6. エッジケース
- imported_symbols への反映は行われないため、解決には直接寄与しない（追跡専用）

### ResolutionScope::register_import_binding / import_binding

1. 目的と責務
   - 露出名→ImportBinding の登録と取得（エイリアスや可視名を扱う）

2. アルゴリズム（関数名:行番号不明）
   - HashMap に insert、取得時は cloned を返す

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| binding（register） | ImportBinding | 登録するバインディング |
| name（import_binding） | &str | 取得する露出名 |

4. 戻り値

| 型 | 説明 |
|----|------|
| ()（register） | なし |
| Option<ImportBinding>（import_binding） | 見つかった場合 |

5. 使用例
```rust
ctx.register_import_binding(binding);
let b = ctx.import_binding("loads");
```

6. エッジケース
- ImportBinding の構造は不明（このチャンクには現れない）

### PythonInheritanceResolver::resolve_method

1. 目的と責務
   - クラス type_name の MRO を用いて method_name を最初に見つけるクラスを返す

2. アルゴリズム（関数名:行番号不明）
   - calculate_mro を取得
   - MRO順で class_methods を参照し、最初に一致したクラス名を返す

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| type_name | &str | 対象クラス名 |
| method_name | &str | メソッド名 |

4. 戻り値

| 型 | 説明 |
|----|------|
| Option<String> | 実際にメソッドを所有するクラス名 |

5. 使用例
```rust
let owner = inh.resolve_method("Child", "run");
```

6. エッジケース
- calculate_mro がキャッシュされない（mro_cache未活用）ため、繰り返し呼び出しでコストが増す

## Walkthrough & Data Flow

- スコープ追加
  - enter_scope(Function) で、既にスコープがある場合は push_enclosing_scope により現在の local_scope を囲みスコープへ移動
  - exit_scope(Function) では clear_local_scope と pop_enclosing_scope でクリーンアップ

- 記号追加
  - add_symbol (トレイト版): ScopeLevel に応じて local/global/imported に挿入
  - add_symbol_python: is_global やスコープ状況により global または local に挿入

- 名前解決
  - resolve: LEGB順→imported→builtin→ドット名（完全修飾→2部構成の分解と再解決）で探索

- インポート
  - populate_imports: Import を内部の imports へ変換・追跡。resolve との直接連携は imported_symbols が必要だが、この関数は更新しない

- 継承・メソッド解決
  - add_inheritance/add_class: 階層情報を class_bases に保存
  - resolve_method: calculate_mro により線形化した順序で class_methods から探索

### Mermaid: resolve 関数の主要分岐

```mermaid
flowchart TD
    A[resolve(name)] --> B{Localに存在?}
    B -- はい --> R1[返す: Local]
    B -- いいえ --> C{Enclosingに存在?}
    C -- はい --> R2[返す: Enclosing]
    C -- いいえ --> D{Globalに存在?}
    D -- はい --> R3[返す: Global]
    D -- いいえ --> E{Importedに存在?}
    E -- はい --> R4[返す: Imported]
    E -- いいえ --> F{Builtinに存在?}
    F -- はい --> R5[返す: Builtin]
    F -- いいえ --> G{nameに'.'含む?}
    G -- いいえ --> Z[None]
    G -- はい --> H{フル修飾名がImportedに存在?}
    H -- はい --> R6[返す: Imported(フル)]
    H -- いいえ --> I{フル修飾名がGlobalに存在?}
    I -- はい --> R7[返す: Global(フル)]
    I -- いいえ --> J{nameを'.'で分割し2部構成?}
    J -- いいえ --> Z2[None]
    J -- はい --> K{先頭(A)が解決可能?}
    K -- いいえ --> Z3[None(外部lib想定)]
    K -- はい --> L[Bを単体名として再resolve]
    L --> R8[返す: 再解決結果 or None]
```

上記の図は `ResolutionScope::resolve` 関数（行番号不明）の主要分岐を示す。

## Complexity & Performance

- 名前解決
  - 時間: 各スコープの HashMap 検索は平均 O(1)。分岐は最大 5＋ドット名補助分岐。よって平均 O(1)（定数係数は分岐数に依存）。
  - 空間: スコープ内のシンボル数に線形。

- スコープ遷移
  - push_enclosing_scope: local_scope を take して enclosing に移動（O(n)）
  - pop_enclosing_scope / clear_local_scope: O(1)〜O(n)（HashMapの clear）

- インポート処理
  - populate_imports: 入力 n に対して O(n)

- 継承・MRO
  - calculate_mro: 再帰＋重複チェックのため MRO長に対して線形〜中程度（PythonのC3線形化ではない簡略版）
  - resolve_method: MRO長 M と各クラスのメソッド探索（vectorの any）で O(M + 総メソッド数)
  - get_all_methods: 重複除去に Vec::contains を用いるため O(T^2) 的。HashSet の採用で改善可能。

- ボトルネック/スケール限界
  - import の追跡（imports フィールド）は解決に直接紐づいていないため、imported_symbols との整合がない限り効果が薄い
  - MROキャッシュ未活用により大量クエリでコスト増
  - get_all_methods の重複除去コスト（Vec 使用）が増大

- 実運用負荷要因
  - 大規模コードベースでの頻繁な resolve 呼び出し
  - 深いネストスコープの push/pop による移動コスト
  - 複数継承階層・多数メソッドでのメソッド探索

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 2部構成のドット名誤解決 | "A.B"（Aは存在、Bは別スコープに同名あり） | Aに属するBを探索、なければNone | Bを単体名で再解決（Aと非連動） | 要修正 |
| add_symbol_pythonのグローバル誤判定 | スコープスタック長=1の関数内でローカル追加 | Localに入る | Globalに入る可能性あり | 要修正 |
| imports と imported_symbolsの不整合 | populate_imports後にresolve | imported_symbolsにも反映し解決可能 | importsのみ更新 | 要修正 |
| MROキャッシュ未使用 | resolve_methodを大量に呼ぶ | 計算結果をキャッシュして再利用 | mro_cacheは読み取りのみ、保存なし | 要修正 |
| enclosingの消去 | ネスト関数からさらに退出 | 外側の囲みスコープは保持 | pop_enclosing_scopeで全消去 | 要検討 |
| "import os"の空name | pathに'.'なし | importを正しく表現 | name=""で保持 | 設計注意 |
| current_class未使用 | Classスコープ入退出 | 現在クラスの追跡に利用 | exit_scopeでNoneにするのみ | 不明 |
| Built-in未設定 | "len"など | builtinsがあれば解決 | builtin_scopeは空 | 設計不足 |

セキュリティチェックリスト
- メモリ安全性
  - Buffer overflow / Use-after-free: HashMap/Vec の安全な使用のみ。unsafe無し。問題なし。
  - Integer overflow: インデックス操作なし。問題なし。
- インジェクション
  - SQL/Command/Path traversal: 対象外。外部コマンド呼び出しなし。問題なし。
- 認証・認可
  - 権限チェック: 対象外。
- 秘密情報
  - ハードコード秘密: なし。
  - ログ漏えい: ログ機構なし。問題なし。
- 並行性
  - Race condition / Deadlock: 共有可変構造体（HashMap）を多スレッドで共有する場合にレースの可能性。現状同期機構なし。

Rust特有の観点（詳細チェックリスト）
- 所有権
  - push_enclosing_scope（関数名:行番号不明）で std::mem::take により local_scope の所有権を移動。安全だが、再入時の期待動作に注意。
- 借用
  - resolve などで &self を用いた読み取り。可変借用の期間は enter/exit 操作時のみ。
- ライフタイム
  - 明示的ライフタイム不要。文字列は所有 String、参照は &str のみ。
- unsafe境界
  - なし。
- 並行性・非同期
  - Send/Sync: HashMap<String, SymbolId> は構成型に依存（SymbolId が Send/Sync か不明）。多スレッド使用時はArc/Mutex等が必要。
  - データ競合: &mut self を要求するため単スレッド前提で安全だが、共有時は要同期。
  - await境界/キャンセル: 非同期なし。
- エラー設計
  - Option のみで不可解決を表現。失敗理由の粒度が不足。Result＋エラー型の導入で改善可能。
  - panic: unwrap/expect 非使用。

## Design & Architecture Suggestions

- resolve のドット名処理の改善
  - "A.B" のとき、A の名前空間に属する B を探索する（モジュール/クラスのメンバー表を保持）。現状は B を単体名で再解決しており誤解決リスクが高い。
- add_symbol_python の判定修正
  - scope_stack.len()==1 で global に入れるロジックを撤廃し、関数スコープ時は Local をデフォルトに。
- imports と imported_symbols の統合
  - populate_imports 実行時に imported_symbols を更新するか、import_bindings を用いて resolve 時にマップする。
- MROキャッシュ適用
  - calculate_mro 完了時に mro_cache.insert(class_name, mro.clone()) を行いキャッシュを有効化。
- get_all_methods の重複除去
  - Vec::contains ではなく HashSet を使い O(T) に改善。
- スコープモデルの明確化
  - scope_stack に応じた「フレーム」構造（Local/Enclosing をフレームで表す）を導入し、push/pop をインデックス移動にする。
- Built-in の統合
  - Python ビルトイン群（len, range など）の SymbolId を外部データでロードし builtin_scope を初期化。

## Testing Strategy (Unit/Integration) with Examples

- resolve のLEGB順
```rust
// Arrange
let mut ctx = PythonResolutionContext::new(file_id);
ctx.add_symbol("x".to_string(), sym_global, ScopeLevel::Global);
ctx.add_symbol("x".to_string(), sym_imported, ScopeLevel::Package);
ctx.enter_scope(ScopeType::Function { /* 不明 */ });
ctx.add_symbol("x".to_string(), sym_local, ScopeLevel::Local);

// Act
let id = ctx.resolve("x");

// Assert: Local優先
assert_eq!(id, Some(sym_local));
```

- ドット名解決（完全修飾名）
```rust
let mut ctx = PythonResolutionContext::new(file_id);
ctx.add_symbol("json.loads".to_string(), sym_loads_full, ScopeLevel::Package);
assert_eq!(ctx.resolve("json.loads"), Some(sym_loads_full));
```

- ドット名2部構成の誤解決検出
```rust
let mut ctx = PythonResolutionContext::new(file_id);
// "json" は存在、"loads" は別のスコープに同名あり
ctx.add_symbol("json".to_string(), sym_json, ScopeLevel::Global);
ctx.add_symbol("loads".to_string(), sym_other_loads, ScopeLevel::Global);
assert_eq!(ctx.resolve("json.loads"), Some(sym_other_loads)); // 現状の挙動
// 期待: json 名前空間配下の loads に限定した探索
```

- imports と imported_symbols の整合テスト
```rust
let mut ctx = PythonResolutionContext::new(file_id);
ctx.populate_imports(&[crate::parsing::Import { path: "pkg.mod.func".to_string(), alias: None }]);
// 期待: resolve("pkg.mod.func") が成功
// 現状: imports は更新されるが imported_symbols は未更新のため失敗
assert_eq!(ctx.resolve("pkg.mod.func"), None);
```

- 継承・メソッド解決
```rust
let mut inh = PythonInheritanceResolver::new();
inh.add_class("Base".to_string(), vec![]);
inh.add_class_methods("Base".to_string(), vec!["run".to_string()]);
inh.add_class("Child".to_string(), vec!["Base".to_string()]);
assert_eq!(inh.resolve_method("Child", "run"), Some("Base".to_string()));
```

- MROキャッシュの有効化テスト（修正後想定）
```rust
// 修正: calculate_mro で mro_cache に保存する
let m1 = inh.get_inheritance_chain("Child"); // 計算してキャッシュ
let m2 = inh.get_inheritance_chain("Child"); // キャッシュヒット（ベンチで高速化確認）
```

## Refactoring Plan & Best Practices

- ステップ1: add_symbol_python の判定ロジック変更（関数スコープは常に Local。global 宣言時のみ Global）
- ステップ2: imports→imported_symbols の連動
  - populate_imports で SymbolId を関連付けられるなら即反映
  - できない場合は ImportBinding を必ず介す解決パスを用意して resolve が参照
- ステップ3: resolve のドット名処理改善
  - 名前空間ツリー（Module/Class→子シンボル）を導入し、"A.B" は A の子からのみ探索
- ステップ4: MROキャッシュの保存実装
  - calculate_mro 終了時に mro_cache.insert
- ステップ5: get_all_methods の重複除去を HashSet に変更
- ステップ6: スコープフレーム抽象化
  - Vec<Frame>（Frame: { locals, nonlocals(enclosing), type }）に置換し、push/popでフレーム操作
- ステップ7: エラー型導入
  - Option から Result<SymbolId, ResolveError> へ拡張（原因別の可視化）

## Observability (Logging, Metrics, Tracing)

- ログ
  - resolve 失敗時にレベル別（debug）で「探索したスコープ」「ドット名の分解結果」「末尾再解決を行った」等を記録
  - populate_imports で「インポートの追加」「alias」情報を記録
- メトリクス
  - 解決成功率、失敗率、ドット名成功/失敗件数
  - MRO計算呼び出し回数とキャッシュヒット率
- トレーシング
  - resolve のスパンを作り、各スコープ検索をイベントとして記録
  - resolve_relationship の種別ごとのフロー

## Risks & Unknowns

- ImportBinding/Import の詳細構造が不明（このチャンクには現れない）
- current_class の割り当てロジックが不明（このチャンクには現れない）
- SymbolId の Send/Sync 特性が不明（並行使用時の安全性に影響）
- Pythonの実際のC3 MROには未対応（簡略化のみ）
- Built-in シンボルの外部データ供給方法が不明（このチャンクには現れない）