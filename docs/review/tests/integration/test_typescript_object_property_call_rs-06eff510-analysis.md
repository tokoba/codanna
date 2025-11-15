# integration\test_typescript_object_property_call.rs Review

## TL;DR

- 目的: TypeScriptの「オブジェクトプロパティ名が関数名と同じ」ケースで、プロパティに束縛された無名関数から同名のトップレベル関数への呼び出しがコールグラフに正しく記録されるかを検証。基礎比較として通常の関数間呼び出しも検証。
- 主要API: codanna::SimpleIndexer の with_settings, index_file, find_symbols_by_name, get_calling_functions_with_metadata を使用。
- 複雑箇所: 同名シンボル（submitForm）が複数存在する状況で「どのシンボルを指すか」をテスト側が最初の要素に依存している点が脆弱。呼び出し関係の追跡が失敗する既知バグの再現。
- 重大リスク: 行番号の境界条件（0/1-index）とシンボル種別フィルタ不足によりテストの安定性が損なわれる可能性。内部実装不明のためパフォーマンス・整合性は推定。
- Rust安全性: unsafeなし。Arcによる設定共有のみ。unwrap/expectはテスト文脈では許容も、本番コードでは非推奨。
- セキュリティ: I/OはTempDirのみで影響限定。インジェクション・権限・秘密情報の懸念は該当なし。
- 改善提案: シンボル種別フィルタ導入、ヘルパー抽出、行番号検証の厳密化、追加ユースケースのテスト拡充。

## Overview & Purpose

このファイルは、codanna の TypeScriptコードインデクサが関数呼び出しの関係性（caller/callee）を正しく追跡できるかを統合テストで検証します。特に、オブジェクトリテラルのプロパティ名が既存の関数名と一致し、そのプロパティに束縛された無名関数（async arrow）が同名のトップレベル関数を呼ぶケースで、呼び出し関係が記録されない既知のバグを再現・検証します。また、通常の関数が別の関数を呼ぶ基礎的なケースが正しく動作することを確認します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function (test) | test_object_property_calls_same_named_function | テスト限定（pubではない） | 同名の関数・プロパティ関数の呼び出し関係が追跡されるべきことを検証（既知のバグ再現） | Med |
| Function (test) | test_regular_function_call_works | テスト限定（pubではない） | 通常の関数→関数呼び出しが追跡されることの基準確認 | Low |
| 外部型 | SimpleIndexer | 外部クレート（codanna） | ソースコードのインデックス作成・シンボル検索・関係取得 | 不明 |
| 外部型 | Settings | 外部クレート（codanna） | インデクサ設定（workspace_rootなど） | 低 |
| 外部型 | TempDir | 外部クレート（tempfile） | 一時ディレクトリ管理 | 低 |

### Dependencies & Interactions

- 内部依存:
  - 両テスト関数は共通の流れ（TempDir作成 → TSコードを書き出し → Settings生成 → SimpleIndexer初期化 → index_file → find_symbols_by_name → get_calling_functions_with_metadata → assert）を使用。
- 外部依存（推奨表）:

| クレート/モジュール | 用途 |
|--------------------|------|
| codanna::SimpleIndexer | インデックス作成と検索、関係取得 |
| codanna::config::Settings | インデクサ設定（workspace_root） |
| tempfile::TempDir | テスト用一時ディレクトリ |
| std::fs | テストファイル書き出し |
| std::sync::Arc | 設定共有（所有権管理） |

- 被依存推定:
  - Cargoのテストランナー（cargo test）から実行される統合テスト。モジュール外からこのファイルの関数が直接呼ばれることはない。

## API Surface (Public/Exported) and Data Contracts

このファイルから外部に公開されるAPIはありません（テスト関数のみ）。ここでは「このテストが使用する外部API（codanna）」の一覧を示します。正確なシグネチャはこのチャンクには現れないため、使用状況からの推定を併記します。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| SimpleIndexer::with_settings | 不明（使用から Arc<Settings> を受け取り SimpleIndexer を返すと推定） | 設定付きインデクサ生成 | O(1) 推定 | O(1) 推定 |
| SimpleIndexer::index_file | 不明（&str パスを取り Result を返すと推定） | 指定ファイルをインデックス化 | O(n) 推定（n=ファイル長） | O(s) 推定（s=シンボル数） |
| SimpleIndexer::find_symbols_by_name | 不明（&str 名称と Option<&str> 言語を取り Vec<Symbol> を返すと推定） | 名前一致のシンボル検索 | O(m) 推定（m=総シンボル数） | O(k)（k=一致数） |
| SimpleIndexer::get_calling_functions_with_metadata | 不明（シンボルIDを取り Vec<(Symbol, Metadata)> を返すと推定） | 指定シンボルを呼ぶ関数の取得 | O(r) 推定（r=関係数） | O(r) |

