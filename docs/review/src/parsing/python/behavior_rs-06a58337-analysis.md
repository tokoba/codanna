# parsing\python\behavior.rs Review

## TL;DR

- 目的: Tree-sitterベースのPython解析向けに、言語固有の挙動を提供する実装。インポート解決、モジュールパス生成、可視性判定、外部シンボルの作成、解決コンテキスト構築が中核。
- 主要公開API: PythonBehavior::new、LanguageBehaviorトレイトの各メソッド（format_module_path、parse_visibility、module_path_from_file、resolve_import_path_with_context、build_resolution_context、create_external_symbol など）。
- 複雑箇所: インポート解決（相対/部分一致/別名/グロブ）、解決コンテキスト構築（インポート/同一ファイル/同パッケージの可視シンボル集約）、外部シンボル生成（ストレージとの整合）。
- Rust安全性: unsafe不使用。BehaviorStateの内部可変性が推測され、Send/Sync・データ競合の安全性はこのチャンクでは不明。
- エラー設計: IndexError(TantivyError)への変換とIndexResultの利用でI/O境界の明確化。文字列ベースのparse_visibilityは誤判定の余地。
- 重大リスク: パッケージ境界を越える相対インポートの誤解決、get_all_symbols(5000)のスケール限界、外部シンボル種別の固定（Class）による解析不一致、BehaviorStateの並行使用安全性の不明。
- テスト: 可視性/モジュールパス/ノード種別/基本機能の単体テストあり。相対インポート・別名・グロブ・外部解決の網羅は不足。

## Overview & Purpose

このファイルは、Pythonの言語固有の振る舞いを提供する構造体PythonBehaviorを定義し、LanguageBehaviorトレイトを実装します。目的は以下の通りです。

