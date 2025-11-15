# parsers\gdscript\test_behavior_api.rs Review

## TL;DR

- 目的: GDScript用のBehavior実装（GdscriptBehavior）のAPI契約をテスト駆動で定義し、PythonBehavior同等の機能を満たすことを検証する。
- 主要公開API（期待契約）: format_module_path, parse_visibility, module_separator, supports_traits, supports_inherent_methods, validate_node_kind, module_path_from_file, register_file, get_module_path_for_file, add_import, get_imports_for_file, import_matches_symbol, is_resolvable_symbol, is_symbol_visible_from_file, create_resolution_context, create_inheritance_resolver。
- 複雑箇所: import_matches_symbolのパス正規化（res://の有無、拡張子.gd、相対パス解決）、可視性判定（同一ファイル/他ファイル・先頭アンダースコア）。
- 重大リスク: パス解決の曖昧性（相対→基準モジュールの扱い）、可視性の慣習と仕様の差異（_readyなどGodotライフサイクル）、State管理（ファイル登録・import蓄積）のスレッド安全性。
- セキュリティ: パス処理におけるパストラバーサル、情報漏えい（ログや可視性誤判定）に注意。メモリ安全性はRust標準の範囲で安全だが、外部実装に依存。
- パフォーマンス: importの取得・登録は基本O(k)（k=ファイル単位のimport数）。パス正規化はO(n)（文字列長）。
- 未実装/不明: 実装本体はこのチャンクには現れない。トレイト名・具体署名は不明だが、テストから推定可能。

## Overview & Purpose

このファイルは、codanna::parsing::gdscriptにあるGdscriptBehaviorのふるまいを定義するテスト群であり、PythonBehaviorに近い言語特性を持つGDScriptに合わせたAPIの契約を明確化するもの。テストはTDDスタイルで段階的に整理されており、以下の層に分かれる:

- Tier 1（Basic API）: 基本的なモジュールパス整形、可視性解析、ノード種別検証などのコアメソッド。
- Tier 3（Stateful API）: ファイル登録、import追跡などの状態管理。
- Tier 2（Resolution API）: importとシンボルの一致判定、可視性・解決可能性の確認など、解決支援。
- GDScript固有: res://のパス、extends、class_name、preloadの扱い。

このテストが通るようにGdscriptBehaviorを実装することで、ビルド・解析・解決の一連のフローにおける一貫した契約が得られる。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_format_module_path | test | モジュールパス整形の期待動作を検証 | Low |
| Function | test_parse_visibility | test | 可視性（Public/Private）の判定規約を検証 | Low |
| Function | test_module_separator | test | モジュール区切り文字（"/"）を検証 | Low |
| Function | test_supports_features | test | 言語機能のサポート可否（traits, inherent methods）を検証 | Low |
| Function | test_validate_node_kinds | test | パーサー対応ノード種別の妥当性検証 | Low |
| Function | test_module_path_from_file | test | ファイルパスからres://モジュールパスを導出 | Low |
| Function | test_has_behavior_state | test | 状態管理の存在確認（import取得が空で動作） | Low |
| Function | test_register_file | test | ファイル登録とモジュールパス取得の整合性 | Low |
| Function | test_add_import | test | import追跡の追加と取得 | Low |
| Function | test_get_imports_for_file_empty | test | import未追加時の空結果確認 | Low |
| Function | test_multiple_imports_same_file | test | 同一ファイル複数importの集約 | Low |
| Function | test_imports_isolated_by_file | test | ファイルごとのimport隔離性 | Low |
| Function | test_import_matches_symbol_exact | test | importとシンボルの完全一致判定 | Low |
| Function | test_import_matches_symbol_with_extension | test | .gd拡張子の有無に関する一致判定 | Low |
| Function | test_import_matches_symbol_without_res_prefix | test | res://プレフィクスの有無に関する一致判定 | Low |
| Function | test_import_matches_symbol_relative_paths | test | 相対パス（./, ../）のコンテキスト解決 | Med |
| Function | test_is_resolvable_symbol | test | シンボル種別・スコープに基づく解決可能性 | Low |
| Function | test_is_symbol_visible_from_file | test | 可視性（同一ファイル・他ファイル、アンダースコア） | Low |
| Function | test_gdscript_class_name_import | test | class_nameによるグローバル可視性（is_glob） | Low |
| Function | test_gdscript_extends_import | test | extends句のimport扱い | Low |
| Function | test_gdscript_preload_import | test | preload("res://...")のimport追跡 | Low |
| Function | test_create_resolution_context | test | 解決コンテキストの生成とダウンキャスト確認 | Med |
| Function | test_create_inheritance_resolver | test | 継承解決器の生成 | Low |
| Function | test_parser_tracks_extends | test (ignore) | パーサ統合後、extendsのimport追跡 | Low |
| Function | test_parser_tracks_class_name | test (ignore) | パーサ統合後、class_nameのグローバル可視性追跡 | Low |
| Function | test_cross_file_extends_resolution | test (ignore) | 解決文脈構築後のクロスファイル継承解決 | Med |

