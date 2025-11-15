# behavior.rs Review

## TL;DR

- TypeScriptBehavior は LanguageBehavior/StatefulBehavior を実装し、TypeScript 特有のモジュール解決・可視性・継承関係・名前解決を提供する中核クラス
- 公開APIは主に pub struct TypeScriptBehavior と pub fn new()。実際の利用面では LanguageBehavior の各メソッド実装が外部に影響する実質的なAPI
- 重要ロジックは import 正規化と tsconfig ベースのエイリアス解決、DocumentIndex/ConcurrentSymbolCache を用いた高速解決、ネームスペース import の事前計算
- 既知の制約: named import（別名なし）を未対応でスキップ、parse_visibility は文字列包含ベースで誤判定のリスク、index.ts 解決の一貫性にばらつき
- パフォーマンス: 非キャッシュ版は get_all_symbols(10000) を行い O(N) で重い可能性。with_cache 版は候補を局所化してスケールしやすい
- セキュリティ/安全性: unsafe なし、thread_local キャッシュは1秒TTLでレース小、パストラバーサルは基本的に正規化/ルート固定で低リスク
- 並行性: thread_local キャッシュ、共有 DocumentIndex とシンボルキャッシュ（ConcurrentSymbolCache）利用。BehaviorState の内部同期はこのチャンクでは不明

## Overview & Purpose

このファイルは、TypeScript 言語に特化した解析・解決の振る舞い（Behavior）を実装する。主な目的は以下の通り。

