# typescript.rs Review

## TL;DR

- 目的: TypeScript の tsconfig.json を基に、パスエイリアス解決用のルールを構築・永続化し、影響範囲のファイルを選定するプロバイダの実装。
- 主要公開API: ProjectResolutionProvider トレイト実装（language_id, is_enabled, config_paths, compute_shas, rebuild_cache, select_affected_files）と補助API（TypeScriptProvider::new, get_resolution_rules_for_file, TsConfigPath の new/as_path）。
- コアロジック: rebuild_cache が tsconfig の extends チェーンを解決し、baseUrl と paths を ResolutionRules に格納して永続化。compute_shas は tsconfig ファイルの SHA を計算。
- 安全性/エラー: unsafe なし。I/O 例外は Result で伝播（rebuild_cache, compute_shas）／get_resolution_rules_for_file はエラーを None に潰す。TOCTOU（exists→read）の可能性あり。
- 並行性: 共有状態の実フィールド（memo）は未使用。永続化層の同時アクセス・ファイル競合の保護はこのファイルでは不明。
- 既知の制約/リスク: select_affected_files はヒューリスティック（固定パターンのみ）。tsconfig の include/exclude や複雑な paths マッピング解決は未実装。Windows パス区切りやグロブ正規化は不明。
- テスト: 基本機能をカバー（有効/無効、パス抽出、SHA 計算、非存在ファイルスキップ、キャッシュ再構築、影響ファイル選定）。エラー/競合/循環 extends のテストは不足。

## Overview & Purpose

このファイルは TypeScript 用の「プロジェクト解決プロバイダ」を提供します。目的は:

- tsconfig.json から baseUrl と paths を抽出し、解決ルールを構築
- tsconfig の extends チェーンを解決して「有効設定」を得る
- ファイルハッシュ（SHA）に基づく再構築の無効化判定を行う
- 影響範囲のファイル/ディレクトリを選ぶ（Sprint 1 の簡易版）
- 解決ルールとインデックスを .codanna 下に永続化/読み出しする

利用者は ProjectResolutionProvider トレイトを通じ、言語ごとの解決機能を抽象的に扱えます。TypeScriptProvider はその TypeScript 実装です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | TsConfigPath | pub | tsconfig.json のパスを型安全に扱う newtype | Low |
| Struct | TypeScriptProvider | pub | TypeScript 解決プロバイダ本体。キャッシュ・永続化・ルール生成 | Med |
| Trait Impl | impl ProjectResolutionProvider for TypeScriptProvider | pub（trait経由） | Provider 共通APIの TypeScript 実装 | Med |
| Function | TypeScriptProvider::new | pub | Provider の生成（メモ初期化） | Low |
| Function | TypeScriptProvider::get_resolution_rules_for_file | pub | 指定ファイルに適用される tsconfig ベースの解決ルール取得 | Low |
| Function | compute_shas（trait実装） | pub（trait経由） | tsconfig ファイルの SHA 計算 | Low |
| Function | rebuild_cache（trait実装） | pub（trait経由） | tsconfig 解析・extends 解決・ルール永続化・影響マッピング更新 | Med |
| Function | select_affected_files（trait実装） | pub（trait経由） | 簡易な影響ファイル選定（ヒューリスティック） | Low |
| Tests | mod tests | - | 単体テスト（機能のサブセットを検証） | Low |

Dependencies & Interactions
- 内部依存
  - TypeScriptProvider::config_paths → extract_config_paths
  - get_resolution_rules_for_file → ResolutionPersistence::load → index.get_config_for_file → index.rules.get
  - rebuild_cache → ResolutionPersistence::{load, save} → index.needs_rebuild, index.update_sha, index.set_rules, index.add_mapping
  - rebuild_cache → crate::parsing::typescript::tsconfig::resolve_extends_chain
  - compute_shas → compute_file_sha
- 外部依存（主なもの）

| 依存 | 用途 | 備考 |
|------|------|------|
| crate::config::Settings | 言語設定の取得 | languages["typescript"] |
| ProjectResolutionProvider | Provider 共通 API | このトレイトへ適合 |
| ResolutionPersistence | ルール・インデックスの永続化 | .codanna ディレクトリ配下 |
| ResolutionRules | baseUrl と paths の契約 | tsconfig を転写 |
| compute_file_sha | ファイルの SHA 計算 | I/O 発生 |
| crate::parsing::typescript::tsconfig::resolve_extends_chain | tsconfig の extends 解決 | 外部実装（詳細不明） |