### Dependencies & Interactions

- 内部依存: このテストファイル内の各テストは独立で、相互に状態を共有しない（毎回`GdscriptBehavior::new()`を使用）。内部の共通ユーティリティ関数は存在しない。
- 外部依存（表）:

  | 依存対象 | 用途 |
  |----------|------|
  | codanna::parsing::gdscript::GdscriptBehavior | テスト対象の振る舞いクラス |
  | codanna::parsing::gdscript::GdscriptResolutionContext | 解決コンテキストの型確認（ダウンキャスト） |
  | codanna::parsing::LanguageBehavior | 振る舞いトレイト（推定、詳細はこのチャンクには現れない） |
  | codanna::parsing::Import | importのデータ契約 |
  | codanna::Visibility | 可視性列挙型 |
  | codanna::FileId | ファイル識別子 |
  | codanna::{Range, Symbol, SymbolKind, SymbolId} | シンボル関連型 |
  | std::path::{Path, PathBuf} | ファイルパスの扱い |

- 被依存推定:
  - GdscriptBehaviorの実装（このチャンクには現れない）
  - GDScriptパーサ（統合テストでの挙動検証対象）
  - シンボル解決層（Resolution/Inheritence Resolver、このチャンクには現れない）

## API Surface (Public/Exported) and Data Contracts

このセクションは「テストから読み取れる期待契約」をまとめる。実装本体はこのチャンクには現れないため、署名は推定であり、正確なトレイト名や返却型は不明。Time/Spaceは一般的な推定。

