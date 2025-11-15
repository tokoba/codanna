# language_behavior.rs Review

## TL;DR

- 目的: 各言語の慣習と解決ロジックを抽象化する**LanguageBehavior**トレイトにより、インデクサを言語非依存にする。
- 主要公開API: **LanguageBehavior**（多数のデフォルト実装）、**LanguageMetadata**（ABI-15メタデータ）。
- コアロジック: インポート解決と可視性適用を行う**build_resolution_context**／**build_resolution_context_with_cache**、コンテキスト付きの**resolve_import_path_with_context**。
- 複雑箇所: インポートの起源分類、Tantivy上の永続インポートとメモリ内のインポートのマージ、キャッシュ併用時の候補検証。
- Rust安全性: トレイトが**Send + Sync**制約。状態管理の実装側は**内部可変性**（Mutex など）必須。unsafeは不使用。
- 重大リスク: 区切り文字末尾（例: "crate::"）で空のセグメント生成→空シンボル登録の可能性、get_all_symbols(10000)の高コスト、インポート重複や別名競合の処理。
- テスト: 互換性判定のユニットテストあり。インポート解決・可視性・別名の詳細テストは未整備。

## Overview & Purpose

このモジュールは、言語固有の規則（モジュールパスのフォーマット、可視性、インポート解決、型／メソッド関係など）を**LanguageBehavior**トレイトで抽象化し、コアインデクサ（SimpleIndexer）から言語分岐を排除する目的で設計されています。

主な責務は以下のとおりです。

- モジュールパスと可視性の解釈（例: Rustの"::"やpub、Pythonの"."や先頭アンダースコア）。
- インポート解決のための**解決コンテキスト**構築（スコープ、可視性、インポート起源の記録）。
- **ツリーシッター**（ABI-15）によるノード種別の検証とメタデータ取得。
- 継承/実装関係やメソッド由来のマッピング。
- インデクサが**外部呼び出し**のスタブを作成すべきかの判断用フック。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Trait | LanguageBehavior | pub | 言語固有の規則・解決ロジックの抽象化（多数のデフォルト実装） | High |
| Struct | LanguageMetadata | pub | ABI-15メタデータの簡易保持（abi_version, node_kind_count, field_count） | Low |
| Macro | debug_global! | module内 | グローバル設定に基づくデバッグ出力 | Low |
| Function (assoc) | LanguageMetadata::from_language | pub | Languageからメタデータ生成 | Low |
| Method | LanguageBehavior::build_resolution_context | pub(default impl) | 解決コンテキスト構築（永続+メモリのインポート統合、可視性適用） | High |
| Method | LanguageBehavior::build_resolution_context_with_cache | pub(default impl) | キャッシュ活用版の高速解決コンテキスト構築 | High |
| Method | LanguageBehavior::resolve_import_path_with_context | pub(default impl) | インポートパスをモジュールコンテキスト付きで解決 | Med |
| Method | LanguageBehavior::configure_symbol | pub(default impl) | 言語規則に基づくモジュールパス設定と可視性適用 | Low-Med |

### Dependencies & Interactions

- 内部依存:
  - Resolution関連: **GenericResolutionContext::new**（ResolutionScope実装）、**GenericInheritanceResolver::new**（InheritanceResolver実装）
  - Index関連: **DocumentIndex**（シンボル検索・取得、インポート取得）
  - 型: **FileId, IndexError, IndexResult, Symbol, SymbolId, Visibility**
  - ログ: **debug_global!**
- 外部依存（クレート・モジュール）:

| クレート/モジュール | 用途 |
|--------------------|------|
| tree_sitter::Language | ABI-15言語メタデータ、ノード種別検証 |
| tree_sitter_rust::LANGUAGE (tests) | テスト用ダミーLanguage |

- 被依存推定:
  - ParserFactory/LanguageParser と組み合わせる上位**インデクサ**（SimpleIndexer）
  - 解析パイプライン中の**解決フェーズ**（シンボル参照解決）
  - 可視性計算・関係構築（RelationKindとのマッピング）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| LanguageBehavior | pub trait LanguageBehavior: Send + Sync | 言語固有ロジックの抽象 | — | — |