- 被依存推定
  - プロジェクト全体の「解決マネージャ」や「パス解決器」が、この provider を通じて TypeScript のルールにアクセス
  - 解析・リント・ジャンプ機能などが get_resolution_rules_for_file を用いて alias 解決に利用

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| TypeScriptProvider::new | fn new() -> Self | Provider インスタンスの生成 | O(1) | O(1) |
| TypeScriptProvider::get_resolution_rules_for_file | fn get_resolution_rules_for_file(&self, file_path: &Path) -> Option<ResolutionRules> | ファイルに適用される解決ルール取得 | O(1)〜O(log N) | O(1) |
| language_id | fn language_id(&self) -> &'static str | 言語 ID を返す | O(1) | O(1) |
| is_enabled | fn is_enabled(&self, settings: &Settings) -> bool | 設定で TypeScript が有効か判定 | O(1) | O(1) |
| config_paths | fn config_paths(&self, settings: &Settings) -> Vec<PathBuf> | 設定から tsconfig パス一覧を抽出 | O(K) | O(K) |
| compute_shas | fn compute_shas(&self, configs: &[PathBuf]) -> ResolutionResult<HashMap<PathBuf, Sha256Hash>> | tsconfig ファイルの SHA を計算 | O(K + Σ|Fi|) | O(K) |
| rebuild_cache | fn rebuild_cache(&self, settings: &Settings) -> ResolutionResult<()> | tsconfig の解析・ルール更新・永続化 | O(K + 解析コスト) | O(K) |
| select_affected_files | fn select_affected_files(&self, settings: &Settings) -> Vec<PathBuf> | 影響ファイル/ディレクトリの簡易選定 | O(K) | O(K) |
| TsConfigPath::new | fn new(path: PathBuf) -> Self | 型安全な tsconfig パス生成 | O(1) | O(1) |
| TsConfigPath::as_path | fn as_path(&self) -> &PathBuf | 内部パス参照の取得 | O(1) | O(1) |

詳細

1) TypeScriptProvider::new
- 目的と責務
  - Provider のインスタンス化（内部メモの初期化）。現状メモは未使用。
- アルゴリズム
  - ResolutionMemo::new() を呼び出し、構造体に格納。
- 引数
  - なし
- 戻り値
  - 新しい TypeScriptProvider
- 使用例
```rust
let provider = TypeScriptProvider::new();
```
- エッジケース
  - 特になし

2) TypeScriptProvider::get_resolution_rules_for_file
- 目的と責務
  - 指定ファイルに対応する tsconfig をインデックスから見つけ、その ResolutionRules(baseUrl, paths) を返す。
- アルゴリズム
  1. .codanna ディレクトリを基準に ResolutionPersistence を生成
  2. "typescript" 名前空間のインデックスを load（失敗は None にフォールド）
  3. index.get_config_for_file(file_path) で該当 tsconfig を探索
  4. 見つかった tsconfig に対応する rules を返す（clone）
- 引数

| 引数 | 型 | 説明 |
|------|----|------|
| file_path | &std::path::Path | 対象となるソースファイルのパス |

- 戻り値

| 型 | 説明 |
|----|------|
| Option<ResolutionRules> | 見つかればルール、なければ None。ロード失敗も None。|

- 使用例
```rust
let rules_opt = provider.get_resolution_rules_for_file(std::path::Path::new("src/main.ts"));
if let Some(rules) = rules_opt {
    // rules.base_url, rules.paths を使用して解決
}
```
- エッジケース
  - 永続化インデックスが存在しない/壊れている場合 → None
  - 対応する tsconfig が見つからない場合 → None
  - ルールが未設定（設定なし） → None

3) language_id
- 目的と責務
  - Provider が表す言語識別子を返す（"typescript"）。
- アルゴリズム/引数/戻り値
  - 単純返却。
- 使用例
```rust
assert_eq!(provider.language_id(), "typescript");
```