- TypeScript のモジュールパス規約（tsconfig に基づくエイリアス含む）を用いた一貫した module_path の計算
- import 文の追跡と、解決時における相対/絶対パス正規化、tsconfig ルールの適用（プロジェクトエイリアスの解決）
- シンボル解決コンテキストの構築（ファイル内シンボル、インポート済シンボル、公開シンボルなど）
- ネームスペース import（import * as X from 'mod'）の別名解決、および修飾名（X.member）への事前マッピング
- 外部モジュール由来の呼び出しターゲット推定と、外部シンボルの擬似作成（.codanna/external/*.d.ts 仮想パス）
- TypeScript の可視性/ホイスティング/継承（extends/implements）に関する言語固有の取り扱い

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | TypeScriptBehavior | pub | TS向け LanguageBehavior 実装、状態(State)保持 | Med |
| Function | TypeScriptBehavior::new | pub | 振る舞いインスタンス生成 | Low |
| Method | load_project_rules_for_file | private | tsconfig ルール（ResolutionIndex）の1秒TTLの読み込みキャッシュ | Med |
| Trait Impl | StatefulBehavior for TypeScriptBehavior | crate | 状態取得（state） | Low |
| Trait Impl | LanguageBehavior for TypeScriptBehavior | crate | 言語固有の各操作（モジュールパス、import解決、可視性、継承、解決コンテキスト構築、外部シンボル生成など） | High |

- 主要ヘルパ/依存：
  - BehaviorState（register_file_with_state, add_import_with_state, get_imports_from_state, get_module_path など）
  - DocumentIndex（シンボル/ファイル/メタデータの永続化・検索）
  - ResolutionPersistence/ResolutionIndex（.codanna 内の tsconfig 解析結果）
  - TypeScriptResolutionContext（解決スコープ、ネームスペース別名や修飾名管理）
  - TypeScriptInheritanceResolver（継承関係の解決）
  - ConcurrentSymbolCache（候補探索を高速化）
  - tree_sitter_typescript::LANGUAGE_TYPESCRIPT

### Dependencies & Interactions

- 内部依存（関数/構造体間）
  - register_file → register_file_with_state（BehaviorState）でファイルと module_path を登録
  - add_import → add_import_with_state（BehaviorState）で import を追跡
  - get_imports_for_file → get_imports_from_state（BehaviorState）
  - build_resolution_context
    - resolve_import（各 import を DocumentIndex とパス正規化で解決）
    - DocumentIndex.find_symbols_by_file/get_all_symbols
    - context.add_import_symbol/add_symbol/add_symbol_with_context
    - context のダウンキャスト → TypeScriptResolutionContext（add_namespace_alias, add_qualified_name）
  - build_resolution_context_with_cache
    - load_project_rules_for_file → TypeScriptProjectEnhancer::enhance_import_path
    - ConcurrentSymbolCache.lookup_candidates
    - DocumentIndex.find_symbol_by_id/find_symbols_by_name/find_symbols_by_file
  - resolve_external_call_target
    - BehaviorState 上の import 追跡から別名/ネームスペースを参照し、修飾名をモジュールへマップ
  - create_external_symbol
    - DocumentIndex.get_next_file_id/index_symbol/store_file_info/store_metadata 等で仮想シンボル生成
  - module_path_from_file
    - ResolutionPersistence.load("typescript") → ResolutionIndex.get_config_for_file

- 外部依存（使用クレート・モジュール）

| 依存 | 用途 |
|------|------|
| tree_sitter_typescript::LANGUAGE_TYPESCRIPT | TypeScript 言語定義取得 |
| crate::storage::DocumentIndex | シンボル/ファイルの検索・保存 |
| crate::project_resolver::persist::{ResolutionPersistence, ResolutionRules} | tsconfig ルール永続化読み込み |
| crate::parsing::{LanguageBehavior, resolution::{ResolutionScope, InheritanceResolver}} | 振る舞いインタフェース、解決スコープ |
| super::resolution::{TypeScriptInheritanceResolver, TypeScriptResolutionContext} | TS 専用の継承/解決文脈 |
| crate::storage::symbol_cache::ConcurrentSymbolCache | シンボル候補の高速探索 |

- 被依存推定（このモジュールを使う箇所）
  - TypeScript パーサ/インデクサ（LanguageBehavior を DI で差し替える箇所）
  - 参照解決（関数呼び出し/型参照のリンク付け）
  - クロスリファレンス/ナビゲーション機能（定義へ移動等）
  - 継承・実装関係の抽出パス

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| TypeScriptBehavior::new | pub fn new() -> Self | 振る舞いインスタンスの生成 | O(1) | O(1) |
| StatefulBehavior::state | fn state(&self) -> &BehaviorState | 内部状態への参照提供 | O(1) | O(1) |
| configure_symbol | fn configure_symbol(&self, symbol: &mut Symbol, module_path: Option<&str>) | シンボルの module_path を設定 | O(1) | O(1) |
| format_module_path | fn format_module_path(&self, base: &str, _: &str) -> String | TS のモジュールパス整形（ファイル単位） | O(n) | O(n) |
| get_language | fn get_language(&self) -> Language | TS 言語ID取得 | O(1) | O(1) |
| module_separator | fn module_separator(&self) -> &'static str | モジュール区切り文字（"."） | O(1) | O(1) |
| module_path_from_file | fn module_path_from_file(&self, file: &Path, project_root: &Path) -> Option<String> | tsconfig に基づく module_path 計算 | O(p) | O(1) |
| parse_visibility | fn parse_visibility(&self, signature: &str) -> Visibility | TS 可視性推定 | O(n) | O(1) |
| supports_traits | fn supports_traits(&self) -> bool | interface 対応報告 | O(1) | O(1) |
| supports_inherent_methods | fn supports_inherent_methods(&self) -> bool | クラスメソッド対応報告 | O(1) | O(1) |
| create_resolution_context | fn create_resolution_context(&self, file_id: FileId) -> Box<dyn ResolutionScope> | TS 専用解決文脈生成 | O(1) | O(1) |
| create_inheritance_resolver | fn create_inheritance_resolver(&self) -> Box<dyn InheritanceResolver> | TS の継承解決器生成 | O(1) | O(1) |
| inheritance_relation_name | fn inheritance_relation_name(&self) -> &'static str | "extends" を返す | O(1) | O(1) |
| map_relationship | fn map_relationship(&self, lang_specific: &str) -> RelationKind | TS 用の関係マップ | O(1) | O(1) |
| register_file | fn register_file(&self, path: PathBuf, id: FileId, modpath: String) | State へのファイル登録 | O(1) | O(1) |
| add_import | fn add_import(&self, import: Import) | State への import 登録（元のパス保持） | O(1) | O(1) |
| get_imports_for_file | fn get_imports_for_file(&self, file_id: FileId) -> Vec<Import> | ファイル単位 import 取得 | O(k) | O(k) |
| resolve_external_call_target | fn resolve_external_call_target(&self, to_name: &str, from_file: FileId) -> Option<(String, String)> | 未解決呼び出しを import に基づき外部ターゲットへマッピング | O(k) | O(1) |
| create_external_symbol | fn create_external_symbol(&self, index: &mut DocumentIndex, module_path: &str, symbol_name: &str, lang_id: LanguageId) -> IndexResult<SymbolId> | 外部シンボルの仮想生成 | O(logN)〜O(N) | O(1) |
| build_resolution_context | fn build_resolution_context(&self, file_id: FileId, index: &DocumentIndex) -> IndexResult<Box<dyn ResolutionScope>> | 非キャッシュの解決文脈構築 | O(k + N) | O(k + N) |
| build_resolution_context_with_cache | fn build_resolution_context_with_cache(&self, file_id: FileId, cache: &ConcurrentSymbolCache, index: &DocumentIndex) -> IndexResult<Box<dyn ResolutionScope>> | キャッシュ活用の高速解決 | O(k·c + s) | O(k + s) |
| is_resolvable_symbol | fn is_resolvable_symbol(&self, symbol: &Symbol) -> bool | TS のホイスティング/スコープに基づく解決可否 | O(1) | O(1) |
| resolve_import | fn resolve_import(&self, import: &Import, index: &DocumentIndex) -> Option<SymbolId> | import をシンボルに解決 | O(c + logN)〜O(N) | O(1) |
| get_module_path_for_file | fn get_module_path_for_file(&self, file_id: FileId) -> Option<String> | State から module_path を O(1) 取得 | O(1) | O(1) |
| import_matches_symbol | fn import_matches_symbol(&self, import_path: &str, symbol_module_path: &str, importing_module: Option<&str>) -> bool | インポートパスとシンボル module_path の一致判定 | O(p) | O(1) |

注:
- N=全シンボル数、k=ファイルの import 数、c=候補数（キャッシュ制限）、s=import から到達したファイル群のシンボル数、p=パス長
- exports=2（このチャンク内）＝pub struct TypeScriptBehavior, pub fn TypeScriptBehavior::new。その他は trait 経由で外部から利用される実質API

データ契約（このチャンクで仕様が読み取れるもの）:
- Import 構造体（外部定義）
  - path: String（オリジナルを保持。相対/エイリアス/パッケージ）
  - alias: Option<String>（ローカル名。namespace import では別名、named/default import でも使用）
  - file_id: FileId
  - is_glob: bool（namespace import）
  - is_type_only: bool（type-only import 空間へ配置分岐）

### 主要APIの詳細

1) TypeScriptBehavior::new
- 目的と責務: 振る舞いインスタンスを初期化（内部 BehaviorState を new）
- アルゴリズム: BehaviorState::new を呼ぶだけ
- 引数/戻り値:
  - 引数: なし
  - 戻り値: Self
- 使用例:
```rust
let behavior = TypeScriptBehavior::new();
```
- エッジケース:
  - 特になし

2) module_path_from_file
- 目的と責務: tsconfig の適用範囲に基づき、ファイルから一意的な module_path を算出
- アルゴリズム（簡略）:
  1. .codanna の ResolutionPersistence から "typescript" インデックスを load
  2. file_path を project_root 相対にして get_config_for_file で該当 tsconfig を取得
  3. tsconfig ディレクトリを基準に相対パスを得て拡張子や末尾 /index を除去
  4. '/' を '.' に置換
- 引数:
  - file_path: &Path
  - project_root: &Path
- 戻り値: Option<String>（成功時 module_path）
- 使用例:
```rust
let mp = behavior.module_path_from_file(Path::new("packages/foo/src/a.ts"), Path::new("packages/foo"));
```
- エッジケース:
  - インデックス/設定が見つからない → None
  - 非UTF-8パス → None
  - .d.ts/.mts/.cts を含む拡張子除去ルール
  - 末尾 index を落とすため "dir/index.ts" → "dir" に正規化

3) build_resolution_context
- 目的と責務: 非キャッシュパスでの解決文脈構築（import・ファイル内・公開シンボル）
- アルゴリズム（概要）:
  1. import を取得し resolve_import で解決。namespace import 未解決時は alias→module の対応を記録
  2. ファイル内シンボルを scope_context に応じて追加。module_path キーでも追加
  3. get_all_symbols(10000) から公開シンボルを可視性判定で追加。module_path キーでも追加
  4. namespace import の alias に対し、public_symbols のうち対象モジュールに属するものに修飾名 alias.member を precompute
- 引数/戻り値:
  - file_id: FileId, index: &DocumentIndex
  - 戻り値: Box<dyn ResolutionScope>
- 使用例:
```rust
let ctx = behavior.build_resolution_context(file_id, &index)?;
```
- エッジケース:
  - import が未解決（named なしなど）→ namespace だけ precompute
  - 大量のシンボルで get_all_symbols が重い

4) build_resolution_context_with_cache
- 目的と責務: キャッシュ（ConcurrentSymbolCache）を活用し、高速に解決文脈を構築
- アルゴリズム（概要）:
  1. import を列挙。alias がないものはスキップ（制限）
  2. ルールキャッシュ（1秒TTL）から tsconfig ルールを取得し、可能ならエイリアス解決（ProjectEnhancer.enhance_import_path）。失敗時は相対正規化
  3. local_name の候補を cache.lookup_candidates で取得→ DocumentIndex.find_symbol_by_id で module_path 一致を確認。失敗時 DB 検索へフォールバック
  4. ファイル内シンボル追加（module_path キーでも）
  5. import の alias から到達できるファイル群を推定し、各ファイルの公開シンボルのみ追加（module_path キーでも）
- 引数/戻り値:
  - file_id, &ConcurrentSymbolCache, &DocumentIndex
  - 戻り値: Box<dyn ResolutionScope>
- 使用例:
```rust
let ctx = behavior.build_resolution_context_with_cache(file_id, &cache, &index)?;
```
- エッジケース:
  - alias がない named import はスキップ（現状の仕様）
  - tsconfig ルールがない/読み込めない → 相対正規化にフォールバック

5) resolve_import
- 目的と責務: Import を 1 つのシンボルに解決（named/default を想定）
- アルゴリズム:
  1. import.path が「強化済み」（相対+src を含む）なら tsconfig ルート基準で '.' 区切りへ変換、そうでなければ相対正規化
  2. alias がある場合、同名シンボルを DocumentIndex から列挙し module_path 一致で決定
  3. alias なし＝namespace/副作用 import は単一シンボルへは解決しない
- 引数/戻り値:
  - import: &Import, index: &DocumentIndex
  - 戻り値: Option<SymbolId>
- 使用例:
```rust
if let Some(id) = behavior.resolve_import(&imp, &index) { /* ... */ }
```
- エッジケース:
  - 候補が多い/一致なし → None
  - index.ts との一致処理は import_matches_symbol の方にあり、本関数内は厳密一致寄り

6) resolve_external_call_target
- 目的と責務: 未解決の呼び出し（to_name）を import 情報を用いて (module_path, symbol_name) に推定マップ
- アルゴリズム:
  - "Alias.member" 形式なら namespace import の alias に一致するものを探し、対象モジュールへ正規化
  - 単一識別子なら named import の alias に一致するものを探しモジュールへ正規化
- 引数/戻り値:
  - to_name: &str, from_file: FileId
  - 戻り値: Option<(String module_path, String symbol_name)>
- 使用例:
```rust
if let Some((module_path, sym)) = behavior.resolve_external_call_target("React.useState", file_id) {}
```
- エッジケース:
  - import が空/一致なし → None

7) create_external_symbol
- 目的と責務: 実体のない外部呼び出し先を仮想シンボルとして DocumentIndex に登録
- アルゴリズム:
  - 既存で同名+同 module_path があれば再利用
  - なければ .codanna/external/<module_path>.d.ts の仮想ファイルを用意し、SymbolKind::Function として登録
  - メタデータの SymbolCounter を更新
- 引数/戻り値:
  - index: &mut DocumentIndex, module_path: &str, symbol_name: &str, language_id
  - 戻り値: IndexResult<SymbolId>
- 使用例:
```rust
let id = behavior.create_external_symbol(&mut index, "react", "useState", lang_id)?;
```
- エッジケース:
  - 種別が常に Function 固定 → 精度低下（改善余地）

8) parse_visibility
- 目的と責務: 簡易な可視性推定（export/private/protected など）
- アルゴリズム: 文字列包含による判定
- 引数/戻り値:
  - signature: &str → Visibility
- 使用例:
```rust
let v = behavior.parse_visibility("export function foo() {}");
```
- エッジケース:
  - "exported" のような部分一致の誤検知リスク

9) import_matches_symbol
- 目的と責務: import_path と symbol.module_path の合致判定（相対パス/.. 解決、index フォールバック）
- アルゴリズム: 相対正規化→ '.' 区切り化、candidate==target もしくは candidate+".index"==target を許容
- 引数/戻り値:
  - (import_path, symbol_module_path, importing_module) → bool
- 使用例:
```rust
let ok = behavior.import_matches_symbol("./utils", "app.utils", Some("app"));
```
- エッジケース:
  - importing_module がない場合は単純一致のみ

この他のメソッド（supports_traits等）は軽量・自明のため割愛。

## Walkthrough & Data Flow

- 登録フェーズ
  - register_file でファイルIDと計算済み module_path を BehaviorState に登録
  - add_import で Import（オリジナルのパスを保持）を BehaviorState に登録

- 解決フェーズ（非キャッシュ: build_resolution_context）
  1. BehaviorState から import 群を取得
  2. 各 import を resolve_import で解決（alias なし namespace は未解決→後段で alias→module を保持）
  3. ファイル内シンボルを scope_context 付きで追加、module_path キーでも追加
  4. DocumentIndex から最大 10000 の全シンボルを取得し、可視性に応じて追加、module_path キーでも追加
  5. namespace import の alias に対し、対象モジュールの public_symbols から alias.member 形式の修飾名を事前計算（TypeScriptResolutionContext にセット）

- 解決フェーズ（キャッシュ: build_resolution_context_with_cache）
  1. import を列挙（alias なしはスキップ）
  2. 1秒TTLの tsconfig ルールロード→可能ならエイリアスを絶対化→ '.' 区切りに
  3. ConcurrentSymbolCache の候補を module_path 比較で早期決定。外れたら DB フォールバック
  4. ファイル内シンボル追加、import から推定した imported_files のシンボルだけをグローバルに追加

- 呼び出し先推定（resolve_external_call_target）
  - "Alias.member" か "name" を import 情報と importing_module に基づいて module_path に正規化し、(module_path, member/name) を返す

### シーケンス図（build_resolution_context_with_cache の主要フロー）

```mermaid
sequenceDiagram
    participant B as TypeScriptBehavior
    participant S as BehaviorState
    participant C as ConcurrentSymbolCache
    participant I as DocumentIndex
    participant P as ResolutionPersistence/Enhancer

    B->>S: get_imports_for_file(file_id)
    S-->>B: Vec<Import>
    loop for each import
        alt alias is Some
            B->>B: load_project_rules_for_file(file_id)
            B->>P: enhance_import_path(import.path)?
            alt enhanced
                P-->>B: enhanced_path
                B->>B: target_module = enhanced_path '.'-join
            else not enhanced
                B->>B: target_module = normalize_ts_import(import.path, importing_module)
            end
            B->>C: lookup_candidates(local_name, 16)
            C-->>B: [SymbolId...]
            loop for each candidate
                B->>I: find_symbol_by_id(id)
                I-->>B: Option<Symbol>
                alt module_path == target_module
                    B->>B: matched = Some(id); break
                end
            end
            alt no match
                B->>I: find_symbols_by_name(local_name)
                I-->>B: Vec<Symbol>
                alt any module_path == target_module
                    B->>B: matched = Some(id)
                end
            end
            alt matched
                B->>B: context.add_import_symbol(local_name, id, is_type_only)
            end
        else alias is None
            B->>B: skip (named import without alias)
        end
    end
    B->>I: find_symbols_by_file(file_id)
    I-->>B: Vec<Symbol>
    B->>B: add own symbols (+ by module_path)
    B->>S: get_imports_for_file(file_id)
    S-->>B: Vec<Import>
    B->>C: discover imported_files via cache
    loop for each imported_file
        B->>I: find_symbols_by_file(imported_file)
        I-->>B: Vec<Symbol>
        B->>B: add visible symbols (+ by module_path)
    end
    B-->>B: return Box<ResolutionScope>