| API名 | シグネチャ（推定） | 目的 | Time | Space |
|-------|---------------------|------|------|-------|
| format_module_path | fn format_module_path(&self, module_path: &str, class_name: &str) -> String | クラス名を与えても、GDScriptではファイルパスをモジュールとして扱うためパスを返す | O(n) | O(n) |
| parse_visibility | fn parse_visibility(&self, line: &str) -> Visibility | 行テキストから可視性（Public/Private）を抽出 | O(n) | O(1) |
| module_separator | fn module_separator(&self) -> &'static str | モジュール区切り文字 "/" を返す | O(1) | O(1) |
| supports_traits | fn supports_traits(&self) -> bool | GDScriptでのtraitsサポート可否（false） | O(1) | O(1) |
| supports_inherent_methods | fn supports_inherent_methods(&self) -> bool | 固有メソッド概念のサポート可否（false） | O(1) | O(1) |
| validate_node_kind | fn validate_node_kind(&self, kind: &str) -> bool | 対応ノード種別の妥当性検証 | O(1)〜O(n) | O(1) |
| module_path_from_file | fn module_path_from_file(&self, path: &Path, root: &Path) -> Option<String> | ファイルパスからres://モジュールパスを導出 | O(n) | O(n) |
| register_file | fn register_file(&self, path: PathBuf, file_id: FileId, module_path: String) | ファイルIDとモジュールパスの関連付け | O(1)〜O(log n) | O(1) |
| get_module_path_for_file | fn get_module_path_for_file(&self, file_id: FileId) -> Option<String> | 登録済みファイルIDからモジュールパス取得 | O(1)〜O(log n) | O(1) |
| add_import | fn add_import(&self, import: Import) | import情報の蓄積 | O(1)〜O(log n) | O(1) |
| get_imports_for_file | fn get_imports_for_file(&self, file_id: FileId) -> Vec<Import> | ファイル単位のimport一覧取得 | O(k) | O(k) |
| import_matches_symbol | fn import_matches_symbol(&self, import_path: &str, symbol_path: &str, context_module: Option<&str>) -> bool | importとシンボルパスの一致可否判定（res://、拡張子、相対） | O(n) | O(1) |
| is_resolvable_symbol | fn is_resolvable_symbol(&self, symbol: &Symbol) -> bool | シンボルが外部解決対象かどうか（Class/Functionなど） | O(1) | O(1) |
| is_symbol_visible_from_file | fn is_symbol_visible_from_file(&self, symbol: &Symbol, from: FileId) -> bool | 可視性規約に基づくファイル間可視性 | O(1) | O(1) |
| create_resolution_context | fn create_resolution_context(&self, file_id: FileId) -> Box<dyn {as_any_mut: fn}> | 解決コンテキスト生成（GdscriptResolutionContextにダウンキャスト可能） | O(1) | O(1) |
| create_inheritance_resolver | fn create_inheritance_resolver(&self) -> Box<dyn Unknown> | 継承関係の解決器生成 | O(1) | O(1) |

詳細説明:

1) format_module_path
- 目的と責務: クラス名が与えられても、モジュールはファイルパス基準であるため、そのままモジュールパス文字列を返す。
- アルゴリズム:
  1. 入力module_pathを返す（class_nameは無視）。
- 引数:

  | 名前 | 型 | 説明 |
  |------|----|------|
  | module_path | &str | ファイルパスに基づくモジュール識別子 |
  | class_name | &str | クラス名（このAPIでは使用されない） |

- 戻り値:

  | 型 | 説明 |
  |----|------|
  | String | 整形済みモジュールパス |

- 使用例:
  ```rust
  let behavior = GdscriptBehavior::new();
  assert_eq!(behavior.format_module_path("scripts/player", "Player"), "scripts/player");
  ```
- エッジケース:
  - 空文字列のmodule_path → 空文字列を返すか、エラーにすべきか（不明）。
  - 末尾に拡張子.gdが付く場合の扱い（テスト対象外）。

2) parse_visibility
- 目的と責務: 行テキストからPublic/Privateを判定。先頭アンダースコアはPrivate慣習。
- アルゴリズム（推定）:
  1. "func"や"class"、"var"等の宣言キーワードを検出。
  2. 識別子が"_"で始まるかを確認。
  3. "_"で始まればVisibility::Private、そうでなければVisibility::Public。
- 引数:

  | 名前 | 型 | 説明 |
  |------|----|------|
  | line | &str | 宣言行テキスト |

- 戻り値:

  | 型 | 説明 |
  |----|------|
  | Visibility | Public/Private |

- 使用例:
  ```rust
  assert_eq!(behavior.parse_visibility("func move():"), Visibility::Public);
  assert_eq!(behavior.parse_visibility("func _ready():"), Visibility::Private);
  ```
- エッジケース:
  - Godotライフサイクルメソッド（_ready, _process）をPublic扱いにするかどうか。テストはPrivateとしている。
  - スペースやコメント混在、型注釈の有無。