4) is_enabled
- 目的と責務
  - Settings.languages["typescript"] の enabled を確認。未設定ならデフォルトで true。
- 使用例
```rust
let enabled = provider.is_enabled(&settings);
```
- エッジケース
  - languages に "typescript" が無い → true を返す（デフォルト有効）

5) config_paths
- 目的と責務
  - 設定から TypeScript の tsconfig パスのリストを抽出し PathBuf として返す。
- アルゴリズム
  1. languages["typescript"] があれば config_files を TsConfigPath に包んで Vec に集める
  2. 取り出して PathBuf に戻す（トレイト互換）
- 使用例
```rust
let configs = provider.config_paths(&settings);
```

6) compute_shas
- 目的と責務
  - 引数の各パスについて、存在するファイルの SHA256 を計算し HashMap に格納。
- アルゴリズム
  1. ループで exists() を確認
  2. 存在すれば compute_file_sha()? で計算し挿入
  3. 例外は Result で伝播
- 使用例
```rust
let shas = provider.compute_shas(&configs)?;
```
- エッジケース
  - 非存在パス → スキップ（エラーなし）
  - 読込権限なし/TOCTOU → compute_file_sha が Err の場合は全体が Err（注意点）

7) rebuild_cache
- 目的と責務
  - 設定から tsconfig を取得し、各 tsconfig ごとに extends チェーンを解決してルールを更新。SHA に基づく再構築最小化と永続化。
- アルゴリズム
  1. config_paths を取得
  2. ResolutionPersistence::load("typescript")? でインデックス読み込み（なければ新規？：外部仕様依存）
  3. 各 config_path について:
     - ファイル存在チェック
     - compute_file_sha で SHA 計算
     - index.needs_rebuild(config_path, &sha) なら:
       - visited Set を用意し resolve_extends_chain(config_path, &mut visited)?
       - index.update_sha
       - index.set_rules(base_url, paths)
       - 親ディレクトリがあれば、"parent/**/*.ts" と "parent/**/*.tsx" のパターンを index.add_mapping
  4. 最後に persistence.save("typescript", &index)?
- 使用例
```rust
provider.rebuild_cache(&settings)?;
```
- エッジケース
  - tsconfig が非存在 → スキップ
  - extends の循環 → resolve_extends_chain が Err（結果として rebuild_cache が Err）
  - JSON 壊れ → Err

8) select_affected_files
- 目的と責務
  - 簡易ヒューリスティックで、影響を受けるであろうディレクトリ/ファイルを返す（Sprint 1）。
- アルゴリズム
  - ルート tsconfig.json なら ["src", "lib", "index.ts"]
  - それ以外は config の親ディレクトリ
- 使用例
```rust
let affected = provider.select_affected_files(&settings);
```
- エッジケース
  - Windows パス/相対パスの扱いは外部要因に依存

9) TsConfigPath::{new, as_path}
- 目的
  - 型安全な tsconfig パス管理
- 使用例
```rust
let tp = TsConfigPath::new(PathBuf::from("tsconfig.json"));
let pb: &PathBuf = tp.as_path();
```

データ契約
- ResolutionRules
  - base_url: Option<PathBuf> または PathBuf（このチャンクでは実体は不明だが、コードでは所有権で格納）
  - paths: 型不明（tsconfig.compilerOptions.paths の構造。通常は HashMap<String, Vec<String>> に相当）
- ResolutionPersistence インデックス
  - rules: HashMap<config_path, ResolutionRules>
  - SHA の追跡と needs_rebuild 判定
  - get_config_for_file: ファイルから適用 tsconfig を特定（グロブ/マッピング基準は外部実装）

## Walkthrough & Data Flow

典型フロー（rebuild_cache 経由でルール更新→クライアントは get_resolution_rules_for_file で参照）:

1. 設定ロード（外部）
2. provider.config_paths(settings) で tsconfig パス列挙
3. provider.rebuild_cache(settings)
   - .codanna から index をロード
   - 各 tsconfig について:
     - SHA を計算し needs_rebuild を判定
     - 必要なら resolve_extends_chain で有効 tsconfig を得て、
       - baseUrl/paths を取り出し index.set_rules
       - 親ディレクトリから "*.ts"/"*.tsx" マッピングを index.add_mapping
   - save で index を保存