```

上記の図は build_resolution_context_with_cache 関数の主要分岐を示す（行番号はこのチャンクでは不明）。

### フローチャート（resolve_import の主要分岐）

```mermaid
flowchart TD
  A[Start resolve_import] --> B{enhanced? (./ and contains /src/)}
  B -- Yes --> C[Convert enhanced_path -> module_path ('.' sep)]
  B -- No --> D[Normalize relative import vs importing_module]
  C --> E{alias exists?}
  D --> E
  E -- No --> F[Namespace/side-effect import -> return None]
  E -- Yes --> G[find_symbols_by_name(alias)]
  G --> H{module_path == target?}
  H -- Yes --> I[Return Some(symbol_id)]
  H -- No --> J[Return None]
```

上記の図は resolve_import 関数の主要分岐を示す（行番号はこのチャンクでは不明）。

## Complexity & Performance

- module_path_from_file: O(p)（パス長）。ディスクからインデックスを読むのは一度（成功時）だが load() 自体は O(size of index)
- build_resolution_context:
  - imports: O(k) × resolve_import のコスト
  - resolve_import: 候補探索 O(C)（名前一致候補数）〜 O(N)（最悪）
  - ファイル内シンボル: O(m)
  - 公開シンボル: get_all_symbols(10000) → O(N) でボトルネック
- build_resolution_context_with_cache:
  - imports: O(k) × {cache候補 c（デフォルト16） + fallback 検索} → ほぼ O(k·c)
  - imported_files のシンボル追加: O(s)
  - 全体として O(k·c + s) でスケールしやすい
- resolve_external_call_target: O(k)（import 列挙と簡易一致）
- create_external_symbol: 既存検索 O(C)〜O(N)、新規ID発番/保存は O(1) 前後（永続層依存）

スケール限界/ボトルネック:
- 非キャッシュ版の get_all_symbols(10000) はプロジェクト規模により顕著なコスト
- parse_visibility の文字列検索は軽微だが誤判定による再解決/再探索のリスクがある
- DocumentIndex の find_symbols_by_name が広い候補を返す場合は最悪 O(N)

I/O/ネットワーク/DB:
- .codanna からのルール読み込み（1秒TTL、thread_localキャッシュあり）
- DocumentIndex との相互作用が主なI/O（tantivy 等のコストに依存）

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| named import（別名なし） | import { useState } from 'react' | useState を解決 | build_resolution_context_with_cache は alias None をスキップ、非キャッシュは resolve_import 側も alias 必須 | 既知の制約 |
| namespace import の事前計算 | import * as React from 'react' | React.useState → react のシンボルへ | 非キャッシュ: public_symbols から修飾名 precompute。キャッシュ版は明示処理なし | 部分的対応 |
| index.ts/ディレクトリ import | import "./utils" vs symbol "app.utils.index" | 双方一致扱い | import_matches_symbol は index 許容。resolve_import の等価考慮は限定的 | 非一貫 |
| tsconfig ルール未設定 | ルールなし | 相対正規化で解決 | 両ビルダーでフォールバック | OK |
| protected の可視性 | protected | Module 可視に近似 | parse_visibility で Module へマップ | 仕様通り |
| 文字列包含の誤検知 | "exported" 等 | 誤検知回避 | 単純包含で誤検知の可能性 | 潜在バグ |
| 外部シンボル種別固定 | 任意シンボル | 適切な種別 | Function 固定で作成 | 改善余地 |
| Windows パス区切り | "a\\b.ts" | 正常化 | このチャンクでは '/' 前提。Path→str 変換辺りで注意 | 潜在問題 |
| キャッシュTTL | ルール頻繁更新 | 迅速反映 | 1秒TTL → 閾値内は古い情報 | 設計上の妥協 |

セキュリティチェックリスト:
- メモリ安全性: unsafe 使用なし。所有権/借用は &self とローカル所有に限定。Use-after-free/Buffer overflow/整数オーバーフローの懸念なし（このチャンク）
- インジェクション: 外部コマンド/SQL なし。パスは固定ルート配下(".codanna")を用いて構築（Path traversal リスク低）
- 認証・認可: 該当なし
- 秘密情報: Hard-coded secrets なし。debug_print にパスや識別子が出力される可能性あり（ログ漏えいの懸念は低だが注意）
- 並行性: thread_local キャッシュ（1秒TTL）でデータ競合の懸念は低。BehaviorState の内部同期は不明（このチャンクには現れない）。DocumentIndex の同時操作の安全性は DocumentIndex 側次第（不明）

Rust特有の観点:
- 所有権/借用: すべて &self で不変参照。DocumentIndex は &mut が必要な箇所（create_external_symbol）で限定的に可変借用
- ライフタイム: 明示的ライフタイムなし。返却するのは String/Box 等の所有値で問題なし
- unsafe 境界: なし
- 並行性: Send/Sync の明示境界はこのファイルでは定義なし。ConcurrentSymbolCache 使用箇所あり。await 非同期は使用なし
- エラー設計: IndexResult/IndexError を用い、I/O 相当箇所で map_err を適切に実施。panic 相当（unwrap 等）は少数（例: module_path_from_file 内の strip_prefix の ok().unwrap_or(...) は安全側フォールバック）

根拠の行番号はこのチャンクには現れないため、関数名のみ記載。

## Design & Architecture Suggestions

- パス正規化の重複解消
  - normalize_ts_import と同等のロジックが複数関数に重複。単一ヘルパ（例: ts_path::normalize_import(import_path, importing_module, rules)）に集約
  - index.ts 補完や OS 依存区切り（'\\'）も含め、統一仕様をテストで保証
- named import（別名なし）の対応
  - Import 構造体に named 列（Vec<(imported, local)>）を持たせる等し、local=imported を自動付与してコンテキストに登録
- create_external_symbol の種別推定
  - 呼び出しコンテキストや使用位置から Function/Variable/Class を推定、少なくとも "unknown/external" 汎用種別の導入で誤同定を回避
- get_all_symbols 依存の削減
  - DocumentIndex に「module_path/prefix での絞り込み」「可視シンボルのみのスキャン」などクエリ追加
  - namespace import の precompute も targeted query に置換（public_symbols 全走査回避）
- 可視性判定の強化
  - parse_visibility を AST ベースへ（signature 文字列包含ではなく、tree-sitter ノード属性利用）
- 監視と設定リロード
  - 1秒TTL 以外に変更通知（ファイル更新時 invalidate）を追加し、不要なディスク I/O を更に削減

## Testing Strategy (Unit/Integration) with Examples

ユニットテスト:
- パス正規化（相対/親ディレクトリ/エイリアス/index.ts）
```rust
#[test]
fn normalize_relative_imports() {
    let b = TypeScriptBehavior::new();
    // 擬似的に import_matches_symbol を使って確認
    assert!(b.import_matches_symbol("./utils", "app.utils", Some("app")));
    assert!(b.import_matches_symbol("../core", "core", Some("app.module")));
    assert!(b.import_matches_symbol("./dir", "app.dir.index", Some("app")));
}
```

- parse_visibility の誤検出防止
```rust
#[test]
fn parse_visibility_basic() {
    let b = TypeScriptBehavior::new();
    assert!(matches!(b.parse_visibility("export function x(){}"), crate::Visibility::Public));
    assert!(matches!(b.parse_visibility("private x:any"), crate::Visibility::Private));
    assert!(matches!(b.parse_visibility("protected x:any"), crate::Visibility::Module));
    // 誤検知ケース（将来改善の回帰テスト）
    assert!(matches!(b.parse_visibility("const exported = 1;"), crate::Visibility::Private));
}
```

- create_external_symbol の再利用
```rust
#[test]
fn reuse_external_symbol() {
    let mut index = DocumentIndex::new_in_memory().unwrap();
    let b = TypeScriptBehavior::new();
    let id1 = b.create_external_symbol(&mut index, "react", "useState", crate::parsing::LanguageId::TypeScript).unwrap();
    let id2 = b.create_external_symbol(&mut index, "react", "useState", crate::parsing::LanguageId::TypeScript).unwrap();
    assert_eq!(id1, id2);
}
```

統合テスト:
- tsconfig エイリアス解決
  - .codanna にエイリアスルールを用意し、build_resolution_context_with_cache で import "@" 系を module_path に正しくマップできるか
- namespace import の修飾名解決
  - import * as React from 'react' → React.useState が public_symbols から解決されるか（非キャッシュパス）
- 大規模プロジェクトにおけるパフォーマンス
  - k, N を増やし、非キャッシュとキャッシュの構築時間/メモリを比較

## Refactoring Plan & Best Practices

- 正規化ヘルパの単一化＋包括テスト
  - 重複コードの削減と、index.ts/拡張子除去/OS差異の明文化
- Import モデル拡張
  - named import の網羅（local=imported デフォルト）、default import の明示フラグ追加
- コンテキスト構築の段階的最適化
  - get_all_symbols 廃止に向け、必要なモジュール/ファイルに限定した取得APIを DocumentIndex に追加
- ログ/デバッグの整流化
  - debug_print の粒度制御、PII 配慮、構築時間の計測ログ
- 外部シンボルの拡張
  - 種別/可視性/スコープの推定ルールを導入。SymbolKind::UnknownExternal 等

## Observability (Logging, Metrics, Tracing)

- 既存: debug_print!, eprintln! による詳細ログ（グローバルフラグ連動）
- 追加提案:
  - 計測ポイント
    - load_project_rules_for_file のヒット/ミス（TTL）
    - build_resolution_context(_with_cache) の所要時間、解決成功/失敗件数、fallback 回数
    - resolve_import の候補数・一致率
  - OpenTelemetry/Tracing の導入
    - スパン: resolution_context 構築、import 解決、DocumentIndex クエリ
  - ログレベル分離（info/debug/trace）と抑制機構

## Risks & Unknowns

- BehaviorState の内部スレッド安全性・データ構造は不明（このチャンクには現れない）
- DocumentIndex の同時アクセス特性・クエリ性能は実装に依存（不明）
- TypeScriptProjectEnhancer（ルール適用）の詳細は不明（このチャンクには現れない）
- Windows 路線やマルチルートワークスペースでのパス取り扱いは未検証
- parse_visibility の簡易実装による誤判定が後段解決に与える影響（未知の組み合わせでの挙動）