3) module_separator
- 目的: ファイルシステム準拠の"/"を返す。
- 使用例:
  ```rust
  assert_eq!(behavior.module_separator(), "/");
  ```

4) supports_traits / supports_inherent_methods
- 目的: 言語機能の非サポート（false）を表明。
- 使用例:
  ```rust
  assert!(!behavior.supports_traits());
  assert!(!behavior.supports_inherent_methods());
  ```

5) validate_node_kind
- 目的: パーサーが扱うノード種別の妥当性検証。
- 使用例:
  ```rust
  assert!(behavior.validate_node_kind("class_definition"));
  assert!(!behavior.validate_node_kind("struct_item"));
  ```
- エッジケース:
  - 未知のノード名 → false。

6) module_path_from_file
- 目的: "/project/scripts/player.gd" → "res://scripts/player"のようにroot基準でres://化。
- アルゴリズム（推定）:
  1. pathがroot配下か確認。
  2. root相対パスに変換。
  3. 末尾拡張子を除去（.gdを削る）。
  4. "res://{相対}"を返す。
- 使用例:
  ```rust
  let root = Path::new("/project");
  let path = Path::new("/project/scenes/levels/level1.gd");
  assert_eq!(behavior.module_path_from_file(path, root), Some("res://scenes/levels/level1".to_string()));
  ```
- エッジケース:
  - root外のパス → None。
  - 複数拡張子や大文字拡張子 → 正規化ルール不明。

7) register_file / get_module_path_for_file
- 目的: ファイルIDとモジュールパスの双方向マップを維持。
- 使用例:
  ```rust
  behavior.register_file(PathBuf::from("/project/scripts/player.gd"), file_id, "res://scripts/player".to_string());
  assert_eq!(behavior.get_module_path_for_file(file_id), Some("res://scripts/player".to_string()));
  ```
- エッジケース:
  - 重複登録（同じfile_idで差し替え）挙動不明。

8) add_import / get_imports_for_file
- 目的: ファイル単位のimport追跡（glob/type_only/alias含む）。
- 使用例:
  ```rust
  behavior.add_import(Import { file_id, path: "res://scripts/enemy.gd".into(), alias: None, is_glob: false, is_type_only: false });
  let imports = behavior.get_imports_for_file(file_id);
  assert_eq!(imports.len(), 1);
  ```
- エッジケース:
  - 同一importの重複追加はどう扱うか（重複許容/排除）。

9) import_matches_symbol
- 目的: importとシンボルパスの一致判定。以下を許容:
  - res://有無の差異
  - .gd拡張子有無
  - 相対パス（./, ../）のコンテキスト解決
- 使用例:
  ```rust
  assert!(behavior.import_matches_symbol("res://scripts/player.gd", "res://scripts/player", None));
  assert!(behavior.import_matches_symbol("./enemy.gd", "res://scripts/enemy", Some("res://scripts/player")));
  ```
- エッジケース:
  - 異なるルート、複雑な相対（../../）の扱い。
  - ディレクトリセパレータの正規化（Windows vs Unix）。

10) is_resolvable_symbol
- 目的: SymbolKindとScopeContextにより解決対象を判定。ModuleスコープのClass/Functionはtrue、Parameterはfalse。
- 使用例:
  ```rust
  let class_symbol = Symbol::new(...).with_scope(ScopeContext::Module);
  assert!(behavior.is_resolvable_symbol(&class_symbol));
  ```
- エッジケース:
  - 変数（メンバー変数 vs ローカル）区別。

11) is_symbol_visible_from_file
- 目的: 同一ファイルなら可視、異ファイルなら先頭アンダースコアは非可視。
- 使用例:
  ```rust
  assert!(behavior.is_symbol_visible_from_file(&symbol, same_file_id));
  assert!(!behavior.is_symbol_visible_from_file(&private_symbol, other_file_id));
  ```
- エッジケース:
  - class_nameによるグローバル可視性との相互作用。