4. クライアントは provider.get_resolution_rules_for_file(file) を呼び
   - index.load → get_config_for_file → rules.get().cloned()
5. パス解決器は rules.base_url と rules.paths で import パスを実ファイルに解決

データの流れの要点:
- 入力: Settings → tsconfig パス → tsconfig 内容（JSON）
- 中間: SHA・rules・マッピングを Index に蓄積
- 出力: ResolutionRules（baseUrl, paths）/ 影響範囲

このチャンクには、resolve_extends_chain の詳細と index の内部構造は現れないため、不明。

## Complexity & Performance

- config_paths: O(K) 時間・O(K) 空間（K は tsconfig の個数）
- compute_shas: O(K + Σ|Fi|) 時間（|Fi| は各ファイルサイズ）、O(K) 空間
- rebuild_cache:
  - ループ O(K)
  - 各 tsconfig 解析コストは JSON サイズ・extends の深さ D とする
  - 概ね O(K + Σ(解析Fi) + ΣD)
- get_resolution_rules_for_file: インデックス参照 O(1)〜O(log N)（内部構造次第）
- select_affected_files: O(K)

ボトルネック:
- 大規模モノレポで tsconfig が多い/extends が深いと rebuild_cache の I/O とパースが支配的
- compute_shas はファイルサイズに線形

スケール限界:
- 単一スレッドで逐次パース。大量プロジェクトでは並列化（Rayon 等）や差分再構築が望ましい
- グロブマッピング（"**/*.ts", "**/*.tsx"）は粗く、精度と性能の両面で改善余地あり

I/O負荷:
- ディスク I/O（SHA 計算、JSON 読み、インデックス保存）が中心
- ネットワーク I/O はなし

## Edge Cases, Bugs, and Security

セキュリティチェックリスト
- メモリ安全性
  - unsafe なし（このチャンクには現れない）。所有権/借用は標準的。大きな問題は見当たらない。
- インジェクション
  - SQL/Command/Path traversal: 外部からのパスは Settings に依存。永続化先は固定「.codanna」。パス連結は親ディレクトリに基づくため、通常は安全。任意の untrusted 設定に対する防御は要件次第。
- 認証・認可
  - 該当なし（ローカルツール想定）
- 秘密情報
  - ハードコード秘密なし。ログ出力もないため漏えいの懸念は少ない。
- 並行性
  - 共有メモは未使用。永続化の同時アクセス/レース（load/save）対策はこのチャンクでは不明（ファイルロックなしなら競合の可能性あり）。

潜在的バグ/仕様懸念
- TOCTOU in compute_shas: exists() → compute_file_sha() の間に削除されると Err で全体が失敗する可能性（部分成功にフォールバックしない）。
- get_resolution_rules_for_file のエラーサプレッション: persistence.load("typescript").ok()? により、ロード失敗も None に変換。原因追跡が困難。
- select_affected_files のヒューリスティック: "src", "lib", "index.ts" 固定で過不足があり、モノレポ/カスタム構成で外す可能性。
- パス正規化/OS 差異: display() を文字列で結合してグロブを生成。Windows の区切り/大文字小文字や UNC パスでの挙動は不明。
- 解析範囲の限定: baseUrl/paths 以外（include/exclude, rootDirs, composite, references 等）未対応。

エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 非存在 tsconfig | "/no/tsconfig.json" | スキップ/エラーなし | compute_shas: スキップ, rebuild_cache: スキップ | OK |
| exists→削除レース | ファイル存在→直後に削除 | そのファイルのみ無視して続行 | compute_shas: Err で全体失敗 | 要修正 |
| 壊れた tsconfig JSON | "{"compilerOptions": ..." | エラーで rebuild_cache が失敗 | resolve_extends_chain? に依存（Err 伝播） | 要確認 |
| 循環 extends | A extends B, B extends A | 循環検出し Err | visited を渡しているため可能性高いが外部実装依存 | 要確認 |
| paths 未設定 | compilerOptions.paths なし | 空または None として扱う | set_rules にそのまま転写 | OK |
| Windows パス | C:\repo\tsconfig.json | 正常なグロブ生成 | display + フォーマットの正当性不明 | 要確認 |
| 競合保存 | 複数スレッドで rebuild_cache | 一貫した index 保存 | ファイルロック不明 | 要検討 |
| インデックス未作成 | 初回 get_resolution_rules_for_file | None だが警告は出ると良い | None を返すのみ | 改善可 |