- Tree-sitter Python言語オブジェクトの提供と、シンボル構成（名前・モジュールパス・可視性）の規約化。
- Pythonのインポート解決（相対インポート「.」「..」、別名 import as、グロブ import *、部分一致）を、Index（DocumentIndex）および収集済みインポート情報（BehaviorState）に基づいて行う。
- ファイルパスからPythonモジュールパスを生成（__init__.py、__main__.py、src/lib/appフォルダ除去、拡張子.py/.pyx/.pyi対応）。
- 解決スコープ（ResolutionScope）を構築して、名前解決に必要なシンボル集合（インポート・同一ファイル・同パッケージ）を用意。
- 外部パッケージ由来のシンボルを、仮想ファイル（.codanna/external/*.pyi）としてDocumentIndexに格納・再利用。

この振る舞いにより、Pythonコード解析時の名前解決と参照の一貫性が担保されます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | PythonBehavior | pub | Python言語向けの振る舞い実装。Tree-sitter言語オブジェクト保持、状態管理、トレイト実装 | Med |
| Field | language: Language | private | tree_sitter_python::LANGUAGEの保持 | Low |
| Field | state: BehaviorState | private | ファイル登録・インポート追跡・モジュールパス管理 | Med |
| Trait Impl | StatefulBehavior for PythonBehavior | crate | BehaviorStateへのアクセサと状態連携 | Low |
| Trait Impl | LanguageBehavior for PythonBehavior | crate | シンボル構成、インポート解決、可視性、モジュール分離子、解決コンテキスト構築など | High |
| Function | new() -> Self | pub | PythonBehaviorの初期化 | Low |
| Function | default() -> Self | pub | new()委譲 | Low |

### Dependencies & Interactions

- 内部依存
  - BehaviorState（インポート・ファイル登録・モジュールパス管理）: register_file_with_state, add_import_with_state, get_imports_from_state, get_module_path（関数名: 行番号=不明）
  - crate::parsing::{LanguageBehavior, ResolutionScope, InheritanceResolver}
  - crate::storage::DocumentIndex（find/index/store各種メソッド）
  - crate::{FileId, SymbolId, Visibility, Symbol, SymbolKind, Range, IndexError, IndexResult}
  - crate::symbol::ScopeContext
  - crate::parsing::python::{PythonResolutionContext, PythonInheritanceResolver}
  - crate::config::is_global_debug_enabled
  - crate::indexing::get_utc_timestamp
- 外部依存（クレート）
  - tree_sitter::Language
  - tree_sitter_python::LANGUAGE
- 被依存推定
  - 解析・インデクサのPython言語サポート部分（モジュール登録、インポート収集、呼び出し解決、継承解析）。
  - クエリエンジン（呼び出し元正規化、外部呼び出し解決）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | fn new() -> Self | PythonBehaviorの生成 | O(1) | O(1) |
| default | fn default() -> Self | new委譲 | O(1) | O(1) |
| configure_symbol | fn configure_symbol(&self, symbol: &mut Symbol, module_path: Option<&str>) | シンボル構成（モジュールパス、表示名、可視性） | O(len) | O(1) |
| create_resolution_context | fn create_resolution_context(&self, file_id: FileId) -> Box<dyn ResolutionScope> | Python専用解決コンテキストの生成 | O(1) | O(1) |
| create_inheritance_resolver | fn create_inheritance_resolver(&self) -> Box<dyn InheritanceResolver> | Python継承解析器生成 | O(1) | O(1) |
| format_module_path | fn format_module_path(&self, base_path: &str, _symbol_name: &str) -> String | モジュールパス整形（Pythonはsymbol名を含めない） | O(len) | O(len) |
| parse_visibility | fn parse_visibility(&self, signature: &str) -> Visibility | 命名規約による可視性推定 | O(len) | O(1) |
| module_separator | fn module_separator(&self) -> &'static str | モジュール区切り文字（"."） | O(1) | O(1) |
| supports_traits | fn supports_traits(&self) -> bool | トレイト非対応判定 | O(1) | O(1) |
| supports_inherent_methods | fn supports_inherent_methods(&self) -> bool | 固有メソッド非対応判定 | O(1) | O(1) |
| get_language | fn get_language(&self) -> Language | Tree-sitter言語取得 | O(1) | O(1) |
| normalize_caller_name | fn normalize_caller_name(&self, name: &str, file_id: FileId) -> String | "<module>"などの呼び出し元名正規化 | O(len) | O(len) |
| resolve_external_call_target | fn resolve_external_call_target(&self, to_name: &str, from_file: FileId) -> Option<(String, String)> | 未解決呼び出し先のモジュール推定（インポート基づき） | O(n) | O(1) |
| create_external_symbol | fn create_external_symbol(&self, document_index: &mut DocumentIndex, module_path: &str, symbol_name: &str, language_id: LanguageId) -> IndexResult<SymbolId> | 外部シンボル（仮想pyi）の作成・インデックス | O(logN + 1) | O(1) |
| module_path_from_file | fn module_path_from_file(&self, file_path: &Path, project_root: &Path) -> Option<String> | ファイルパスからPythonモジュールパス生成 | O(len) | O(len) |
| register_file | fn register_file(&self, path: PathBuf, file_id: FileId, module_path: String) | 状態へのファイル登録（Stateful） | O(1) | O(1) |
| add_import | fn add_import(&self, import: crate::parsing::Import) | 状態へのインポート登録（Stateful） | O(1) | O(1) |
| get_imports_for_file | fn get_imports_for_file(&self, file_id: FileId) -> Vec<crate::parsing::Import> | ファイルのインポート取得（Stateful） | O(k) | O(k) |
| get_module_path_for_file | fn get_module_path_for_file(&self, file_id: FileId) -> Option<String> | ファイルのモジュールパス取得（Stateful） | O(1) | O(1) |
| import_matches_symbol | fn import_matches_symbol(&self, import_path: &str, symbol_module_path: &str, importing_module: Option<&str>) -> bool | Python流の一致判定（相対・部分一致） | O(len) | O(1) |
| resolve_import_path_with_context | fn resolve_import_path_with_context(&self, import_path: &str, importing_module: Option<&str>, document_index: &DocumentIndex) -> Option<SymbolId> | インポートパスからシンボルID解決（文脈使用） | O(candidates) | O(1) |
| build_resolution_context | fn build_resolution_context(&self, file_id: FileId, document_index: &DocumentIndex) -> IndexResult<Box<dyn ResolutionScope>> | 名前解決スコープの構築（インポート/ファイル内/パッケージ） | O(I + F + S) | O(U) |
| is_resolvable_symbol | fn is_resolvable_symbol(&self, symbol: &crate::Symbol) -> bool | スコープと種別に基づく解決対象判定 | O(1) | O(1) |
| resolve_import | fn resolve_import(&self, import: &crate::parsing::Import, document_index: &DocumentIndex) -> Option<SymbolId> | Pythonインポート解決（総合） | O(candidates) | O(1) |
| is_symbol_visible_from_file | fn is_symbol_visible_from_file(&self, symbol: &crate::Symbol, from_file: FileId) -> bool | 命名規約による可視性判定（ファイル外） | O(1) | O(1) |

I: インポート数、F: ファイル内シンボル数、S: 取得した全シンボルの上限（5000）、U: コンテキスト内ユニーク名数、candidates: 名前検索で返る候補数。

以下、主要APIの詳細。

### configure_symbol

1. 目的と責務
   - シンボルのmodule_pathを設定し、Pythonモジュールシンボルの場合は表示名を最終セグメントへ短縮。signatureから可視性を推定。

2. アルゴリズム
   - module_pathがSomeの場合、format_module_pathで整形し、シンボルへ設定。
   - kindがModuleならpathの末尾をnameにする。module_pathがNoneかつModuleならname="module"にする。
   - signatureがある場合、parse_visibilityでVisibility設定。

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | symbol | &mut Symbol | 対象シンボル（name, kind, signatureなど利用） |
   | module_path | Option<&str> | 基底モジュールパス |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | () | 変更はsymbolへ反映 |

5. 使用例
   ```rust
   let behavior = PythonBehavior::new();
   let mut sym = Symbol::new(sym_id, "my_func".into(), SymbolKind::Function, file_id, Range::new(0,0,0,0));
   sym.signature = Some("def my_func(a):".into());
   behavior.configure_symbol(&mut sym, Some("pkg.module"));
   assert_eq!(sym.module_path.as_deref(), Some("pkg.module"));
   assert_eq!(sym.visibility, Visibility::Public);
   ```

6. エッジケース
   - module_pathがNoneかつModule: nameが"module"へ設定。
   - signatureがdocstring風の文字列でもcontains判定で影響を受ける可能性（誤判定注意）。

根拠: configure_symbol（関数名: 行番号=不明）。

### parse_visibility

1. 目的と責務
   - 文字列signatureに含まれる命名規約からVisibilityを推定。

2. アルゴリズム
   - dunderメソッド群（__init__, __str__, __repr__, __eq__, __hash__, __call__）をPublic。
   - "def __" または "class __" をPrivate。
   - "def _" または "class _" をModule（保護/モジュールレベル）。
   - その他はPublic。

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | signature | &str | 宣言シグネチャ文字列 |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Visibility | 推定結果 |

5. 使用例
   ```rust
   let behavior = PythonBehavior::new();
   assert_eq!(behavior.parse_visibility("def _helper():"), Visibility::Module);
   assert_eq!(behavior.parse_visibility("def __private():"), Visibility::Private);
   assert_eq!(behavior.parse_visibility("def __init__(self):"), Visibility::Public);
   ```

6. エッジケース
   - コメントや文字列中のパターンに反応する可能性。
   - "class __Name" はPythonの名前マングリング（Private）だが、メソッド以外の特殊名はPublicにするべきパターンとの整合性要確認。

根拠: parse_visibility（関数名: 行番号=不明）。

### module_path_from_file

1. 目的と責務
   - プロジェクトルートからの相対パスをPythonのモジュールパス（.区切り）へ変換。

2. アルゴリズム
   - project_rootからの相対化、src/lib/appプリフィクス除去。
   - 拡張子.py/.pyx/.pyi除去。
   - "__init__.py" はパッケージ名（末尾"/__init__"除去）。
   - "__main__.py" または "main" は "__main__" へ。
   - "/" を "." に置換、空や"__init__"はNone。

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | file_path | &Path | 対象ファイル |
   | project_root | &Path | プロジェクトルート |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Option<String> | モジュールパス。ルート__init__.pyや空はNone |

5. 使用例
   ```rust
   let behavior = PythonBehavior::new();
   let root = Path::new("/project");
   let p = Path::new("/project/src/pkg/mod.py");
   assert_eq!(behavior.module_path_from_file(p, root), Some("pkg.mod".to_string()));
   ```

6. エッジケース
   - project_rootとの関係が不正（strip_prefix失敗）はNone。
   - 拡張子が想定外（.py/.pyx/.pyi以外）はそのまま扱う。

根拠: module_path_from_file（関数名: 行番号=不明）。

### import_matches_symbol

1. 目的と責務
   - インポートパスとシンボルのmodule_pathの一致可否をPython規則で判定。

2. アルゴリズム
   - 完全一致を最優先。
   - 相対インポート（先頭"."）はresolve_python_relative_importでfromモジュールに基づき解決して比較。
   - 絶対インポートの部分一致:
     - "."を含まない場合はsymbol_module_pathが".{import_path}"で終わるなら一致。
     - "."を含む場合は末尾一致。

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | import_path | &str | インポートパス（.や..対応） |
   | symbol_module_path | &str | シンボルのモジュールパス |
   | importing_module | Option<&str> | 起点モジュール（相対時使用） |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | bool | 一致ならtrue |

5. 使用例
   ```rust
   let behavior = PythonBehavior::new();
   assert!(behavior.import_matches_symbol("pkg.mod", "pkg.mod", Some("pkg.mod")));
   assert!(behavior.import_matches_symbol(".sibling", "pkg.sibling", Some("pkg.child")));
   assert!(behavior.import_matches_symbol("mod", "pkg.mod", Some("pkg.child")));
   ```

6. エッジケース
   - 相対インポートがモジュール深さを超える（"..."）場合の扱い。
   - importing_moduleがNoneの場合は部分一致のみ適用される。

根拠: import_matches_symbol（関数名: 行番号=不明）。

### resolve_import_path_with_context

1. 目的と責務
   - インポートパスをIndex上のシンボルIDへ解決（Python規則で一致判定）。

2. アルゴリズム
   - "."区切りで分割し末尾をsymbol名として検索（DocumentIndex::find_symbols_by_name）。
   - 候補ごとにmodule_pathを取り出し、import_matches_symbolで一致チェック。

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | import_path | &str | インポートパス |
   | importing_module | Option<&str> | 起点モジュール（相対時） |
   | document_index | &DocumentIndex | 参照用インデックス |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Option<SymbolId> | 解決に成功したID |

5. 使用例
   ```rust
   let opt_id = behavior.resolve_import_path_with_context("pkg.mod.Name", Some("pkg.user"), &doc_index);
   if let Some(sym_id) = opt_id {
       // 参照成功
   }
   ```

6. エッジケース
   - segmentsが空はNone。
   - 複数候補がある場合は最初の一致を返す（最適一致ではない）。

根拠: resolve_import_path_with_context（関数名: 行番号=不明）。

### build_resolution_context

1. 目的と責務
   - 1ファイルの名前解決に必要なスコープ（import、同一ファイル、同パッケージの可視シンボル）を集約するResolutionScopeの構築。

2. アルゴリズム
   - PythonResolutionContextを作成。
   - get_imports_for_fileで取得した各インポートをresolve_importし、別名alias優先でnameを決定し、Packageスコープに追加。
   - find_symbols_by_fileで同一ファイルのシンボルを取得。is_resolvable_symbolがtrueならscope_contextに応じてModule/Global/Localに追加。
   - get_all_symbols(5000)で他ファイルのシンボルを一定数取得。is_symbol_visible_from_fileかつVisibility::PublicならGlobalとして追加。

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | file_id | FileId | 対象ファイル |
   | document_index | &DocumentIndex | 検索対象のインデックス |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | IndexResult<Box<dyn ResolutionScope>> | 構築された解決コンテキスト |

5. 使用例
   ```rust
   let ctx = behavior.build_resolution_context(file_id, &doc_index)?.into();
   // ctxを用いて名前解決を実施
   ```

6. エッジケース
   - resolve_importがNoneの場合、インポート名は追加されない。
   - get_all_symbolsの上限（5000）により見逃し発生の可能性。

根拠: build_resolution_context（関数名: 行番号=不明）。

Mermaidフローチャート（上記の図は`build_resolution_context`関数の主要分岐を示す。行番号: 不明）

```mermaid
flowchart TD
    A[Start: new PythonResolutionContext(file_id)] --> B[Imports = get_imports_for_file(file_id)]
    B --> C{for each import}
    C -->|resolve_import OK| D[alias or name決定]
    D --> E[context.add_symbol(name, id, ScopeLevel::Package)]
    C -->|resolve_import None| F[skip]
    E --> G[FileSymbols = find_symbols_by_file(file_id)]
    F --> G
    G --> H{for each symbol}
    H -->|is_resolvable_symbol| I[scope_level判定(Module/Global/Local)]
    I --> J[context.add_symbol(symbol.name, id, scope_level)]
    H -->|else| K[skip]
    J --> L[AllSymbols = get_all_symbols(5000)]
    K --> L
    L --> M{for each s in AllSymbols}
    M -->|s.file_id != file_id && is_symbol_visible_from_file(s, file_id) && s.visibility == Public| N[context.add_symbol(s.name, s.id, Global)]
    M -->|else| O[skip]
    N --> P[End: Box<ResolutionScope>]
    O --> P
```

### create_external_symbol

1. 目的と責務
   - 外部（プロジェクト外/未登録）シンボルを仮想ファイル(.pyi)に紐づけてDocumentIndexへ登録し、再利用可能にする。

2. アルゴリズム
   - document_index.find_symbols_by_nameで既存シンボルを再利用可能か確認（module_path一致）。
   - path_str=".codanna/external/{module_path.replace('.', '/')}.pyi"を生成し、file_infoが無ければ新規登録（get_next_file_id, store_file_info）。
   - get_next_symbol_idで新規ID割当、SymbolKind::ClassでSymbol生成しPublic可視性、module_path, ScopeContext::Global, language_id設定。
   - index_symbolで登録。最後にSymbolCounterメタデータ更新。

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | document_index | &mut DocumentIndex | インデックスへの挿入 |
   | module_path | &str | 対象モジュールパス |
   | symbol_name | &str | シンボル名 |
   | language_id | LanguageId | 言語ID |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | IndexResult<SymbolId> | 成功時は新規または既存のID |

5. 使用例
   ```rust
   let sym_id = behavior.create_external_symbol(&mut doc_index, "requests", "Session", language_id)?;
   ```

6. エッジケース
   - 既存シンボルにmodule_pathがNoneの場合は再利用されない。
   - ファイル・シンボルID枯渇（FileIdExhausted, SymbolIdExhausted）がエラーに。

根拠: create_external_symbol（関数名: 行番号=不明）。

### resolve_external_call_target

1. 目的と責務
   - 未解決呼び出し（to_name）について、インポート情報から推定したモジュールパスを返す。

2. アルゴリズム
   - get_imports_for_file(from_file)を走査。
   - import.alias==to_nameなら imp.path の末尾を除いたmodule_pathで返す。
   - imp.pathが".{to_name}"で終わればmodule_pathに分割して返す。
   - imp.is_globなら imp.path をmodule_pathとして返す。

3. 引数
   | 引数 | 型 | 説明 |
   |------|----|------|
   | to_name | &str | 呼び出し先名（未解決） |
   | from_file | FileId | 呼び出し元ファイル |

4. 戻り値
   | 型 | 説明 |
   |----|------|
   | Option<(String, String)> | (推定module_path, シンボル名=to_name) |

5. 使用例
   ```rust
   if let Some((mp, name)) = behavior.resolve_external_call_target("np", file_id) {
       assert_eq!(name, "np");
       // mp例: "numpy"（from numpy import as np など仮）
   }
   ```

6. エッジケース
   - 別名/グロブが複数ある場合は先勝。
   - "import pkg" の形は未対応（シンボル名と一致しないため推定不可）。

根拠: resolve_external_call_target（関数名: 行番号=不明）。

## Walkthrough & Data Flow

- シンボル構成（configure_symbol）
  - 入力: Symbol, module_path（Option）
  - 処理: module_path整形・設定、Module種別ならname短縮、signatureによりvisibility設定。
  - 出力: 修正済みSymbol。

- インポート解決（resolve_import → resolve_import_path_with_context → import_matches_symbol）
  - 入力: Import（path, alias, file_id）、document_index。
  - 処理:
    - file_idからimporting_moduleを取得。
    - import_pathを末尾シンボル名で検索、候補のmodule_pathと一致判定。
    - 相対インポートはfrom_moduleに基づきresolve_python_relative_importで補正。
  - 出力: Option<SymbolId>。

- 解決コンテキスト構築（build_resolution_context）
  - 入力: file_id, document_index。
  - 処理:
    - 収集済インポートを解決しPackageスコープに登録（alias優先）。
    - 同一ファイルの解決可能シンボルをscopeに応じて追加。
    - 全シンボル（上限5000）から、他ファイルかつ可視性PublicのものをGlobalに追加。
  - 出力: ResolutionScope。

- 外部シンボル生成（create_external_symbol）
  - 入力: module_path, symbol_name, language_id, document_index（可変）。
  - 処理: 既存再利用→なければ仮想pyiファイル・ID割当→Symbol作成→index→メタ更新。
  - 出力: SymbolId。

## Complexity & Performance

- parse_visibility: O(len(signature))、単純なcontains判定。誤判定リスクはあるが高速。
- import_matches_symbol: O(len(path))、部分一致と相対解決。軽量。
- resolve_import_path_with_context: O(C)（C=find_symbols_by_nameの候補数）、候補比較がボトルネック。インデックス検索自体は高速だが候補が多いと遅延。
- build_resolution_context:
  - get_imports_for_file: O(I)
  - find_symbols_by_file: O(F)
  - get_all_symbols(5000): O(5000) + 判定・追加。スケール時のボトルネック。パッケージフィルタ無しのため不要な走査が増える。
- create_external_symbol: 既存検索（名前のみ）→一致チェックはO(C)。file/symbol ID割当・インデックスはO(1〜logN)程度。I/O（Tantivy）コストあり。

スケール限界:
- 大規模プロジェクトでget_all_symbols(5000)の固定上限は、不足（見逃し）または過剰（不必要な走査）を招く。パッケージ・モジュールパスのプレフィクスで絞るべき。
- import解決の候補探索は名前のみで行うため、一般名（"load", "run", "init"）で候補が爆発しうる。

実運用負荷要因:
- DocumentIndexの検索・書込（Tantivy）に依存。バッチ処理時のI/O待ち、コンカレンシー時のロック/競合が潜在的に影響（このチャンクでは不明）。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 相対インポートの深すぎる階層 | import_path="...sibling", from_module="a.b" | ルートへ達したらそれ以上上がらない/安全に処理 | resolve_python_relative_importはparts.pop()を深さ分実行 | 要確認（過剰ドットでルート過ぎの挙動不明） |
| 末尾にドットがある | import_path="pkg.module." | 無視または不一致 | trim_start_matches('.')のみで末尾ドット未処理 | 要修正 |
| from import * の解決 | imp.is_glob=true | 任意to_nameを同モジュール由来として扱う | resolve_external_call_targetはmodule_path返す | OK |
| __init__.py ルート | "/project/__init__.py" | None（ルートパッケージ） | module_path_from_fileはNone返す | OK |
| __main__.py | "/project/__main__.py" | "__main__" | 実装済 | OK |
| dunder以外の"__"を含むclass名 | "class __PrivateClass:" | Private | parse_visibilityはPrivateへ | OK |
| コメントに"__init__" | "def foo(): # __init__" | 可視性はPublicのまま | contains判定でPublic化される可能性 | 要修正（誤判定） |
| 異常なモジュールパス | module_path="pkg../../evil" | 安全なファイルパス生成 | '.'→'/'変換で相対が発生しない | OK（ただし入力の検証不在） |

セキュリティチェックリスト

- メモリ安全性
  - Buffer overflow: RustのString/Vec使用のみでunsafeなし。問題なし。
  - Use-after-free: 所有権管理により防止。問題なし。
  - Integer overflow: IDをu64へ格納、通常範囲。問題なし。
- インジェクション
  - SQL: Tantivyベースの検索でSQL不使用。問題なし。
  - Command: 外部コマンド未使用。問題なし。
  - Path traversal: ".codanna/external/" + module_path.replace('.', '/')で生成。相対 ".." を含むmodule_pathが入力される可能性は理論上あるが、通常は解析生成値。入力検証はなく、念のため正規化推奨。
- 認証・認可
  - 機構なし。問題なし（コンポーネント範囲外）。
- 秘密情報
  - ハードコード秘密なし。ログ漏洩もない（DEBUG時にeprintlnするのみ）。
- 並行性
  - BehaviorStateへの内部可変アクセスが推測される（&selfで登録関数）。Send/Syncやデータ競合の安全性はこのチャンクでは不明。多スレッド解析時のレースコンディションは検討必要。

Rust特有の観点

- 所有権
  - String/PathBuf/Rangeなどはmove/cloneで安全に扱われる。Index操作は&mut DocumentIndexで明示的排他。
- 借用
  - &selfで状態操作（register_file, add_importなど）。内部可変性（RefCell/Mutex等）の存在が推測されるが詳細は不明。
- ライフタイム
  - 明示的ライフタイムパラメータ不要。トレイトオブジェクトはBoxで所有。
- unsafe境界
  - unsafe不使用。
- 並行性・非同期
  - Send/Sync境界は不明。awaitは存在しない同期実装。
  - 共有状態の保護方法（Mutex/RwLock）は本チャンクに現れない。
- エラー設計
  - IndexResult/IndexErrorでI/Oエラーを包む。unwrap/expect不使用。From/Intoの変換詳細は不明。

## Design & Architecture Suggestions

- import解決の強化
  - get_all_symbols(5000)の広範走査を避け、パッケージプレフィクスでの絞り込み検索（module_pathプレフィクス条件）をDocumentIndexに追加する。
  - resolve_import_path_with_contextで複数候補のスコアリング（完全一致>相対解決一致>末尾一致>単語一致）を導入。
- 相対インポートの堅牢化
  - resolve_python_relative_importで過剰ドットの扱い（ルート超過時は停止）を明示。パッケージ境界（__init__.py存在）を考慮した上でルート越えを防止。
- 可視性判定の精度向上
  - parse_visibilityは文字列contains依存からASTベース（ノード種別・識別子名）へ。コメントや文字列に影響されないようにする。
- 外部シンボル種別の柔軟化
  - create_external_symbolが常にClass。関数・モジュール・属性など利用状況に応じたSymbolKindを選べる拡張を検討。
- 解決コンテキストの重複排除
  - context.add_symbol時に重複登録の抑止（同名多重登録による曖昧性軽減）。
- ロギングの体系化
  - eprintlnのDEBUGを構造化ログ（レベル・カテゴリ・モジュール）へ。import解決のトレースが可能に。

## Testing Strategy (Unit/Integration) with Examples

既存テストは基本機能を網羅。以下を追加推奨。

- 相対インポート境界
  ```rust
  #[test]
  fn test_resolve_relative_import_boundary() {
      let behavior = PythonBehavior::new();
      // from a.b import ...
      assert_eq!(
          behavior.resolve_python_relative_import("...x", "a.b"),
          "x" // ルート超過はルート扱い（期待仕様要定義）
      );
  }
  ```
- 別名・グロブの外部呼び出し推定
  ```rust
  #[test]
  fn test_resolve_external_call_target_alias_and_glob() {
      let behavior = PythonBehavior::new();
      // 状態にimport: from numpy import array as arr
      behavior.add_import(crate::parsing::Import {
          path: "numpy.array".into(), alias: Some("arr".into()), file_id: FileId::new(1).unwrap(), is_glob: false
      });
      let r = behavior.resolve_external_call_target("arr", FileId::new(1).unwrap());
      assert_eq!(r, Some(("numpy".into(), "arr".into())));
      // glob
      behavior.add_import(crate::parsing::Import {
          path: "math".into(), alias: None, file_id: FileId::new(1).unwrap(), is_glob: true
      });
      let r2 = behavior.resolve_external_call_target("sqrt", FileId::new(1).unwrap());
      assert_eq!(r2, Some(("math".into(), "sqrt".into())));
  }
  ```
- import_matches_symbolの部分一致
  ```rust
  #[test]
  fn test_import_matches_symbol_suffix() {
      let behavior = PythonBehavior::new();
      assert!(behavior.import_matches_symbol("module", "pkg.module", Some("pkg.other")));
      assert!(behavior.import_matches_symbol("pkg.module.Name", "root.pkg.module.Name", Some("root.pkg.user")));
  }
  ```
- build_resolution_contextの重複と可視性
  ```rust
  #[test]
  fn test_build_resolution_context_visibility_and_duplicates() {
      // DocumentIndexをモックし、Public/Private/_nameなどを混在させて登録
      // 期待: Privateは他ファイルから追加されない、_nameは追加される（現仕様）、重複名の扱い確認
  }
  ```
- module_path_from_fileの拡張子とディレクトリ除去
  ```rust
  #[test]
  fn test_module_path_from_file_variants() {
      let behavior = PythonBehavior::new();
      let root = Path::new("/p");
      assert_eq!(behavior.module_path_from_file(Path::new("/p/lib/a.pyx"), root), Some("a".into()));
      assert_eq!(behavior.module_path_from_file(Path::new("/p/app/a/b.pyi"), root), Some("a.b".into()));
  }
  ```

注: validate_node_kindは本チャンクに実装が現れないため、トレイト側の既定実装に依存（関数名: 行番号=不明）。

## Refactoring Plan & Best Practices

- 機能分割
  - import解決ロジック（相対/部分一致/外部推定）を専用モジュールへ分離し、ユニットテスト容易化。
- エラーハンドリングの明確化
  - create_external_symbolの既存再利用条件にmodule_path==Noneの扱い（スキップ）を明示しログ追加。
- API整合性
  - is_symbol_visible_from_fileとparse_visibilityの規約整合。Private/Moduleの扱いを共通化。
- パフォーマンス
  - build_resolution_contextの他ファイル探索は、module_pathプレフィックスでフィルタするIndexAPIを追加。
- 安全性
  - BehaviorStateのSend/Sync保証をドキュメント化。必要ならRwLockで保護し、&self APIを並行安全に。

## Observability (Logging, Metrics, Tracing)

- 現状はDEBUGフラグ時のeprintlnのみ。改善提案:
  - ログ: import解決試行（入力、候補数、選択結果）、外部シンボル作成（既存再利用/新規、ID）。
  - メトリクス: 1ファイルあたりの解決コンテキスト構築時間、候補比較件数、get_all_symbols呼び出し回数。
  - トレーシング: build_resolution_contextの各ステップをspanで計測。resolve_importでの一致判定パス（完全/相対/末尾/失敗）。

## Risks & Unknowns

- BehaviorStateの内部実装が不明（RefCell/Mutex/RwLockなど）。並行安全性・性能への影響は未確定。
- DocumentIndexのget_all_symbols(5000)の仕様（順序・フィルタ）が不明。コンテキストの網羅性と効率に影響。
- validate_node_kindの提供元・実装詳細がこのチャンクには現れない。ノード種別の妥当性チェックの正確性は外部に依存。
- create_external_symbolでSymbolKind::Class固定は解析の正確性に影響する可能性（関数・モジュールの外部呼び出し対応）。
- module_pathの入力ソース（他コンポーネントでの生成）が不明。異常値への耐性は限定的。

以上、本ファイルはPython解析の中核であり、インポートとスコープ解決を的確に扱う一方、精度・スケール・並行性の観点で改善余地が存在します。