12) create_resolution_context
- 目的: file_idに基づく解決コンテキスト生成。`as_any_mut().downcast_mut::<GdscriptResolutionContext>()`が成功する。
- 使用例:
  ```rust
  let mut ctx = behavior.create_resolution_context(file_id);
  let _gd = ctx.as_any_mut().downcast_mut::<GdscriptResolutionContext>().expect("Should be GdscriptResolutionContext");
  ```

13) create_inheritance_resolver
- 目的: 継承解決のためのリゾルバ生成（詳細型は不明）。
- 使用例:
  ```rust
  let _resolver = behavior.create_inheritance_resolver();
  ```

## Walkthrough & Data Flow

- 基本APIフロー（test_format_module_path, test_parse_visibility, test_module_separator, test_supports_features, test_validate_node_kinds）
  - 入力は主に文字列。返却はVisibilityやbool、"/"などのプリミティブ。外部状態に依存しない純粋関数的動作。

- ファイル→モジュールパス導出（test_module_path_from_file）
  - 入力: ファイルPathとプロジェクトroot Path。
  - 処理: root相対パスの抽出、拡張子.gdの除去、"res://"接頭辞付与。
  - 出力: Option<String>（root外の場合None）。

- 状態管理フロー（test_has_behavior_state, test_register_file, test_add_import, test_get_imports_for_file_empty, test_multiple_imports_same_file, test_imports_isolated_by_file）
  - register_file: file_id→module_path の保存。
  - add_import: file_id毎にimportリストへ追加。
  - get_imports_for_file: file_idでフィルタリングしてVec<Import>を返す。
  - 隔離: 各file_idは独立したimport空間を持つ。

- 解決フロー（test_import_matches_symbol_*）
  - 正規化ステップの組み合わせ:
    - res://接頭辞の付与/除去の同一視。
    - ".gd"拡張子の有無の同一視。
    - 相対（"./", "../"）のcontext_module基準解決。
  - 最終的に正規化された絶対的モジュールパス同士の比較。

- シンボル解決可能性と可視性（test_is_resolvable_symbol, test_is_symbol_visible_from_file）
  - is_resolvable_symbol: SymbolKind（Class/Function）かつScopeContext::Moduleでtrue、Parameter等でfalse。
  - is_symbol_visible_from_file: 同一file_idならtrue。他ファイルなら先頭"_"は非可視。

- GDScript固有（test_gdscript_class_name_import, test_gdscript_extends_import, test_gdscript_preload_import）
  - class_name: Import{ is_glob: true }としてグローバル可視化。
  - extends: 親クラスをimportとして追跡。
  - preload: リソース（.tscn等）をimportとして追跡しalias可能。

- コンテキストとリゾルバ（test_create_resolution_context, test_create_inheritance_resolver）
  - create_resolution_context: トレイトオブジェクトを返し、Anyを介してGdscriptResolutionContextへダウンキャスト可能。
  - create_inheritance_resolver: 継承解決器のインスタンスを返す（型詳細不明）。

## Complexity & Performance

- format_module_path: O(n)文字列コスト（実質ただの返却ならO(1)〜O(n)コピー）。Space O(n)（返却文字列）。
- parse_visibility: O(n)（文字列走査）、Space O(1)。
- module_separator: O(1)、Space O(1)。
- supports_*: O(1)、Space O(1)。
- validate_node_kind: O(1)〜O(n)（固定集合照合ならO(1)、文字列比較ならO(n)）。Space O(1)。
- module_path_from_file: O(n)（パス分解・拡張子除去）。Space O(n)。
- register_file/get_module_path_for_file: O(1)〜O(log n)（HashMap/TreeMapによる）。Space O(1)（項目追加時はO(1)増加）。
- add_import/get_imports_for_file: O(1)追加、取得はO(k)（kは当該ファイルのimport数）。Space O(k)。
- import_matches_symbol: O(n)（正規化・比較）。Space O(1)。
- is_resolvable_symbol/is_symbol_visible_from_file: O(1)。Space O(1)。
- create_resolution_context/create_inheritance_resolver: O(1)。Space O(1)。