各APIの詳細説明（推定ベース）

1) SimpleIndexer::with_settings
- 目的と責務: 設定を適用したインデクサを生成。
- アルゴリズム: 設定構造体を所有または参照し、インデクサ内部状態を初期化。
- 引数:

| 名前 | 型 | 説明 |
|------|----|------|
| settings | Arc<Settings> | ワークスペースルート等を含む設定 |

- 戻り値:

| 型 | 説明 |
|----|------|
| SimpleIndexer | インデクサ本体（推定） |

- 使用例:
```rust
let settings = Settings { workspace_root: Some(temp_dir.path().to_path_buf()), ..Default::default() };
let mut indexer = SimpleIndexer::with_settings(Arc::new(settings));
```
- エッジケース:
  - 設定不備（workspace_rootが無効）: このチャンクには現れない。

2) SimpleIndexer::index_file
- 目的と責務: 単一ファイルを解析してシンボルと関係を構築。
- アルゴリズム（推定）:
  1. ファイルを読み込み
  2. 言語判定（拡張子.ts）
  3. パース（TS AST）
  4. シンボル抽出・関係構築（関数定義・呼び出し等）
- 引数:

| 名前 | 型 | 説明 |
|------|----|------|
| path | &str | インデックス対象ファイルパス |

- 戻り値:

| 型 | 説明 |
|----|------|
| Result<_, _> | 成否の結果（具象型は不明） |

- 使用例:
```rust
indexer.index_file(test_file.to_str().unwrap()).expect("Failed to index file");
```
- エッジケース:
  - パース不能ファイル: expectによりテストは失敗。

3) SimpleIndexer::find_symbols_by_name
- 目的と責務: 名前でシンボルを検索。言語フィルタ付き。
- アルゴリズム（推定）: 内部インデックスから名前一致でフィルタし返却。
- 引数:

| 名前 | 型 | 説明 |
|------|----|------|
| name | &str | 検索するシンボル名 |
| language | Option<&str> | 言語フィルタ（Some("typescript")） |

- 戻り値:

| 型 | 説明 |
|----|------|
| Vec<Symbol-like> | シンボルの配列（kind, range.start_line, id, name を持つ構造と推定） |

- 使用例:
```rust
let submit_form_symbols = indexer.find_symbols_by_name("submitForm", Some("typescript"));
```
- エッジケース:
  - 重複名（プロパティ関数と通常関数）→ 複数ヒット。最初の要素依存は危険。

4) SimpleIndexer::get_calling_functions_with_metadata
- 目的と責務: 指定シンボル（callee）を呼ぶ関数（caller）一覧を取得。
- アルゴリズム（推定）: 関係グラフから逆引き取得。
- 引数:

| 名前 | 型 | 説明 |
|------|----|------|
| callee_id | ID型（不明） | 対象シンボルの内部ID |

- 戻り値:

| 型 | 説明 |
|----|------|
| Vec<(Symbol-like, Metadata-like)> | 呼び手シンボルと付随メタデータ |

- 使用例:
```rust
let callers = indexer.get_calling_functions_with_metadata(submit_form.id);
```
- エッジケース:
  - バグ再現ケースでは空配列が返る（本来は少なくとも1要素を期待）。

## Walkthrough & Data Flow

両テストとも同じ基本データフローです。

1) test_object_property_calls_same_named_function
- TypeScriptコード生成（同名関数・プロパティ関数）:
```typescript
// Bug reproduction: object property name matches function name
// codanna should detect that actions.submitForm calls submitForm()

function submitForm(data: any) {
    return { success: true, data };
}

const actions = {
    // Property name 'submitForm' matches function name
    // This call should be detected as a caller
    submitForm: async (request: any) => {
        return submitForm({ input: request.body });
    }
};
```
- TempDirにファイル書き込み
- Settingsをworkspace_rootにTempDirを指定
- SimpleIndexerを設定付きで初期化
- index_fileでインデックス化
- submitFormのシンボル検索（同名の複数候補が想定されるため、本来は種別フィルタが望ましいが現実装は先頭を選択）
- actionsのシンボル検索（存在確認のみ）
- get_calling_functions_with_metadata で submitForm の呼び手を取得
- 期待: actions.submitForm に束縛された匿名関数が caller として得られる
- 実際: callers が空（バグ再現）。テストは assert!( !callers.is_empty() ) で失敗する前提の説明文が含まれる