| format_module_path | fn format_module_path(&self, base: &str, name: &str) -> String | モジュールパス整形 | O(len) | O(len) |
| parse_visibility | fn parse_visibility(&self, signature: &str) -> Visibility | 可視性解析 | O(len) | O(1) |
| module_separator | fn module_separator(&self) -> &'static str | 区切り文字提供 | O(1) | O(1) |
| supports_traits | fn supports_traits(&self) -> bool | トレイト概念有無 | O(1) | O(1) |
| supports_inherent_methods | fn supports_inherent_methods(&self) -> bool | 固有メソッド有無 | O(1) | O(1) |
| get_language | fn get_language(&self) -> tree_sitter::Language | ABI-15言語取得 | O(1) | O(1) |
| validate_node_kind | fn validate_node_kind(&self, kind: &str) -> bool | ノード種別検証 | O(1) | O(1) |
| get_abi_version | fn get_abi_version(&self) -> usize | ABIバージョン取得 | O(1) | O(1) |
| normalize_caller_name | fn normalize_caller_name(&self, name: &str, file_id: FileId) -> String | 呼出し元名の正規化 | O(len) | O(len) |
| configure_symbol | fn configure_symbol(&self, symbol: &mut Symbol, module_path: Option<&str>) | モジュール・可視性設定 | O(len) | O(len) |
| module_path_from_file | fn module_path_from_file(&self, file: &Path, root: &Path) -> Option<String> | ファイル→モジュールの写像 | O(path) | O(path) |
| resolve_import_path | fn resolve_import_path(&self, path: &str, index: &DocumentIndex) -> Option<SymbolId> | インポート解決（基本） | O(c) | O(1) |
| create_resolution_context | fn create_resolution_context(&self, file_id: FileId) -> Box<dyn ResolutionScope> | コンテキスト生成 | O(1) | O(1) |
| create_inheritance_resolver | fn create_inheritance_resolver(&self) -> Box<dyn InheritanceResolver> | 継承解決器生成 | O(1) | O(1) |
| add_import | fn add_import(&self, import: crate::parsing::Import) | インポート登録 | O(1) | O(1) |
| register_file | fn register_file(&self, path: PathBuf, file_id: FileId, module_path: String) | ファイル登録 | O(1) | O(1) |
| add_trait_impl | fn add_trait_impl(&self, type_name: String, trait_name: String, file_id: FileId) | トレイト実装登録 | O(1) | O(1) |
| add_inherent_methods | fn add_inherent_methods(&self, type_name: String, methods: Vec<String>) | 固有メソッド登録 | O(m) | O(m) |
| add_trait_methods | fn add_trait_methods(&self, trait_name: String, methods: Vec<String>) | トレイト定義メソッド登録 | O(m) | O(m) |
| resolve_method_trait | fn resolve_method_trait(&self, type_name: &str, method: &str) -> Option<&str> | メソッドの由来トレイト解決 | O(1..n) | O(1) |
| format_method_call | fn format_method_call(&self, recv: &str, method: &str) -> String | メソッド呼出し表現生成 | O(len) | O(len) |
| inheritance_relation_name | fn inheritance_relation_name(&self) -> &'static str | 継承関係名（extends/implements） | O(1) | O(1) |
| map_relationship | fn map_relationship(&self, specific: &str) -> RelationKind | 関係種別マッピング | O(1) | O(1) |
| build_resolution_context | fn build_resolution_context(&self, file_id: FileId, index: &DocumentIndex) -> IndexResult<Box<dyn ResolutionScope>> | 解決用コンテキストの構築 | O(I + Sf + Sg) | O(I + Sf + Sg) |
| build_resolution_context_with_cache | fn build_resolution_context_with_cache(&self, file_id: FileId, cache: &ConcurrentSymbolCache, index: &DocumentIndex) -> IndexResult<Box<dyn ResolutionScope>> | キャッシュ活用コンテキスト構築 | O(I + Sf + F·Sp + B) | O(I + Sf + F·Sp) |
| is_resolvable_symbol | fn is_resolvable_symbol(&self, symbol: &Symbol) -> bool | 解決対象判定 | O(1) | O(1) |
| is_symbol_visible_from_file | fn is_symbol_visible_from_file(&self, symbol: &Symbol, from_file: FileId) -> bool | 可視性判定 | O(1) | O(1) |
| get_imports_for_file | fn get_imports_for_file(&self, file_id: FileId) -> Vec<crate::parsing::Import> | メモリ内インポート取得 | O(k) | O(k) |
| resolve_import | fn resolve_import(&self, import: &crate::parsing::Import, index: &DocumentIndex) -> Option<SymbolId> | インポート→シンボルID解決 | O(c) | O(1) |
| import_matches_symbol | fn import_matches_symbol(&self, import_path: &str, symbol_module_path: &str, importing_module: Option<&str>) -> bool | インポートとモジュールの照合 | O(len) | O(1) |
| get_module_path_for_file | fn get_module_path_for_file(&self, file_id: FileId) -> Option<String> | ファイルのモジュールパス取得 | O(1) | O(1) |
| register_expression_types | fn register_expression_types(&self, file_id: FileId, entries: &[(String, String)]) | 式→型の補助情報登録 | O(n) | O(n) |
| initialize_resolution_context | fn initialize_resolution_context(&self, ctx: &mut dyn ResolutionScope, file_id: FileId) | コンテキスト初期化フック | O(1..n) | O(1..n) |
| classify_import_origin | fn classify_import_origin(&self, import: &crate::parsing::Import, resolved: Option<SymbolId>, importing_module: Option<&str>, index: &DocumentIndex) -> ImportOrigin | インポート起源分類 | O(1) | O(1) |
| is_compatible_relationship | fn is_compatible_relationship(&self, from_kind: SymbolKind, to_kind: SymbolKind, rel_kind: RelationKind, file_id: FileId) -> bool | 関係の整合性判定 | O(1) | O(1) |
| resolve_external_call_target | fn resolve_external_call_target(&self, to_name: &str, from_file: FileId) -> Option<(String, String)> | 外部呼出しターゲット推定 | O(1..n) | O(1) |
| create_external_symbol | fn create_external_symbol(&self, index: &mut DocumentIndex, module_path: &str, symbol_name: &str, language_id: crate::parsing::LanguageId) -> IndexResult<SymbolId> | 外部スタブ作成 | O(1..n) | O(1) |
| resolve_import_path_with_context | fn resolve_import_path_with_context(&self, path: &str, importing_module: Option<&str>, index: &DocumentIndex) -> Option<SymbolId> | コンテキスト付きインポート解決 | O(c) | O(1) |
| LanguageMetadata::from_language | fn from_language(language: tree_sitter::Language) -> Self | メタデータ生成 | O(1) | O(1) |