ボトルネック:
- importが大量のファイルではget_imports_for_fileがO(k)で線形。必要に応じてインデックス化検討。
- 相対パス解決の実装が複雑化するとO(n)の定数係数が増大。

スケール限界:
- 大規模プロジェクトでのファイルID→モジュールパスマップ、file_id→importsのメモリ占有。キャッシュのライフサイクル管理が重要。

I/O/ネットワーク/DB:
- このテストではI/OはPathのみ（ファイル読み取りなし）。実装側がファイル存在確認を行う場合のI/O負荷は別途考慮（本チャンクには現れない）。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト:
- メモリ安全性: このテストは安全なRustのみ使用。実装側のunsafe利用有無は不明（このチャンクには現れない）。
- インジェクション: SQL/Commandは関与なし。Path処理におけるパストラバーサル（相対パス解決）が潜在的リスク。
- 認証・認可: 非該当。
- 秘密情報: ハードコード機密なし。ログへのシンボル名・パス漏えいは考慮（実装側）。
- 並行性: テストコードは単純。実装側で共有状態（importマップ等）を扱う場合のデータ競合（Race）、Deadlockの可能性に注意。

詳細エッジケース（テストから抽出・期待動作定義）:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| アンダースコア関数の可視性 | "func _ready():" | Visibility::Private | このチャンクには現れない | テストで定義 |
| res://なしのimport | "scripts/player.gd" vs "res://scripts/player" | 一致とみなす | このチャンクには現れない | テストで定義 |
| .gd拡張子の有無 | "res://scripts/player.gd" vs "res://scripts/player" | 一致とみなす | このチャンクには現れない | テストで定義 |
| 相対パス（同ディレクトリ） | "./enemy.gd", ctx="res://scripts/player" | "res://scripts/enemy"に解決 | このチャンクには現れない | テストで定義 |
| 相対パス（親ディレクトリ） | "../utils/math.gd", ctx="res://scripts/player" | "res://utils/math"に解決 | このチャンクには現れない | テストで定義 |
| import未追加 | file_id=1 | get_imports_for_fileは空Vec | このチャンクには現れない | テストで定義 |
| グローバルclass_name | Import{ path: "Player", is_glob: true } | グローバル可視性として扱う | このチャンクには現れない | テストで定義 |
| 同一/他ファイル可視性 | "_private_func" 他ファイル | false。同一ファイルならtrue | このチャンクには現れない | テストで定義 |

潜在バグ:
- パス正規化の不完全性（複数スラッシュ、ケース差異、Windowsパス区切り）。
- ファイル登録の再登録時の整合性（古いモジュールパスの残存）。
- importの重複管理（同じimportを複数回追加）。
- class_nameのグローバル名衝突対策不備。

## Design & Architecture Suggestions

- パス正規化の一元化:
  - "res://"付与/除去、".gd"拡張子扱い、相対→絶対解決を単一のユーティリティに集約。
  - OS差異（パス区切り）を吸収。

- 可視性規約の明確化:
  - 先頭アンダースコアのPrivate慣習と、Godotライフサイクルの扱いを仕様に明記。テストはPrivate扱いで固定しているため、実装はこれに合わせるか、例外規則を導入するならテストを更新。

- 状態管理のスレッド安全:
  - 多スレッド解析時の競合回避。HashMapならMutex/RwLock、lockの粒度調整。
  - get_imports_for_fileは読み取り頻度が高いためRwLockのRead重視が有効。

- Data Contractの拡張:
  - Importにsource（宣言位置Range）やkind（extends/preload/class_name）を付与するとデバッグ容易（このチャンクには現れないが提案）。

