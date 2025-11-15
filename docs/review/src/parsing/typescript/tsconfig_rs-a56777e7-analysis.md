# tsconfig.rs Review

## TL;DR

- 目的: TypeScriptのtsconfig.json（JSONC対応）を解析し、extends連鎖の解決とパスエイリアスのコンパイル・解決を行う。
- 主要公開API: parse_jsonc_tsconfig, read_tsconfig, resolve_extends_chain, PathAliasResolver::{from_tsconfig, resolve_import, expand_extensions}, PathRule::{new, try_resolve}。
- 複雑箇所: resolve_extends_chainの再帰・循環検出と親子マージの整合性、PathRuleのTSパターン→Rust正規表現変換、from_tsconfigの特異度ソート。
- 重大リスク: pathsのターゲット配列を1件目しか使わないため、複数解決が欠落。Windowsパス区切りと文字列連結の非整合、extendsのパッケージ解決非対応。
- 安全性: unsafeなし。ファイルIO/JSON5/regexに依存。エラーはResolutionError（invalid_cache, cache_io）に正しくラップ。
- パフォーマンス: ルール数Rに対してresolve_importがO(R)で正規表現マッチ。大規模プロジェクトでRが増えるとコスト増。
- テスト: JSONC、最小構成、extends連鎖、循環検出、パス解決、拡張子展開を網羅。複数ターゲットや境界パターンの追加テスト余地あり。

## Overview & Purpose

このファイルはTypeScriptのtsconfig.jsonを以下の要件で扱うためのコアです。

- JSONC（コメント・末尾カンマ）をjson5でパースする。
- extendsによる設定継承の連鎖を再帰的に解決し、子が親を上書きする形でマージする。
- compilerOptions.pathsを正規表現にコンパイルしたルールとして保持し、import文字列をパス候補に解決する。
- baseUrlを適用し、TypeScript標準的な拡張子候補（.ts, .tsx, .d.ts, index.ts, index.tsx）を展開する。

利用者は、tsconfigファイルを読み込み、必要ならextendsを解決した上でPathAliasResolverを作成し、import specifierを解決するフローを取る。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | PathRule | pub | TSのpathsエントリを正規表現にコンパイルし、specifierを1ターゲットへ置換 | Med |
| Struct | PathAliasResolver | pub | pathsからルールセットを構築し、importをbaseUrl込みで候補パスに解決、拡張子展開 | Med |
| Struct | CompilerOptions | pub | tsconfigのcompilerOptionsの必要部分（baseUrl, paths）を表現 | Low |
| Struct | TsConfig | pub | tsconfigの全体（extendsとcompilerOptions）を表現 | Low |
| Fn | parse_jsonc_tsconfig | pub | JSONC文字列からTsConfigへパース | Low |
| Fn | read_tsconfig | pub | ファイル読み込み＋JSONCパース | Low |
| Fn | resolve_extends_chain | pub | extends再帰解決、循環検出、親子マージ | Med |
| Fn | merge_tsconfig | private | 親→子の上書きルールに基づくマージ | Low |

### Dependencies & Interactions

- 内部依存
  - read_tsconfig → parse_jsonc_tsconfig
  - resolve_extends_chain → read_tsconfig, merge_tsconfig, resolve_extends_chain（再帰）
  - PathAliasResolver::from_tsconfig → PathRule::new
  - PathAliasResolver::resolve_import → PathRule::try_resolve
- 外部依存

| クレート/モジュール | 用途 |
|--------------------|------|
| serde::{Deserialize, Serialize} | TsConfig/CompilerOptionsのシリアライズ/デシリアライズ |
| json5 | JSONCのパース |
| regex | パターンマッチ用正規表現のコンパイル/マッチ |
| std::fs, std::path::{Path, PathBuf} | ファイル読み込み、パス操作 |
| std::collections::{HashMap, HashSet} | paths辞書、循環検出セット |
| crate::project_resolver::{ResolutionError, ResolutionResult} | 統一的な結果/エラー型 |

- 被依存推定
  - TypeScriptインポート解決やバンドラ/解析器（例: モジュールグラフ生成）からこのモジュールのPathAliasResolverが使われる。
  - プロジェクト設定読み込みフェーズでresolve_extends_chainとread_tsconfigが呼ばれる。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| parse_jsonc_tsconfig | fn parse_jsonc_tsconfig(content: &str) -> ResolutionResult<TsConfig> | JSONC文字列をTsConfigへ変換 | O(n) | O(n) |