注:
- c: 同名候補数
- I: インポート数
- Sf: ファイル内シンボル数
- Sg: 全体から読み込むシンボル数（最大10000）
- F: インポート元ファイル数
- Sp: インポート元ファイル内の公開シンボル数
- B: 予備読み込み（fallback）件数（最大100）

以下、主要APIの詳細を解説します（全メソッドがこのチャンクに存在しますが、コアロジック中心に抜粋）。

1) 目的と責務
2) アルゴリズム
3) 引数
4) 戻り値
5) 使用例
6) エッジケース

### LanguageBehavior::build_resolution_context

1. 目的と責務
- 現在のファイルに対する解決可能シンボルのスコープを構築し、インポート・可視性・モジュール名義による解決を可能にします。

2. アルゴリズム（主な手順）
- ResolutionScope生成（create_resolution_context）
- Tantivy由来インポート＋メモリ内インポート（get_imports_for_file）のマージ
- populate_imports によりコンテキストへ生インポート投入
- 各インポートについて:
  - resolve_import（→ resolve_import_path_with_context）で解決
  - classify_import_origin で起源分類（内部/外部）
  - alias／末尾セグメント／フルパスの各露出名を ImportBinding として登録
  - 内部かつ解決成功なら、プライマリ名（alias優先）で Module スコープに追加
- 現ファイルのモジュール級シンボルをスキャンし、解決対象判定（is_resolvable_symbol）に基づき Module スコープに追加。module_path でも別名義追加。
- 他ファイルの公開シンボルを最大10000件読み込み、可視性判定（is_symbol_visible_from_file）に基づき Global スコープに追加し、module_pathでも追加。
- initialize_resolution_context フック呼出し。

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| file_id | FileId | 対象ファイルID |
| document_index | &DocumentIndex | シンボル・インポート情報の永続インデックス |

4. 戻り値

| 型 | 説明 |
|----|------|
| IndexResult<Box<dyn ResolutionScope>> | 成功時は解決スコープ、失敗時はIndexError（Tantivy操作のラップ） |

5. 使用例
```rust
fn build_ctx_for_file<B: LanguageBehavior>(
    behavior: &B,
    file_id: FileId,
    index: &DocumentIndex
) -> IndexResult<Box<dyn ResolutionScope>> {
    behavior.build_resolution_context(file_id, index)
}
```