- 解決コンテキストの型安全:
  - as_any_mutダウンキャスト依存より、ジェネリック/タグ型で型安全に。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト構成:
  - Tier 1: 純粋なAPI（文字列・bool）を即値で検証。
  - Tier 3: 状態管理（register_file/add_import/get_imports_for_file）をシナリオ駆動で検証。
  - Tier 2: import_matches_symbolの多分岐を個別テストで網羅（res://、拡張子、相対）。
  - 可視性・解決可能性: Symbol/ScopeContextでの判定基準を点検。

- 追加で有益なケース:
  - Windows風パス（"scripts\\player.gd"）の正規化。
  - 相対パスの深い階層（"../../core/base.gd"）。
  - import重複追加の扱い（去重の要否）。
  - type_only=trueの意味づけ（シンボル可視性との相互作用）。

- 統合テスト（#[ignore]で宣言済み）:
  - パーサがextends/class_nameを検出し、behavior.add_importを呼ぶことの検証。
  - build_resolution_context後のクロスファイル解決（親クラス探索）。

- 使用例（追加サンプル）:
  ```rust
  // 相対パス解決の一括確認
  let ctx = Some("res://a/b/c");
  assert!(behavior.import_matches_symbol("../x.gd", "res://a/b/x", ctx));
  assert!(behavior.import_matches_symbol("../../y.gd", "res://a/y", ctx));
  ```

## Refactoring Plan & Best Practices

- テストのグルーピング:
  - import_matches_symbol系を1つのモジュールにまとめ、共通ヘルパー（正規化前後を表示）を導入。
  - 状態管理系は前処理/後処理で初期化/クリアを明示。

- フィクスチャの導入:
  - 反復的な`GdscriptBehavior::new()`、`FileId::new(...)`、`Path::new(...)`生成を関数化。

- プロパティベーステスト:
  - パス正規化に対しQuickCheck風で多様なパスを生成し一致性検証。

- 失敗時メッセージ強化:
  - import_matches_symbolの失敗時に正規化ステップの中間結果を出力（テスト側は期待文字列提示）。

- コントラクト文書化:
  - 本テストファイルのコメントに仕様を箇条書きで追加し、実装側と同期。

## Observability (Logging, Metrics, Tracing)

- ログ:
  - パス正規化処理（入力→正規化結果）をtrace/debugログで出力。
  - import追加時（file_id, path, alias, flags）をinfo/debug。
  - ファイル登録時（file_id↔module_path）をdebug。

- メトリクス:
  - import数/ファイル、解決試行回数、成功/失敗率。
  - 正規化の平均文字列長、相対解決の深さ（../の数）。

- トレーシング:
  - 解決コンテキスト生成からシンボル解決までのスパンにspanを設定（file_idタグ、モジュールパスタグ）。
  - 継承解決チェーンの深さ・探索ノード数。

※ このテストファイル自体にはログ/メトリクスの呼び出しはない。

## Risks & Unknowns

- 不明点:
  - 実際のトレイト名・戻り型（ResolutionContext/InheritanceResolver等）はこのチャンクには現れない。
  - 可視性におけるGodotライフサイクルメソッドの公式扱いとテスト仕様の差異。
  - type_onlyフラグの解決への影響は未検証。

- リスク:
  - パス正規化が不完全だと、誤一致/不一致が発生し解決精度を損なう。
  - グローバルclass_nameの衝突（同名クラス）扱い。
  - 共有状態のスレッド安全性（並列解析時）。

- 緩和策:
  - パス正規化の包括的ユニットテストとユーティリティ化。
  - 可視性ポリシーの明文化とドキュメント整合。
  - 共有マップにRwLock、読み取りを優先、書き込み時の最小ロック時間。 

以上、当該テストファイルはGdscriptBehaviorの外部API契約と期待動作を広範囲に定義しており、実装者は本仕様に準拠して安全・一貫・可観測な実装を行うべきである。