| read_tsconfig | fn read_tsconfig(path: &Path) -> ResolutionResult<TsConfig> | ファイル読み込み＋JSONCパース | O(n) | O(n) |
| resolve_extends_chain | fn resolve_extends_chain(base_path: &Path, visited: &mut HashSet<PathBuf>) -> ResolutionResult<TsConfig> | extends連鎖を解決し最終TsConfigを返す | O(D + ΣM) | O(ΣK) |
| PathRule::new | fn new(pattern: String, targets: Vec<String>) -> ResolutionResult<PathRule> | TSパターンを正規表現にコンパイルする | O(P + T) | O(P + T) |
| PathRule::try_resolve | fn try_resolve(&self, specifier: &str) -> Option<String> | specifierに対し1ターゲットへ置換したパスを返す | O(|spec|) | O(|spec|) |
| PathAliasResolver::from_tsconfig | fn from_tsconfig(config: &TsConfig) -> ResolutionResult<PathAliasResolver> | pathsを特異度順にコンパイルしたルールセット生成 | O(R log R + Σcompile) | O(R) |
| PathAliasResolver::resolve_import | fn resolve_import(&self, specifier: &str) -> Vec<String> | エイリアスに一致する候補パスを返す | O(R · match) | O(C) |
| PathAliasResolver::expand_extensions | fn expand_extensions(&self, path: &str) -> Vec<String> | TS拡張子とindex変種を展開 | O(1) | O(1) |
| CompilerOptions | struct CompilerOptions { pub baseUrl: Option<String>, pub paths: HashMap<String, Vec<String>> } | tsconfigのcompilerOptions表現 | - | - |
| TsConfig | struct TsConfig { pub extends: Option<String>, pub compilerOptions: CompilerOptions } | tsconfigの全体表現 | - | - |
| PathRule | struct PathRule { pub pattern: String, pub targets: Vec<String>, /* regex等はprivate */ } | 1つのpathsエントリのコンパイル済み表現 | - | - |
| PathAliasResolver | struct PathAliasResolver { pub baseUrl: Option<String>, pub rules: Vec<PathRule> } | エイリアス解決器 | - | - |

説明（代表APIを詳細化）:

1) parse_jsonc_tsconfig
- 目的と責務: JSONC（コメント・末尾カンマ）を含む文字列からTsConfigへパース。
- アルゴリズム:
  1. json5::from_strを呼び出す。
  2. 失敗時はResolutionError::invalid_cacheにラップして返す。
- 引数:

| 名 | 型 | 説明 |
|----|----|------|
| content | &str | tsconfig.jsonの内容（JSONC許容） |

- 戻り値:

| 型 | 説明 |
|----|-----|
| ResolutionResult<TsConfig> | 成功時TsConfig、失敗時ResolutionError |

- 使用例:
```rust
let content = r#"{ "compilerOptions": { "baseUrl": ".", "paths": { "@/*": ["src/*"] } } }"#;
let cfg = parse_jsonc_tsconfig(content)?;
```
- エッジケース:
  - 空文字列や不正JSONC（コメント閉じ忘れ等）→ invalid_cacheで詳細メッセージ。

2) read_tsconfig
- 目的と責務: パスからファイル読み込み→parse_jsonc_tsconfigへ。
- アルゴリズム:
  1. std::fs::read_to_stringで読み込む。
  2. 失敗時はResolutionError::cache_ioを返す。
  3. parse_jsonc_tsconfigでパース。
- 引数:

| 名 | 型 | 説明 |
|----|----|------|
| path | &Path | tsconfigファイルのパス |

- 戻り値:

| 型 | 説明 |
|----|-----|
| ResolutionResult<TsConfig> | 読み込み＆パース結果 |

- 使用例:
```rust
let cfg = read_tsconfig(Path::new("tsconfig.json"))?;
```
- エッジケース:
  - 不存在/権限なし→ cache_io。

3) resolve_extends_chain
- 目的と責務: extendsを再帰的に辿り、親設定を子で上書きして最終構成に。
- アルゴリズム:
  1. base_pathをcanonicalize（失敗→cache_io）。
  2. visitedに含まれていれば循環→invalid_cache。
  3. read_tsconfigで現在の設定を取得。
  4. extendsがSomeの場合:
     - 絶対/相対を判定し、親パスを生成。
     - 拡張子なしなら.jsonを付与。
     - resolve_extends_chainで親を解決。
     - merge_tsconfigで親→子の順にマージ（子優先）。
  5. visitedから現在を除去して返却。