6. エッジケース
- インポートが重複（Tantivyとメモリ）する場合は重複排除されるが、aliasは同一でも露出名が複数あり得る。
- 区切り文字末尾のパス（例: "crate::"）で末尾セグメントが空文字になりうる。空の露出名や空のプライマリ名が登録される可能性あり。
- get_all_symbols(10000) は高コスト。大量プロジェクトで負荷高。

Mermaidフローチャート:
```mermaid
flowchart TD
    A[Start build_resolution_context] --> B[create_resolution_context(file_id)]
    B --> C[get_imports_for_file from DB]
    C --> D[merge with in-memory imports]
    D --> E[context.populate_imports(imports)]
    E --> F{for each import}
    F -->|resolve_import| G[resolved_symbol?]
    G --> H[classify_import_origin]
    H --> I[derive binding names (alias, last segment, full path)]
    I --> J[register_import_binding for each exposed name]
    J --> K{Internal & resolved?}
    K -->|Yes| L[context.add_symbol(primary_name, Module)]
    K -->|No| M[skip add as symbol]
    F --> N[Next import]
    N --> O[find_symbols_by_file(file_id)]
    O --> P{for each symbol}
    P -->|is_resolvable_symbol| Q[add name & module_path (Module)]
    P -->|else| R[skip]
    Q --> S[get_all_symbols(limit=10000)]
    S --> T{for each symbol not same file}
    T -->|is_symbol_visible_from_file| U[add name & module_path (Global)]
    T -->|else| V[skip]
    U --> W[initialize_resolution_context]
    W --> X[Return context]
```
上記の図は`build_resolution_context`関数の主要分岐を示す（行番号: 不明）。

### LanguageBehavior::build_resolution_context_with_cache

1. 目的と責務
- シンボルキャッシュを活用して**DB問い合わせを最小化**し、メモリ使用量とレイテンシを削減しながら解決スコープを構築します。

2. アルゴリズム
- create_resolution_context → DB＋メモリインポートの統合 → populate_imports
- 各インポートについてキャッシュ候補 lookup_candidates(symbol_name, 16)
  - 候補が空なら DB fallback（resolve_import）
  - 候補が複数なら module_path と import_matches_symbol で検証 → 不一致なら DB fallback
- classify_import_origin → ImportBinding登録 → 内部かつ解決成功なら Module スコープへ追加
- 現ファイルの解決可能シンボルを Module スコープへ（name と module_path）
- インポート元ファイルをキャッシュから逆引き（lookup_by_name）し、そのファイルの公開シンボルのみ Global スコープへ追加
- インポートが皆無なら最小限（100件）のシンボル fallback 取得
- initialize_resolution_context

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| file_id | FileId | 対象ファイルID |
| cache | &ConcurrentSymbolCache | シンボル名→IDのキャッシュ |
| document_index | &DocumentIndex | 永続インデックス |

4. 戻り値

| 型 | 説明 |
|----|------|
| IndexResult<Box<dyn ResolutionScope>> | 構築されたスコープ、またはエラー |

5. 使用例
```rust
fn build_ctx_fast<B: LanguageBehavior>(
    behavior: &B,
    file_id: FileId,
    cache: &codanna::storage::symbol_cache::ConcurrentSymbolCache,
    index: &DocumentIndex,
) -> IndexResult<Box<dyn ResolutionScope>> {
    behavior.build_resolution_context_with_cache(file_id, cache, index)
}
```

6. エッジケース
- キャッシュヒットが間違ったモジュールパスの候補を返す場合、DBへのフォールバックが発生。
- get_imports_for_file(file_id) の使用箇所が「キャッシュ用のインポート元ファイル集合の推定」でTantivyの永続インポートを含まない可能性（メモリのみ）→ imported_files が過少になり得る。
- importing_module が None の場合、相対インポートの検証が不十分（import_matches_symbol を上書きすべき）。