Rustでの主要処理抜粋:
```rust
let mut indexer = SimpleIndexer::with_settings(Arc::new(Settings {
    workspace_root: Some(temp_dir.path().to_path_buf()),
    ..Default::default()
}));

indexer.index_file(test_file.to_str().unwrap()).expect("Failed to index file");

let submit_form_symbols = indexer.find_symbols_by_name("submitForm", Some("typescript"));
assert!(!submit_form_symbols.is_empty(), "submitForm function should be indexed");

let submit_form = &submit_form_symbols[0];
let callers = indexer.get_calling_functions_with_metadata(submit_form.id);
assert!(!callers.is_empty(), "BUG: submitForm should have at least one caller (the property method in actions object)");
```

2) test_regular_function_call_works
- 通常の関数呼び出しケースのTypeScriptコード生成
```typescript
function submitForm(data: any) {
    return { success: true, data };
}

function handleRequest(request: any) {
    return submitForm({ input: request.body });
}
```
- 同様にインデックス化
- submitFormの呼び手を取得
- 期待と実際: callers.len() == 1 で handleRequest が caller（このベースラインは成功する設計）

主要処理抜粋:
```rust
let mut indexer = SimpleIndexer::with_settings(Arc::new(Settings {
    workspace_root: Some(temp_dir.path().to_path_buf()),
    ..Default::default()
}));

indexer.index_file(test_file.to_str().unwrap()).expect("Failed to index file");

let submit_form_symbols = indexer.find_symbols_by_name("submitForm", Some("typescript"));
assert!(!submit_form_symbols.is_empty());
let submit_form = &submit_form_symbols[0];

let callers = indexer.get_calling_functions_with_metadata(submit_form.id);
assert_eq!(callers.len(), 1, "submitForm should have exactly one caller (handleRequest)");
assert_eq!(callers[0].0.name.as_ref(), "handleRequest");
```

## Complexity & Performance

- index_file: O(n) 時間（n=ソースファイル文字数）。空間は抽出されたシンボル・関係数に比例（O(s)）。
- find_symbols_by_name: O(m) 時間（m=インデックス済みシンボル総数）。空間は返却数に比例。
- get_calling_functions_with_metadata: 関係グラフからの逆引きで O(r)（r=対象シンボルに紐づく呼び手数）。
- 実運用負荷要因:
  - I/O: ファイル読み込み・書き込み（TempDir）、テストでは小規模で支配的ではない。
  - パース: TypeScript AST生成コストが支配的。大規模ファイルではCPU・メモリ増。
  - シンボル解決: 同名シンボルが増えると検索・解決コスト増加。

## Edge Cases, Bugs, and Security

既知バグ: オブジェクトリテラルのプロパティ関数（プロパティ名が呼び先関数と同名）の呼び出しが caller として記録されない。

詳細エッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| プロパティ名=関数名 | actions.submitForm が submitForm を呼ぶ | submitForm の callers に匿名関数（actions.submitForm）が現れる | test_object_property_calls_same_named_function | 失敗（空配列） |
| 同名シンボル複数ヒット | find_symbols_by_name("submitForm") | トップレベル関数を正しく選択 | submit_form_symbols[0] を無条件選択 | 脆弱（順序依存） |
| 行番号の境界条件 | 0/1-indexの差異 | オブジェクト定義の範囲内判定成功 | caller.range.start_line >= 7 && <= 11 | 脆弱（実装依存） |
| ベースライン通常呼び出し | handleRequest → submitForm | callers.len() == 1 で handleRequest を返す | test_regular_function_call_works | 成功 |

セキュリティチェックリスト:
- メモリ安全性: unsafe未使用。ArcでSettingsを共有。Use-after-free/Buffer overflow/Integer overflowの可能性はこのチャンクには現れない。
- インジェクション: SQL/Command/Path traversalの入力はなし。ファイルパスはTempDir配下で固定生成。
- 認証・認可: 該当なし。
- 秘密情報: ハードコード秘密なし。ログ出力はテスト情報のみで漏洩懸念なし。
- 並行性: テスト内で並列処理なし。Race/Deadlockの懸念なし。

Rust特有の観点（このファイルの範囲での評価）:
- 所有権: Settings はローカル変数から Arc::new(settings) で移動（test関数内）。SimpleIndexer に渡され、以降 indexer が所有（test_object_property_calls_same_named_function, test_regular_function_call_works）。
- 借用: &str でパスを渡す（to_str().unwrap()）。短期間借用で安全。
- ライフタイム: 明示的ライフタイム不要。TempDir はスコープ終了時に破棄。
- unsafe境界: なし。
- Send/Sync: SimpleIndexer/Settings の Send/Sync は不明。テストでは単一スレッド実行。
- データ競合: 共有状態なし。
- await境界/キャンセル: 非同期はTSコード内のみでRust側には非同期なし。
- エラー設計: unwrap/expect を使用（テスト文脈では許容）。本番コードではエラー伝播が望ましい。