- 引数:

| 名 | 型 | 説明 |
|----|----|------|
| base_path | &Path | 起点のtsconfigパス |
| visited | &mut HashSet<PathBuf> | 循環検出用の訪問済み集合 |

- 戻り値:

| 型 | 説明 |
|----|-----|
| ResolutionResult<TsConfig> | マージ済み最終構成 |

- 使用例:
```rust
let mut visited = std::collections::HashSet::new();
let merged = resolve_extends_chain(Path::new("packages/web/tsconfig.json"), &mut visited)?;
```
- エッジケース:
  - extends循環、親ディレクトリ解決不可、拡張子の付与ロジック。

4) PathAliasResolver::from_tsconfig
- 目的と責務: pathsを「特異度（長い・ワイルドカード少）」順に並べ、PathRuleにコンパイル。
- アルゴリズム:
  1. config.compilerOptions.pathsのキーと値を収集。
  2. ソートキー: (-len(pattern), wildcard_count)。
  3. 各エントリをPathRule::newでコンパイル。
  4. baseUrlとrulesを保持。
- 引数/戻り値省略（表は上記参照）。
- 使用例:
```rust
let r = PathAliasResolver::from_tsconfig(&cfg)?;
```
- エッジケース:
  - 無効な正規表現→invalid_cache。
  - ターゲットが空→invalid_cache。

5) PathAliasResolver::resolve_import
- 目的と責務: import specifierに対し、マッチする全ルールから候補パス生成。
- アルゴリズム:
  1. ルールを順に試す。
  2. PathRule::try_resolveがSomeなら候補に追加。
  3. baseUrlがSomeなら「baseUrl/resolved」に連結（"."は特別扱いで未連結）。
- 使用例:
```rust
let candidates = r.resolve_import("@components/Button");
for c in candidates {
    // "./src/components/Button" など
}
```
- エッジケース:
  - 複数ルールが同時マッチ→候補が複数。
  - baseUrl末尾の"/"はトリム。

6) PathAliasResolver::expand_extensions
- 目的と責務: パスに.ts, .tsx, .d.ts, index.ts, index.tsxを展開。
- 使用例:
```rust
let expanded = r.expand_extensions("./src/components/Button");
// ["./src/components/Button", "./src/components/Button.ts", ...]
```