Mermaidシーケンス図（アクター数>=3: Behavior, Cache, DocumentIndex, ResolutionScope）:
```mermaid
sequenceDiagram
    participant B as Behavior
    participant C as ConcurrentSymbolCache
    participant D as DocumentIndex
    participant R as ResolutionScope
    B->>B: create_resolution_context(file_id)
    B->>D: get_imports_for_file(file_id)
    B->>B: merge in-memory imports
    B->>R: populate_imports(imports)
    loop for each import
        B->>C: lookup_candidates(symbol_name, 16)
        alt candidates empty
            B->>D: resolve_import (DB)
        else candidates present
            loop for id in candidates
                B->>D: find_symbol_by_id(id)
                B->>B: import_matches_symbol(import.path, symbol.module_path, importing_module)
                alt matches
                    B->>B: matched = Some(id)
                    break
                else not match
                    B->>B: continue
                end
            end
            alt no matched
                B->>D: resolve_import (DB fallback)
            end
        end
        B->>B: classify_import_origin(...)
        B->>R: register_import_binding(...)
        alt Internal & resolved
            B->>R: add_symbol(primary_name, Module)
        end
    end
    B->>D: find_symbols_by_file(file_id)
    B->>R: add local resolvable symbols
    B->>C: lookup_by_name(symbol_name) // infer imported files
    B->>D: find_symbols_by_file(imported_file_id)
    B->>R: add visible symbols (Global)
    alt imported_files empty
        B->>D: get_all_symbols(100)
        B->>R: add visible fallback symbols
    end
    B->>B: initialize_resolution_context(R, file_id)
```
上記の図は`build_resolution_context_with_cache`関数の主要な相互作用を示す（行番号: 不明）。

### LanguageBehavior::resolve_import_path_with_context

1. 目的と責務
- インポートパスを**言語固有の照合規則**（import_matches_symbol）に基づいて候補のモジュールパスと比較し、最適なシンボルIDを返す。

2. アルゴリズム
- module_separator()で分割→末尾セグメントをシンボル名とし、index.find_symbols_by_name で候補取得
- 候補ごとに module_path を取り出し、import_matches_symbol(import_path, module_path, importing_module) で一致判定
- 最初に一致した候補のIDを返す

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| import_path | &str | インポートパス |
| importing_module | Option<&str> | インポート元モジュール（相対インポート判定用） |
| document_index | &DocumentIndex | インデックス |

4. 戻り値

| 型 | 説明 |
|----|------|
| Option<SymbolId> | 一致したID、またはNone |

5. 使用例
```rust
let id_opt = behavior.resolve_import_path_with_context("crate::foo::Bar", Some("crate::mod"), &index);
```

6. エッジケース
- import_path が空または分割後ゼロ要素→None
- 末尾セグメントが空（末尾が区切り）→候補検索が不正確
- importing_module 未指定時に相対インポート対応ができない（言語側override必要）

### LanguageBehavior::configure_symbol

1. 目的と責務
- シンボルに対し、**モジュールパス**の整形と**可視性**の適用を行う（言語規則）。

2. アルゴリズム
- module_path が Some の場合、format_module_path(base_path, symbol.name) を適用して symbol.module_path を設定
- signature が Some の場合、parse_visibility で可視性設定

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| symbol | &mut Symbol | 対象シンボル |
| module_path | Option<&str> | ベースのモジュールパス |

4. 戻り値

| 型 | 説明 |
|----|------|
| なし | 副作用で symbol を更新 |

5. 使用例
```rust
for s in &mut symbols {
    behavior.configure_symbol(s, Some("crate::module"));
}
```

6. エッジケース
- signature が None の場合、可視性は更新されない。
- format_module_path の言語実装が不正確だと、誤った module_path が保存される。

### LanguageBehavior::import_matches_symbol

1. 目的と責務
- 言語ごとに異なる**相対インポート**やエイリアスの規則を反映し、インポートパスとシンボルのモジュールパスの一致判定を行う。

2. アルゴリズム
- 既定は単純な**文字列完全一致**のみ。
- 言語側で override して相対解決（例: "helpers::func" vs "crate::module::helpers::func"）などをサポート。

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| import_path | &str | ソースコード上のインポートパス |
| symbol_module_path | &str | インデックス上の完全モジュールパス |
| importing_module | Option<&str> | 相対インポート許容の判断材料 |

4. 戻り値

| 型 | 説明 |
|----|------|
| bool | 一致ならtrue |

5. 使用例
```rust
let ok = behavior.import_matches_symbol("helpers::func", "crate::module::helpers::func", Some("crate::module"));
```

6. エッジケース
- importing_module が None の場合に相対一致が不可（既定のままではfalse）。
- 大文字小文字差異（言語規則次第で正規化が必要）。

### LanguageBehavior::is_resolvable_symbol

1. 目的と責務
- スコープに追加するべきか（解決対象か）を判定。