## Design & Architecture Suggestions

- シンボル解決精度向上:
  - オブジェクトリテラルのプロパティ関数（arrow function）を独立した「関数シンボル」として抽出し、プロパティ名を別メタデータとして保持。
  - 呼び出し解決時、識別子 submitForm がスコープ内で何を参照しているかの静的解決（レキシカルスコープ/シャドウイング）を適用。
- コールグラフ拡張:
  - オブジェクトリテラル内の関数式からの呼び出しを通常の関数呼び出しと同等に扱う。
  - 「プロパティ名と関数名が同じ」ケースでも、識別子解決によりトップレベル関数へのエッジを構築。
- テスト設計:
  - find_symbols_by_name の返却を種別フィルタ（関数定義のみ）や場所（トップレベルに定義）で絞り込み。
  - 行番号照合を厳密化（ASTのrange仕様に合わせ0/1-indexを明確化）。

## Testing Strategy (Unit/Integration) with Examples

追加でカバーすべきユースケース（TypeScriptコード例）:

- オブジェクトメソッド（プロパティ記法の関数式以外、短縮メソッド記法）:
```typescript
function submitForm(data: any) { return {}; }
const actions = {
  // メソッド記法
  submitForm(request: any) {
    return submitForm({ input: request.body });
  }
};
```

- シャドウイング（ローカル変数が同名）:
```typescript
function submitForm(data: any) { return {}; }
const actions = {
  submitForm: async (request: any) => {
    const submitForm = (x: any) => ({});
    return submitForm({ input: request.body }); // ここはローカルを呼ぶべき
  }
};
```

- importされた同名関数とローカル関数の衝突:
```typescript
import { submitForm as submitFormExt } from "./lib";
function submitForm(data: any) { return {}; }
const actions = {
  submitForm: async (req: any) => submitForm({ input: req.body }) // ローカルを解決
};
```

- 動的プロパティ名（計算済みプロパティ）:
```typescript
function submitForm(data: any) { return {}; }
const prop = "submitForm";
const actions = {
  [prop]: async (req: any) => submitForm({ input: req.body })
};
```

- 後から代入されるプロパティ関数:
```typescript
function submitForm(data: any) { return {}; }
const actions: any = {};
actions.submitForm = async (req: any) => submitForm({ input: req.body });
```

これらのケースで、submitForm の callers が正しく取得できるか、またはシャドウイング時に意図通りの解決になるかを検証するテストを追加してください。

## Refactoring Plan & Best Practices

- ヘルパー抽出:
  - 重複する初期化処理（TempDir作成、Settings生成、indexer初期化）を共通関数に切り出す。
  - シンボル取得ユーティリティ（名前＋種別フィルタ）を用意して、最初の要素への依存を排除。
- 厳密なアサーション:
  - submitForm のシンボル kind が「関数定義」であることを確認してから id を使用。
  - 行番号の判定をASTの仕様（0/1-index）に合わせる。あるいは range（start/end）で包含判定。
- テストの可読性:
  - 期待値を明確化したメッセージ、失敗時に関係ダンプ（可能なら）を出す。
- ベストプラクティス:
  - unwrap/expectの利用はテストでは許容だが、テストヘルパー内で失敗内容を詳細化しデバッグ容易性を高める。

## Observability (Logging, Metrics, Tracing)

- 現在: println! による簡易ログ（発見シンボル数・kind・行番号・メタデータの印字）。
- 改善提案:
  - インデクサが提供するなら、インデックスされたシンボルと関係のダンプAPI（このチャンクには現れない）を使い、失敗時に詳細ログを出力。
  - ログ整形（JSON風）で後解析しやすくする。
  - メタデータの意味（呼び出し位置、呼び出し種類）が分かるようにコメント補足。

## Risks & Unknowns

- 内部実装不明:
  - SimpleIndexer のシンボル種別、範囲、言語判定、関係構築の詳細はこのチャンクには現れない。
- 検索順序の非決定性:
  - find_symbols_by_name の返却順序が安定しない場合、submit_form_symbols[0] への依存は脆弱。
- 行番号の定義:
  - range.start_line が0/1-indexかの保証が無く、境界チェックが不安定。
- 解析限界:
  - オブジェクトリテラル内関数式と呼び出し解決のアルゴリズムが未対応である可能性。修正にはASTレベルのスコープ解決強化が必要。

以上により、本テストは「既知の問題を再現し、基礎ケースは通る」ことを明確に示します。安定性と診断性を高めるために、フィルタリングとヘルパーの導入、追加ユースケースによるテスト拡充を推奨します。