根拠（関数名:行番号）
- unsafe なし: このチャンクには unsafe ブロックは現れない（行番号:不明）
- exists→sha: compute_shas 内の exists チェック→compute_file_sha 呼び出し（行番号:不明）
- エラーサプレッション: get_resolution_rules_for_file 内の load(...).ok()?（行番号:不明）

## Design & Architecture Suggestions

- エラー設計
  - get_resolution_rules_for_file を Result<Option<ResolutionRules>> にし、ロード障害と未存在を区別。ログで原因提示。
  - compute_shas は個々のファイルエラーを無視して続行し、Vec<(PathBuf, Error)> の警告を返せる API を検討。
- マッピング精度
  - tsconfig の include/exclude、rootDir(s)、references を考慮して index.add_mapping を生成。現行の "**/*.ts(x)" は過剰。
- パス解決エンジン
  - paths のワイルドカード（"*" や "/*"）を Node/TS の解決仕様に従って展開するレイヤを導入（現状は永続化のみ）。
- 並行性と永続化
  - ResolutionPersistence にファイルロック/ジャーナリング導入。複数プロセス/スレッドの同時 rebuild_cache に耐える。
  - 冪等な save（テンポラリ→アトミックリネーム）で破損防止。
- パフォーマンス
  - compute_shas/rebuild_cache の並列化（Rayon）。SHA は I/O バウンドゆえ有効。
  - 長い extends チェーンのメモ化（この Struct の memo を活用）。
- クロスプラットフォーム
  - パス正規化（標準化された区切り、ケース感度のポリシー）とグロブの OS 互換性検証。
- 可観測性
  - tracing で info/debug/error を適切に出力。どの tsconfig を再構築したか、スキップ理由、解析時間などを計測。

## Testing Strategy (Unit/Integration) with Examples

追加で欲しいテスト
- エラー分岐
  - compute_shas: exists→削除のレースを模擬し、部分成功にする改修後に検証
  - get_resolution_rules_for_file: 永続化破損時の挙動（Result 化後）
- tsconfig 特性
  - extends チェーン（多段/循環）: resolve_extends_chain が循環検出するか
  - baseUrl/paths の各種パターン（ワイルドカード、相対/絶対、相互依存）
- パス/OS 差異
  - Windows/Unix の区切り、ドライブ文字を含むパス
- 競合/同時実行
  - 2 スレッドで rebuild_cache を同時実行 → index の整合性を確認（永続化にロックを導入後）

例: get_resolution_rules_for_file の Result 化後のテスト（案）
```rust
#[test]
fn rules_returns_error_when_index_corrupted() {
    let provider = TypeScriptProvider::new();
    // 事前に .codanna/typescript インデックスを意図的に壊す処理を用意（ヘルパ）
    corrupt_typescript_index();

    let res = provider.get_resolution_rules_for_file(Path::new("src/main.ts"));
    assert!(res.is_err(), "壊れたインデックスは Err で返すべき");
}
```

例: compute_shas の部分成功テスト（改修案）
```rust
#[test]
fn compute_shas_continues_on_individual_errors() {
    let provider = TypeScriptProvider::new();
    let temp_dir = tempfile::tempdir().unwrap();
    let good = temp_dir.path().join("good.json");
    let bad = temp_dir.path().join("bad.json");
    std::fs::write(&good, "{}").unwrap();
    // bad は存在→直後に削除のレースを模擬
    std::fs::write(&bad, "{}").unwrap();
    std::fs::remove_file(&bad).unwrap();

    let (shas, warnings) = provider.compute_shas_partial(&[good.clone(), bad.clone()]).unwrap();
    assert!(shas.contains_key(&good));
    assert!(warnings.iter().any(|(p, _)| p == &bad));
}
```