2. アルゴリズム
- symbol.scope_context がある場合:
  - Module/Global/Package → true
  - Local/Parameter → false
  - ClassMember → 可視性が Public のとき true
- scope_context がない場合:
  - SymbolKind によるホワイトリスト（Function, Method, Struct, Trait, Interface, Class, TypeAlias, Enum, Constant）

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| symbol | &Symbol | 対象 |

4. 戻り値

| 型 | 説明 |
|----|------|
| bool | 解決対象ならtrue |

5. 使用例
```rust
if behavior.is_resolvable_symbol(&symbol) {
    ctx.add_symbol(symbol.name.to_string(), symbol.id, ScopeLevel::Module);
}
```

6. エッジケース
- ClassMember の非公開が除外されるため、言語の「内部可視性」規則と不整合がありうる（override推奨）。
- scope_context 未設定時の後方互換ロジックが過剰/過少を生みうる。

### LanguageBehavior::is_symbol_visible_from_file

1. 目的と責務
- 異なるファイル間の可視性判定。

2. アルゴリズム
- 同一ファイルなら常に可視
- それ以外は Visibility::Public のみ可視

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| symbol | &Symbol | 対象 |
| from_file | FileId | 判定元ファイル |

4. 戻り値

| 型 | 説明 |
|----|------|
| bool | 可視ならtrue |

5. 使用例
```rust
if behavior.is_symbol_visible_from_file(&symbol, file_id) {
    ctx.add_symbol(symbol.name.to_string(), symbol.id, ScopeLevel::Global);
}
```

6. エッジケース
- 言語特有のモジュール境界（例: Pythonの__all__、Goのパッケージ公開）に対応するには override 必須。

### LanguageMetadata::from_language

1. 目的と責務
- **ABI-15**の言語メタデータ（abi_version, node_kind_count, field_count）を収集。

2. アルゴリズム
- Language から各値を読み取って構造体へ詰める。

3. 引数

| 名前 | 型 | 説明 |
|------|----|------|
| language | tree_sitter::Language | 対象言語 |

4. 戻り値

| 型 | 説明 |
|----|------|
| LanguageMetadata | メタデータ |

5. 使用例
```rust
let meta = LanguageMetadata::from_language(tree_sitter_rust::LANGUAGE.into());
```

6. エッジケース
- なし（ABI-15 APIが値を提供）。

その他公開メソッド（supports_traits, normalize_caller_name, resolve_import 等）はデフォルト実装が軽量で、言語側での上書きが想定されています。

## Walkthrough & Data Flow

- 入力:
  - DocumentIndex からの**インポート**・**シンボル**データ
  - メモリに保持された**インポート**（LanguageBehaviorの状態）
  - ファイルID（コンテキストのキー）
- 処理:
  1. 解決コンテキスト（ResolutionScope）を作る
  2. インポートを**統合**し、コンテキストへ露出名（alias／末尾／フルパス）単位のバインディングを登録
  3. インポートが**内部**かつ**解決済み**なら、プライマリ名で**Moduleスコープ**にシンボルを追加
  4. 現ファイルの解決可能シンボルを**Moduleスコープ**へ（name と module_path）
  5. 他ファイルの公開シンボルを**Globalスコープ**へ（name と module_path）
  6. 最後に言語固有初期化フック（initialize_resolution_context）を適用
- 出力:
  - Box<dyn ResolutionScope> に登録された解決可能名→SymbolIdのマップ

with_cache のデータフロー差分:
- キャッシュにより**インポートのシンボル候補**を優先し、**module_path**と**import_matches_symbol**で検証後にDBへフォールバック。
- インポート経由で参照される可能性のある**ファイル集合**のみから公開シンボルをロード（get_all_symbols を基本避ける）。

## Complexity & Performance

- build_resolution_context
  - 時間: O(I + Sf + Sg)
    - I: インポート数、Sf: 現ファイルのシンボル数、Sg: グローバルロード（最大10000件）
  - 空間: O(I + Sf + Sg)（コンテキストへの登録数に比例）
  - ボトルネック: get_all_symbols(10000) の大量読み込み、文字列分割・比較の繰り返し
- build_resolution_context_with_cache
  - 時間: O(I + Sf + F·Sp + B)
    - F: インポート元ファイル数、Sp: そのファイルの公開シンボル数、B: fallback件数（<=100）
  - 空間: O(I + Sf + F·Sp)
  - 改善点: 大幅に読み込み対象を絞るためメモリとI/O負荷が削減