7) PathRule::new / try_resolve
- 目的と責務: TSの「@alias/*」パターンから正規表現を生成し、「*」部分を$1で置換。
- アルゴリズム（new）:
  1. pattern内の"*"を"(.*)"へ置換し、全体をregex::escapeでエスケープ。エスケープされた"(.*)"を元に戻す。
  2. "^...$"で全体一致。
  3. targets.first()をテンプレートに採用、"*"を"$1"に置換。
- 使用例:
```rust
let rule = PathRule::new("@components/*".into(), vec!["components/*".into()])?;
assert_eq!(rule.try_resolve("@components/Button"), Some("components/Button".into()));
```
- エッジケース:
  - ターゲットが複数でも最初の1つしか使わない。
  - パターンに"*"がない場合、テンプレートに"$1"が含まれていなければ置換不要。

データ契約（TsConfig/CompilerOptionsのJSON構造例）:
```json
{
  "extends": "./base.json",
  "compilerOptions": {
    "baseUrl": "./src",
    "paths": {
      "@components/*": ["components/*"],
      "@utils/*": ["utils/*"]
    }
  }
}
```

## Walkthrough & Data Flow

- ファイル→構成解決→エイリアス解決の全体フロー:
  1. read_tsconfigで読み込み＆parse_jsonc_tsconfig。
  2. extends連鎖がある場合はresolve_extends_chainで親から順に解決＆merge。
  3. PathAliasResolver::from_tsconfigでルールを構築（特異度順ソート）。
  4. resolve_importでspecifierから候補を生成し、expand_extensionsで拡張子展開。

Mermaidフローチャート（resolve_extends_chainの主要分岐）:
```mermaid
flowchart TD
    A[Start: base_path] --> B[canonicalize(base_path)]
    B -->|Err| E1[Return cache_io error]
    B --> C{visited.contains?}
    C -->|Yes| E2[Return invalid_cache: Circular extends]
    C -->|No| D[visited.insert(canonical)]
    D --> R[read_tsconfig(canonical)]
    R -->|Err| E3[Return read error]
    R --> F{config.extends is Some?}
    F -->|No| Z[visited.remove; Return config]
    F -->|Yes| G{Path::new(extends).is_absolute?}
    G -->|Yes| H[Parent = extends]
    G -->|No| I[Parent = canonical.parent().join(extends)]
    H --> J{parent.extension().is_none?}
    I --> J
    J -->|Yes| K[parent.with_extension("json")]
    J -->|No| L[parent]
    K --> M[resolve_extends_chain(parent, visited)]
    L --> M
    M -->|Err| E4[Return parent resolve error]
    M --> N[config = merge_tsconfig(parent, child)]
    N --> Z[visited.remove; Return merged]
```
上記の図は`resolve_extends_chain`関数の主要分岐を示す（行番号不明）。

## Complexity & Performance

- parse_jsonc_tsconfig: 時間 O(n)（入力サイズ）、空間 O(n)（デシリアライズ）。
- read_tsconfig: 時間 O(n)（ファイル長）、空間 O(n)。
- resolve_extends_chain: 時間 O(D + ΣM)（D=extends深さ、ΣM=各マージでのHashMap操作）、空間 O(ΣK)（pathsキー数の合計）。
- PathAliasResolver::from_tsconfig: 時間 O(R log R + Σcompile)（R=paths数、正規表現コンパイル）、空間 O(R)。
- PathAliasResolver::resolve_import: 時間 O(R · match)（各ルールでregexマッチ）、空間 O(C)（候補数）。
- PathAliasResolver::expand_extensions: 時間 O(1)、空間 O(1)。

ボトルネック:
- ルール数Rが多い大規模プロジェクトで、resolve_importが正規表現マッチをR回行うためレイテンシ増。
- resolve_extends_chainは深い継承チェーンでファイルIO＋再帰が連続する。

スケール限界/実運用負荷:
- 多層extendsチェーンではIOが支配的。
- pathsに多数のパターンがある場合、インポート解決頻度に比例してCPU使用が増える。
- ネットワーク/DBは登場しないが、ファイルシステムの性能に依存。

## Edge Cases, Bugs, and Security

エッジケース詳細表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| JSONC不正 | `{ invalid json }` | invalid_cacheで詳細メッセージ | parse_jsonc_tsconfig | 対応済 |
| ファイル不存在 | `/does/not/exist/tsconfig.json` | cache_ioでエラー | read_tsconfig | 対応済 |
| extends循環 | a.json→b.json→a.json | invalid_cache: Circular extends | resolve_extends_chain | 対応済 |
| 親ディレクトリなし | ルート直下でparent()がNone | invalid_cacheで説明 | resolve_extends_chain | 対応済 |
| ターゲット未指定 | `paths: {"@/*": []}` | invalid_cache「ターゲットなし」 | PathRule::new | 対応済 |
| 複数ターゲット | `["@a/*": ["x/*","y/*"]]` | 全ターゲットへ展開 | PathRule::new/resolve_import | 未対応（1件のみ） |
| パターンに"*"なし | `"@lib": ["lib"]` | 文字列一致でターゲットへ | try_resolve | 多分OK |
| baseUrl="." | `"."` | 連結せずに返す | resolve_import | 対応済 |
| Windowsパス区切り | baseUrl="src" | `src/...`をString連結 | resolve_import | 文字列連結のみ（PathBuf未使用） |
| index展開 | `components/Button` | `index.ts`, `index.tsx`追加 | expand_extensions | 対応済 |

バグ/改善ポイント:
- pathsターゲットの複数対応が欠落（PathRule::newがfirst()のみ採用）。仕様に沿うには全ターゲットを候補にすべき。
- 文字列連結でパスを構築しているためOS依存の区切りと正規化の問題（PathBuf推奨）。
- resolve_extends_chainでエラー発生時にvisited.removeが実行されないため、呼び出し側がvisitedを再利用すると残骸が残る可能性（再呼び出し設計に注意）。通常は単発呼び出しで問題は小。
- extendsの解決が相対/絶対パスのみで、npmパッケージ名解決（TypeScriptはnodeモジュール解決を許容）が非対応。

セキュリティチェックリスト:
- メモリ安全性: unsafe未使用。Buffer overflow/use-after-free/整数オーバーフローの懸念なし（標準String/HashMap使用）。
- インジェクション: SQL/Command/Path traversalのうち、パスは呼び出し側入力に依存。resolve_importはファイルアクセスをしないため直接のリスクは低い。read_tsconfigは任意pathを読むため、呼び出し側で入力検証が必要。
- 認証・認可: 該当なし。
- 秘密情報: ハードコード秘密なし。ログ漏洩の懸念は低いが、エラー詳細にパスが含まれるため取り扱いに注意。
- 並行性: グローバル共有状態なし。visitedは外部から渡されるmutable構造で、並列呼び出し時はスレッド毎に分離すべき。

## Design & Architecture Suggestions

- 複数ターゲット対応: PathRuleを「targets全て」をテンプレート化し、try_resolveで複数候補を返す設計に変更。もしくはPathAliasResolver側でtargetsを展開してルール化する。
- パス正規化: 文字列連結ではなくPathBufを用い、OSに依存しない正規化とjoinを行う。import specifierは`/`固定だがファイルアクセス時点でPathBufへ変換する。
- エラーハンドリング: resolve_extends_chainでvisited管理をRAII（drop guard）やスコープガードで確実に除去するパターンに変更。
- ルールマッチ高速化: 正規表現の代わりにグロブマッチや前方一致＋suffix抽出で高速化（特にRが大の場合）。正規表現は柔軟だが重い。
- extendsのパッケージ解決: TypeScript互換性を上げるなら、node_modulesやtsconfigパッケージ名の解決をオプションで追加。
- API戻り値: resolve_importはPathBufのVecを返し、expand_extensionsもPathBufで返す。最終的にファイル存在チェックまで内包する高レベルAPIを追加してもよい。

## Testing Strategy (Unit/Integration) with Examples

既存テストの網羅:
- JSONCパース（コメント/末尾カンマ）: parse_tsconfig_with_comments。
- 最小構成: parse_minimal_tsconfig。
- extendsフィールド存在: parse_tsconfig_with_extends。
- 不正JSONエラー: invalid_json_returns_error。
- ファイル読み込み（存在/不存在）: read_example_tsconfig_from_file, read_nonexistent_file_returns_error。
- 実プロジェクト/例のextends連鎖: resolve_real_extends_chain, resolve_with_extends_chain_real_files。
- パスエイリアス解決: resolve_path_aliases_with_real_tsconfig。
- 拡張子展開: expand_typescript_extensions。
- 循環検出: detect_circular_extends。

追加すべきテスト例:
- 複数ターゲット展開（改善後前提）:
```rust
let cfg = TsConfig {
    compilerOptions: CompilerOptions {
        baseUrl: Some("./src".into()),
        paths: HashMap::from([("@/*".into(), vec!["a/*".into(), "b/*".into()])]),
    },
    ..Default::default()
};
let r = PathAliasResolver::from_tsconfig(&cfg)?;
let c = r.resolve_import("@/x");
assert!(c.contains(&"./src/a/x".into()));
assert!(c.contains(&"./src/b/x".into()));
```
- 特異度ソートの検証（長い/ワイルドカード少の優先）:
```rust
let cfg = TsConfig {
    compilerOptions: CompilerOptions {
        paths: HashMap::from([
            ("@/registry/*".into(), vec!["reg/*".into()]),
            ("@/*".into(), vec!["root/*".into()])
        ]),
        ..Default::default()
    }, ..Default::default()
};
let r = PathAliasResolver::from_tsconfig(&cfg)?;
let c = r.resolve_import("@/registry/item");
assert!(c.first().unwrap().contains("reg/item")); // より特異的なルールが先行
```
- パターンに"*"なし:
```rust
let rule = PathRule::new("@lib".into(), vec!["lib".into()])?;
assert_eq!(rule.try_resolve("@lib"), Some("lib".into()));
assert_eq!(rule.try_resolve("@libx"), None);
```
- baseUrl="."の扱い:
```rust
let cfg = TsConfig {
    compilerOptions: CompilerOptions { baseUrl: Some(".".into()), paths: HashMap::from([("@/*".into(), vec!["src/*".into()])]) }
    , ..Default::default()
};
let r = PathAliasResolver::from_tsconfig(&cfg)?;
assert!(r.resolve_import("@/a").contains(&"src/a".into())); // 接頭辞不要
```
- エラー流儀の検証（無効な正規表現パターン）:
```rust
let bad = PathRule::new("(".into(), vec!["x".into()]);
assert!(bad.is_err());
```

## Refactoring Plan & Best Practices

- PathRuleをtargetsごとに複数テンプレートへリファクタリングし、try_resolveでVec<String>を返すか、PathAliasResolver側でtargetsを展開して複数ルール化する。既存API互換性を保つならPathAliasResolver::resolve_importで複数ターゲットを反映。
- パス連結をPathBufに置換し、`base.join(resolved)`で組み立て→`to_string_lossy()`は最終段階のみ使用。
- resolve_extends_chainのvisited管理をスコープガード化:
```rust
// 擬似コード
visited.insert(canonical.clone());
let guard = ScopeGuard::new(|| { visited.remove(&canonical); });
// ... 途中で早期returnしてもremoveが走る
```
- ルールマッチの最適化：ワイルドカード位置が末尾のケースが多いなら、prefixチェック＋suffix抽出でregexを回避。
- API設計：`expand_extensions`は拡張子リストを引数で受け取れるようにし拡張可能性を確保。
- エラー文言の国際化対応やログフレームワーク（log/tracing）基盤を導入し、println!の使用を避ける（テスト出力以外）。

## Observability (Logging, Metrics, Tracing)

- ログ:
  - 読み込み/パース失敗時にResolutionErrorへ十分な文脈が含まれているが、アプリ側で`log`/`tracing`のレベルに応じて記録できる形にするのが望ましい。
  - resolve_extends_chainの各ステップ（canonicalize、親パス算出、再帰呼び出し、マージ）に`trace!`を入れるとデバッグ容易。
- メトリクス:
  - ルール数R、resolve_importの呼び出し回数/平均時間/ヒット率（何件候補が返ったか）を計測。
  - extends深さD、ファイル読み込み回数。
- トレーシング:
  - import解決リクエスト毎にspanを作り、どのルールがマッチしたか、baseUrl適用有無などのタグを付与。

## Risks & Unknowns

- ResolutionResult/ResolutionErrorの正確な定義は外部モジュールのため詳細不明（このチャンクには現れない）。invalid_cache, cache_ioのバリアントは使用されているが他のバリアントの有無は不明。
- TypeScriptの正式なextends解決（npmパッケージ名、"tsconfig.node.json"等の慣習）への対応範囲は不明。現実的なプロジェクトでは必要になる可能性がある。
- 大量のpaths（Rが大）環境下での性能要件（閾値、期待レイテンシ）は不明。キャッシュ戦略や非同期化の必要性は要求次第。
- Windows環境におけるパスの扱い（区切り文字やcanonicalizeの挙動）に関する要件は不明。現在は文字列連結で`/`を使用。
- ルールの正規表現変換は「`*`→`(.*)`」の単純モデルに依存。`**`など高度なグロブ表現の要件は不明。現コードは対応していない。 

## Rust特有の観点（詳細チェックリスト）

- メモリ安全性
  - 所有権: read_tsconfig/parse_jsonc_tsconfig/resolve_extends_chainはOwned TsConfigを返却。参照のライフタイムは関数スコープに限定され安全。
  - 借用: resolve_extends_chainは`&mut HashSet<PathBuf>`を可変借用し、関数内で挿入・削除。スコープ外へ参照を保持しない。
  - ライフタイム: 明示的なライフタイムパラメータ不要。返り値はOwned。
- unsafe境界
  - 使用箇所: なし。
  - 不変条件/安全性根拠: 標準ライブラリと安全なクレート（serde/json5/regex）利用のため安全。
- 並行性・非同期
  - Send/Sync: 明示的な境界は現れないが、保持する型（String, Vec, HashMap, Regex）によりインスタンスのSend/Syncは型ごとに異なる。regex::RegexはSync+Send（2024時点）だが、本チャンクでは明示していない。
  - データ競合: 共有可変状態なし（visitedは呼び出し側で管理）。
  - await境界/キャンセル: 非同期処理なし。
- エラー設計
  - Result vs Option: 構文/IO/正規表現コンパイル失敗はResult。マッチ有無はOption（try_resolve）で適切に表現。
  - panic箇所: 本体コードにunwrap/expectなし（テストのみ使用）。健全。
  - エラー変換: json5/regex/IOエラーをResolutionError（invalid_cache/cache_io）に変換。From/Intoの実装はこのチャンクには現れない。

以上の観点は関数名を根拠としたものであり、行番号はこのチャンクでは不明。