例: select_affected_files の精度テスト（パッケージ構成）
```rust
#[test]
fn affected_files_for_package_tsconfig_is_parent_dir() {
    let provider = TypeScriptProvider::new();
    let settings = create_test_settings_with_ts_config(vec![
        PathBuf::from("packages/web/tsconfig.json"),
    ]);
    let affected = provider.select_affected_files(&settings);
    assert!(affected.iter().any(|p| p.ends_with("packages/web")));
}
```

## Refactoring Plan & Best Practices

- インターフェース
  - get_resolution_rules_for_file を Result<Option<...>> に変更し、詳細なエラーを返却。呼び出し側でログ/リトライ可。
- I/O ロバスト性
  - compute_shas: exists チェックをやめ、compute_file_sha のエラーを個別収集しつつ続行する API へ（部分成功）。
- 役割の明確化
  - select_affected_files のヒューリスティックを設定駆動へ（設定の include/exclude パターン or インデックス由来のマッピングを使用）。
- メモ化の活用
  - TypeScriptProvider.memo を extends 解決結果や SHA キャッシュに利用。ファイル監視（notify）と組み合わせて増分更新。
- パス処理ユーティリティ
  - display() ベースではなく Path/PathBuf を保持したままグロブを生成する関数を用意（OS 依存を吸収）。
- ドメインモデルの強化
  - ResolutionRules の型を厳密化（paths の型、安全なワイルドカード処理ユーティリティ、正規化済みパス）。
- ドキュメンテーション
  - 仕様（TS のパス解決ルール）への準拠度と差分をモジュールドキュメントに明示。

## Observability (Logging, Metrics, Tracing)

- ログ（tracing）
  - level=info: rebuild_cache の開始/終了、再構築した tsconfig の一覧
  - level=warn: 個別ファイルの SHA 計算失敗、インデックスロード失敗（get_resolution_rules_for_file）
  - level=debug: needs_rebuild の判定理由（旧 SHA と新 SHA）
```rust
use tracing::{info, warn, debug};

fn rebuild_cache(&self, settings: &Settings) -> ResolutionResult<()> {
    info!("Rebuilding TypeScript resolution cache");
    /* ... */
    if index.needs_rebuild(config_path, &sha) {
        debug!(config=?config_path, "Rebuilding due to SHA change");
        /* ... */
    }
    /* ... */
    info!("Saved TypeScript resolution index");
    Ok(())
}
```
- メトリクス
  - 再構築にかかった時間（ヒストグラム）
  - 解析した tsconfig 数、スキップ数、エラー数
- トレーシング
  - resolve_extends_chain に span を付け、深さや参照数を記録

## Risks & Unknowns

- ResolutionPersistence の内部
  - needs_rebuild 判定の基準、インデックスの形式、原子的保存の有無は不明
- resolve_extends_chain の仕様
  - 循環検出・相対/絶対パス解決・エラー詳細は不明
- ResolutionRules の正確な型
  - base_url と paths の具体的な型や正規化状態はこのチャンクには現れない
- 並行実行時の安全性
  - ファイルロックや同時 save の衝突回避の仕組みが不明
- クロスプラットフォーム
  - Windows でのグロブ生成/一致精度が不明
- 将来のメモ使用
  - memo フィールドは未使用。用途と整合性は未確定

以上を踏まえ、現状は Sprint 1 の要件（基本的なパスエイリアス解決のためのルール生成と永続化）を満たしつつ、堅牢性・可観測性・精度の観点で拡張余地が大きい実装となっています。

【Rust特有の観点】
- 所有権
  - extract_config_paths で Settings 内の PathBuf を clone して TsConfigPath を生成（所有権移動の副作用なし、行番号:不明）
  - get_resolution_rules_for_file は index.rules.get(...).cloned() で値を複製し、ライフタイム問題を回避（行番号:不明）
- 借用/ライフタイム
  - メソッドは基本的に &self かつ所有データを返却（または clone）しており、明示的ライフタイムは不要
- unsafe 境界
  - なし（このチャンクには現れない）
- 並行性・非同期
  - 非同期なし。共有可変状態は実質なし（memo 未使用）。Send/Sync 境界は型定義上不明だが、現行の使用では問題化しない想定
- エラー設計
  - Result と Option の使い分けは概ね妥当だが、get_resolution_rules_for_file のエラー潰しは改善余地
  - panic/unwrap/expect の使用は本体なし（テストのみ）