- resolve_import_path_with_context
  - 時間: O(c)（同名候補数）＋照合コスト
  - 空間: O(1)

実運用負荷要因:
- 文書数・シンボル数の増大に伴う DB スキャンコスト
- 言語独自の相対インポートルールが複雑な場合、照合コスト増加
- ログ（debug_global!）の標準エラー出力が高頻度で発生すると I/O ノイズ増加

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: このチャンクでは該当なし（Rust安全・unsafe不使用）。
  - 所有権/借用: &selfメソッド中心。戻り値のOption<&str>（resolve_method_trait）はself由来の参照が必要（'self ライフタイムに紐づく）。実装側で静的文字列か self 内データを返す必要あり。
- インジェクション
  - SQL/Command/Path traversal: DB操作は DocumentIndex 経由の抽象。外部入力の文字列はインデックス検索に使用（インジェクションリスクは低）。Path操作は module_path_from_file の言語側実装に依存（このチャンクでは不明）。
- 認証・認可
  - 権限チェック漏れ/セッション固定: 該当なし。
- 秘密情報
  - ハードコード秘密/ログ漏洩: debug_global! はパスやモジュール名などを**標準エラー**に出力するため、過剰ログは情報漏洩の懸念（設定で抑制可能）。
- 並行性
  - Race condition/Deadlock: LanguageBehavior は **Send + Sync**。ただし、add_import や register_file のような**状態管理**を行う場合、内部で Mutex/RwLock を用いないと**データ競合**の可能性。
  - await境界: 非同期は登場せず。
  - キャンセル: 該当なし。

詳細エッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空インポートパス | "" | Noneを返す | resolve_import_path_with_context | 実装済 |
| 末尾区切りで空セグメント | "crate::" | 末尾セグメントが空→露出名に空を含めない | build_resolution_context | 問題の可能性（要修正） |
| 先頭区切り（PHPの"\\Class"） | "\\Class" | 末尾は"Class"で候補検索 | resolve_import_path_with_context | 概ねOK |
| エイリアスと末尾同名 | alias="Bar", path="a::Bar" | 重複露出名は1つに統合 | build_resolution_context | 露出名重複チェックあり |
| 同名シンボルが複数のモジュール | "util::fmt" | module_pathで厳密一致 or 相対一致 | import_matches_symbol | 既定は厳密一致（要override） |
| get_all_symbolsの高負荷 | 大規模プロジェクト | キャッシュ版で読み込みを限定 | build_resolution_context_with_cache | 改善済（fallback100） |
| 可視性未設定 | signature=None | デフォルト可視性は変化なし | configure_symbol | 想定どおり |

## Design & Architecture Suggestions

- get_all_symbols(10000) の使用は**キャッシュ版**へ統一し、読み込み範囲をインポートに基づき**限定**することを推奨。
- import_matches_symbol の既定を**相対一致サポート**（importing_moduleに基づく前置追加）へ強化し、言語側の負担を軽減。
- binding_names の生成時、**空文字列**を除外するガードを追加する（末尾区切りケース対応）。
- resolve_import／resolve_import_path* 系は現在 Option<SymbolId> を返すが、**曖昧性**（複数候補）やエラー原因を伝えるため、返り型を Result<Option<SymbolId>, IndexError> に拡張する検討。
- LanguageBehavior の**状態管理**API（add_import, register_file 等）に対して、**スレッド安全**な内部ストレージ利用をガイド（Mutex/RwLock）するドキュメント強化。
- classify_import_origin の既定は「解決済み=内部」だが、**モノレポ外の外部モジュール**をインデックスに登録したケースへの配慮（例えば module_path の命名規則で外部判定）を追加可能。

## Testing Strategy (Unit/Integration) with Examples

現状テストは「関係の整合性判定」に限定。以下の追加テストを推奨します。

- 単体テスト（Unit）
  - import_matches_symbol 相対一致
  - resolve_import_path_with_context の曖昧候補解決（module_pathの一致／不一致）
  - build_resolution_context の binding_names 生成（alias/末尾/フルパス、空文字除外）
  - is_resolvable_symbol の scope_context ベース判定（ClassMemberのPublic/Private）
  - is_symbol_visible_from_file の言語特有可視性（override例）

- 統合テスト（Integration）
  - cache を利用した build_resolution_context_with_cache の候補検証とフォールバック挙動
  - get_imports_for_file（Tantivy＋メモリ統合）の重複排除
  - classify_import_origin の外部判定拡張（言語側override）

例: 相対インポート一致のテスト（Rust風）
```rust
struct RustBehavior;
impl LanguageBehavior for RustBehavior {
    fn format_module_path(&self, base: &str, symbol_name: &str) -> String {
        format!("{base}::{}", symbol_name)
    }
    fn parse_visibility(&self, sig: &str) -> crate::Visibility {
        if sig.trim_start().starts_with("pub ") { crate::Visibility::Public } else { crate::Visibility::Private }
    }
    fn module_separator(&self) -> &'static str { "::" }
    fn get_language(&self) -> tree_sitter::Language { tree_sitter_rust::LANGUAGE.into() }
    fn import_matches_symbol(
        &self,
        import_path: &str,
        symbol_module_path: &str,
        importing_module: Option<&str>,
    ) -> bool {
        import_path == symbol_module_path ||
        importing_module.map(|m| format!("{m}::{import_path}")).as_deref() == Some(symbol_module_path)
    }
}

#[test]
fn test_relative_import_match() {
    let b = RustBehavior;
    assert!(b.import_matches_symbol("helpers::func", "crate::module::helpers::func", Some("crate::module")));
}
```

例: 空末尾区切りのガード
```rust
#[test]
fn test_binding_names_no_empty_segment() {
    // 擬似import: "crate::"
    // 期待: 空文字の露出名を登録しない（現実装は要修正）
    // このチャンクの現実装では空が入り得るため、修正後のテストとして意図。
}
```

## Refactoring Plan & Best Practices

- 空露出名のガード:
  - build_resolution_context / with_cache で last_segment が空の場合は binding_names に追加しない。
- import_matches_symbol の**相対一致**を既定でサポート:
  - 例: if let Some(m) = importing_module { if format!("{m}{}{}", sep, import_path) == symbol_module_path { return true; } }
- with_cache 内の「インポート元ファイル推定」で、メモリ内インポートだけでなく**Tantivy由来**インポートも考慮して imported_files を拡張。
- 外部スタブ作成のための create_external_symbol を**デフォルト no-op**ではなく**Result::Err だが呼び出し契約を明記**（ドキュメント側強化）。
- ログ: debug_global! のメッセージを**短文化し、キー情報のみに限定**。必要なら log crate に差し替え。

ベストプラクティス:
- 言語側の状態を**Arc<Mutex<_>>**や**DashMap**で保持し、LanguageBehavior が Send + Sync を満たす安全な内部構造にする。
- **単一責務**の原則に従い、解決規則（import_matches_symbol）・可視性・モジュールパスの各関数を過度に複合化しない。
- **境界条件テスト**（空文字、区切り末尾、先頭区切り、別名重複）を用意。

## Observability (Logging, Metrics, Tracing)

- ロギング:
  - debug_global! による**詳細ログ**が存在。大量プロジェクトでは I/O 増加となるため、設定での制御推奨。
  - 重要キーポイント（キャッシュヒット/ミス、DBフォールバック、登録件数）に限定すると良い。
- メトリクス:
  - コンテキスト構築時間、インポート数、解決成功率、フォールバック発生率、追加シンボル総数（Module/Global別）を計測。
- トレーシング:
  - build_resolution_context（with_cache含む）全体に**スパン**を設定し、各サブステップ（インポート統合、ローカル追加、グローバル追加）にタグ付け。

## Risks & Unknowns

- Unknown:
  - DocumentIndex の内部実装（検索のインデックス戦略、整合性保証）、ResolutionScope の具体的なデータ構造、Import の完全仕様（相対・別名・ワイルドカード等）。
  - ParserFactory／LanguageParser との結合部（このチャンクには現れない）。
- Risks:
  - get_all_symbols(10000) による**スケール問題**（メモリ/時間）。
  - 言語側の**状態管理**が非同期・並列インデクス実行時に**レース**するリスク（Send + Sync要件に注意）。
  - import_matches_symbol の既定が**厳密一致**のみのため、相対インポート多用の言語では不解決が多発。
  - 末尾区切りの**空露出名**で context.add_symbol("") が登録されうる不具合。
  - debug_global! のエラーストリーム出力が**ノイズ**や情報漏洩の懸念（設定で緩和可能）。

以上の観点から、**公開API**と**コアロジック**は十分に拡張可能であり、状態管理・相対インポート・高負荷読み込み周辺の改善で安定性と性能が大きく向上します。