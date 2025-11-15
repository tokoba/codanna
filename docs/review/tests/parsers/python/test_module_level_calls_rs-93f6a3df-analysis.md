# parsers\python\test_module_level_calls.rs Review

## TL;DR

- 目的: Pythonコードからの**クラスインスタンス化呼び出し**の検出テスト。特に**モジュールレベル**の呼び出しが**caller="<module>"**として検出されることを確認。
- 主要API利用: `PythonParser::new`, `PythonParser::parse`, `PythonParser::find_calls`, `FileId::new`, `SymbolCounter::new`（本ファイル自体の公開APIはなし）。
- コアロジック: 解析→シンボル検証（`SymbolKind::Module`）→呼び出し抽出→期待組合せのアサート。
- 複雑箇所: `find_calls`の出力タプル（caller, callee, range様）整合、モジュールレベルの呼び出しを`<module>`へ正しくマッピングすること。
- 重大リスク: `unwrap`によるパニック、`to_string`の不必要なアロケーション、`find_calls`の型詳細・範囲情報が不明でテストの堅牢性に影響。
- 安全性/並行性: `unsafe`なし、同期のみのテスト。共有状態（`SymbolCounter`）は単一スレッドで安全に可変借用。
- パフォーマンス: 処理は入力サイズにほぼ線形（推定）。大規模ファイルでの`to_string`や線形探索のオーバヘッドが増加。

## Overview & Purpose

このファイルはRustの単体テストで、`codanna`ライブラリのPythonパーサ（`PythonParser`）がPythonコード中のクラスインスタンス化（`DatabaseClient()`, `ConfigManager()`, `Logger()`など）を正しく検出できるかを検証します。特に、従来バグがあったと推測される「モジュールレベルのインスタンス化検出」について、callerを`"<module>"`として認識できる修正が有効であることを確認します。

テストは以下を確認します:
- パーサがファイルに対し**モジュールシンボル**（`SymbolKind::Module`）を生成していること。
- `find_calls`が関数・メソッド・モジュールレベルのインスタンス化をそれぞれ正しく検出すること。

このチャンクには`PythonParser`実装は含まれず、テストコードのみが存在します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_module_level_class_instantiation_detection | private (test) | Pythonコードを解析し、関数/メソッド/モジュールレベルのクラスインスタンス化呼び出し検出を検証 | Med |

### Dependencies & Interactions

- 内部依存
  - `test_module_level_class_instantiation_detection` → `PythonParser::new`（パーサ生成）
  - `test_module_level_class_instantiation_detection` → `PythonParser::parse`（シンボル抽出）
  - `test_module_level_class_instantiation_detection` → `PythonParser::find_calls`（呼び出し抽出）
  - `test_module_level_class_instantiation_detection` → `FileId::new`（ファイル識別子作成）
  - `test_module_level_class_instantiation_detection` → `SymbolCounter::new`（解析用カウンタ）

- 外部依存（使用クレート・モジュール）
  | クレート/モジュール | シンボル | 用途 |
  |---------------------|----------|------|
  | `codanna::parsing::LanguageParser` | トレイト（推定） | パーサインタフェース（このチャンクでは直接未使用/不明） |
  | `codanna::parsing::python::PythonParser` | 構造体 | Pythonコード解析と呼び出し検出 |
  | `codanna::types::SymbolKind` | 列挙 | シンボルの種類（Moduleなど）の判定 |
  | `codanna::types::FileId` | 構造体 | ファイルIDの生成 |
  | `codanna::types::SymbolCounter` | 構造体 | 解析時のシンボルカウント管理 |

- 被依存推定
  - 本ファイルはテスト専用。`cargo test`で実行され、他モジュールからの直接利用は「このチャンクには現れない」。プロダクションコードへの被依存は「不明」。

## API Surface (Public/Exported) and Data Contracts

本ファイルの公開API: 該当なし（テスト関数のみ）。

以下は本テストが利用する外部API（シグネチャはこのチャンクには現れないため推定を交えます）。型詳細は「不明」項目で明記します。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| PythonParser::new | 不明（Resultを返し`expect`可能） | パーサインスタンス生成 | O(1) | O(1) |
| PythonParser::parse | 不明（`&str`, `FileId`, `&mut SymbolCounter`を受け取り`Vec<Symbol>`様を返す） | Pythonコードからシンボル抽出（Module等） | O(n) | O(k) |
| PythonParser::find_calls | 不明（`&str`を受け取り`Vec<(caller, callee, range様)>`） | コード内の呼び出し（インスタンス化等）の抽出 | O(n) | O(m) |
| FileId::new | 不明（`u32`→`Result<FileId, E>`） | ファイル識別子の生成 | O(1) | O(1) |
| SymbolCounter::new | `fn new() -> SymbolCounter`（推定） | 解析中のシンボル数追跡 | O(1) | O(1) |

詳細説明:

1) PythonParser::new
- 目的と責務: パーサの初期化。失敗時は`Result::Err`（`expect("Failed to create parser")`より推測）。
- アルゴリズム: 初期化のみ。詳細は不明。
- 引数: なし。
- 戻り値: Result（正確な型は不明）。
- 使用例:
  ```rust
  let mut parser = PythonParser::new().expect("Failed to create parser");
  ```
- エッジケース:
  - 初期化失敗（リソース不足や設定不備）→ `expect`によりテストがpanic。

2) PythonParser::parse
- 目的と責務: コード文字列を解析し、シンボル（`Module`等）を抽出。
- アルゴリズム: 字句/構文解析（詳細不明）。
- 引数:
  | 引数 | 型 | 説明 |
  |------|----|------|
  | source | `&str` | Pythonコード |
  | file_id | `FileId` | ファイル識別子 |
  | counter | `&mut SymbolCounter` | シンボルカウント状態 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | `Vec<Symbol>`様 | シンボルリスト（`name`, `kind`などを含むと推定） |
- 使用例:
  ```rust
  let symbols = parser.parse(python_code, file_id, &mut counter);
  let module_symbol = symbols.iter().find(|s| s.kind == SymbolKind::Module);
  assert!(module_symbol.is_some());
  assert_eq!(module_symbol.unwrap().name.as_ref(), "<module>");
  ```
- エッジケース:
  - 空コード→`Module`のみ生成か不明。
  - 構文エラー→戻り値仕様不明（このチャンクには現れない）。

3) PythonParser::find_calls
- 目的と責務: 呼び出し関係（caller→callee）抽出。インスタンス化も対象。
- アルゴリズム: AST/トークン解析で呼び出しノード検出（詳細不明）。
- 引数:
  | 引数 | 型 | 説明 |
  |------|----|------|
  | source | `&str` | Pythonコード |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | `Vec<(caller, callee, range様)>` | 呼び出しタプル（caller, calleeの型は不明。`to_string`可能） |
- 使用例:
  ```rust
  let calls = parser.find_calls(python_code);
  for (caller, callee, _range) in &calls {
      println!("  {caller} -> {callee}");
  }
  ```
- エッジケース:
  - モジュールレベル呼び出し→callerが`"<module>"`になることを期待。
  - 関数/メソッド内呼び出し→それぞれの関数/メソッド名がcaller。
  - コメント/文字列中の疑似呼び出し→無視すべき。

4) FileId::new
- 目的と責務: 有効なファイルID生成。
- 使用例:
  ```rust
  let file_id = FileId::new(1).unwrap();
  ```
- エッジケース:
  - 不正ID→`unwrap`がpanic。

5) SymbolCounter::new
- 目的と責務: 解析時のカウント初期化。
- 使用例:
  ```rust
  let mut counter = SymbolCounter::new();
  ```
- エッジケース: 特になし。

注: 重要な主張の根拠（関数名:行番号）は、このチャンクには行番号情報がないため「不明」。

## Walkthrough & Data Flow

処理の主要ステップとデータフロー:

1. Pythonコードセットアップ（`python_code`）
   - モジュールレベルのインスタンス化3件（`DatabaseClient()`, `ConfigManager()`, `Logger()`）
   - 関数`process_data`内のインスタンス化2件（`DataProcessor()`, `InputValidator()`）
   - クラス`Application.__init__`内のインスタンス化2件（`DatabaseConnection()`, `CacheManager()`）

2. パーサ初期化
   - `PythonParser::new().expect(...)`でインスタンス作成。
   - `FileId::new(1).unwrap()`でファイルID作成。
   - `SymbolCounter::new()`でカウンタ作成。

3. 解析（シンボル抽出）
   - `parse(python_code, file_id, &mut counter)`を呼び、返却された`symbols`から`SymbolKind::Module`を検索。
   - `name.as_ref() == "<module>"`をアサート。

4. 呼び出し抽出
   - `find_calls(python_code)`で`calls: Vec<(caller, callee, range様)>`取得。
   - 目視確認用に`println!`。

5. アサーション（期待値検証）
   - `call_pairs`へ`(caller.to_string(), callee.to_string())`でペア化。
   - 関数レベル2件、メソッドレベル2件、モジュールレベル3件の検出を`contains`で検証。
   - 成功メッセージとモジュールレベルのみのフィルタ出力。

データフロー概要:
- 入力: `&str python_code`
- 状態: `FileId`, `SymbolCounter`（可変）
- 出力1: `symbols`（Moduleシンボルの存在確認）
- 出力2: `calls`（caller→calleeペアの検証）

## Complexity & Performance

- 時間計算量
  - `PythonParser::parse`: O(n)（n=コード長、推定）
  - `PythonParser::find_calls`: O(n)（n=コード長、推定）
  - モジュール内検証（`iter().find`, `contains`の線形探索）: O(k + m·p)
    - k=シンボル数、m=検出呼び出し数、p=期待ペア数（本テストでは小）
- 空間計算量
  - `symbols`: O(k)
  - `calls`と`call_pairs`: O(m + m) ≈ O(m)
- ボトルネック
  - `call_pairs.contains(...)`の複数回線形検索はmが増えるとコスト増。
  - `to_string`による文字列アロケーションが不要な場合にオーバヘッド。
- 実運用負荷要因
  - I/Oはなし。CPUメイン。大規模コード解析では`find_calls`/`parse`の線形走査が支配的。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価（本ファイルはテストであり、危険操作は限定的）:

- メモリ安全性
  - Buffer overflow / Use-after-free: 該当なし（安全なRust。`unsafe`未使用）。
  - Integer overflow: 該当なし。
- インジェクション
  - SQL/Command/Path traversal: 該当なし（静的文字列を解析）。
- 認証・認可
  - 該当なし。
- 秘密情報
  - Hard-coded secrets: 該当なし。
  - Log leakage: `println!`で解析結果を標準出力へ出すが、テスト環境想定。
- 並行性
  - Race condition / Deadlock: 該当なし（単一スレッド）。

既知/推測されるバグポイント:
- コメント「Get calls - this is where the bug manifests」より、過去に`find_calls`がモジュールレベル呼び出しを検出しなかった可能性。現在は修正済みと期待（アサートで確認）。
- `FileId::new(1).unwrap()`は失敗時にpanic。テストでは許容だが、ライブラリ側の`new`仕様次第で脆弱。
- `to_string`乱用により、不要なヒープ確保が多い。比較用に`&str`のまま扱えるならそちらが望ましい。

エッジケース詳細:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | `""` | Moduleは生成、呼び出しは0件 | `parse`/`find_calls`の仕様はこのチャンクでは不明 | 不明 |
| コメント/Docstring中の疑似呼び出し | `"# Logger()"`や`"""DataProcessor()"""` | 無視 | `find_calls`のフィルタ仕様は不明 | 不明 |
| 条件分岐内のモジュールレベル呼び出し | `if True: client = C()` | `<module>`として検出 | 不明 | 不明 |
| メソッドチェーン | `client = C().setup()` | calleeは`C`（インスタンス化）を検出 | 不明 | 不明 |
| 別名インポート/シャドーイング | `from m import Logger as L; x = L()` | calleeは`L`か`Logger`か仕様次第 | 不明 | 不明 |
| 無効なFileId | `FileId::new(0)`想定 | `Err`で扱い、panicしない | 本テストは`unwrap` | 改善余地 |

注: 行番号はこのチャンクには現れないため「不明」。

## Design & Architecture Suggestions

- 呼び出し表現の型強化
  - `find_calls`の返却型を`Vec<Call>`（`struct Call { caller: Cow<'a, str>, callee: Cow<'a, str>, range: Range }`など）にし、所有権と借用を明確化。比較に`&str`を使えば`to_string`不要。
- モジュールレベルcallerの統一
  - 解析器と呼び出し抽出で`"<module>"`の命名規則を明文化し、`parse`で生成されるModuleシンボル名と一致させる（今回のテストがそれを検証）。
- APIのエラー設計
  - `parse`/`find_calls`が構文エラーや不正入力時に`Result`を返す設計（現状は戻り値から推測が難しい）。テストでエラー系も網羅可能に。
- 比較効率化
  - テスト側で`HashSet<(caller, callee)>`を用いて期待集合比較。線形`contains`多用を避ける。
- デバッグ容易性
  - `find_calls`にデバッグ用の`--`（feature）で抽出理由（ノード種別、行、列）を含む詳細モードを追加。

## Testing Strategy (Unit/Integration) with Examples

拡張テスト観点:
- 空／コメントのみのコード
- Docstring内の括弧類が誤検出されないこと
- 条件分岐、ループ、例外ハンドリング内の呼び出し
- メソッドチェーン、属性アクセス、名前の再束縛（シャドーイング）
- エイリアスインポート、モジュール境界を跨ぐ呼び出し
- ネストされた関数・クラス定義とその内側のインスタンス化

例1: コメント/Docstring無視の検証
```rust
#[test]
fn test_ignore_comments_and_docstrings() {
    let python_code = r#"
# Logger()
\"\"\" DataProcessor() \"\"\"
def f():
    pass
"#;
    let mut parser = PythonParser::new().expect("Failed to create parser");
    let calls = parser.find_calls(python_code);
    assert!(calls.is_empty(), "コメントやDocstring内の疑似呼び出しは検出しない");
}
```

例2: 条件分岐内でも`<module>`として検出
```rust
#[test]
fn test_module_level_inside_if() {
    let code = r#"
if True:
    client = DatabaseClient()
"#;
    let mut parser = PythonParser::new().expect("Failed to create parser");
    let calls = parser.find_calls(code);
    let has = calls.iter().any(|(caller, callee, _)| caller == "<module>" && callee == "DatabaseClient");
    assert!(has, "条件分岐内でもモジュールレベルの呼び出しとして検出されるべき");
}
```

例3: 期待集合の効率的比較（HashSet）
```rust
use std::collections::HashSet;

#[test]
fn test_expected_calls_set() {
    let code = r#"
def process_data():
    processor = DataProcessor()
    validator = InputValidator()
"#;
    let mut parser = PythonParser::new().expect("Failed to create parser");
    let calls = parser.find_calls(code);
    let actual: HashSet<(String, String)> =
        calls.iter().map(|(c, callee, _)| (c.to_string(), callee.to_string())).collect();
    let expected: HashSet<(String, String)> = [
        ("process_data".to_string(), "DataProcessor".to_string()),
        ("process_data".to_string(), "InputValidator".to_string()),
    ].into_iter().collect();
    assert_eq!(actual.intersection(&expected).count(), expected.len());
}
```

## Refactoring Plan & Best Practices

- `unwrap`の削減
  - `FileId::new(1)?`のように`Result`を伝播（テストでは`anyhow`や`color-eyre`を併用し、失敗時に情報豊富なエラー）。
- 文字列比較の効率化
  - `call_pairs`の生成で`to_string`を避け、`&str`比較を試みる（返却型が借用可能なら）。あるいは一括で`HashSet`比較。
- 出力の明確化
  - `println!`の代わりに`assert`メッセージをより具体的にし、失敗箇所の差分を表示。
- テストの構造化
  - 共通セットアップ（パーサ生成）をヘルパー関数へ抽出。
- 型の明確化
  - `find_calls`の返却タプルに対する型エイリアス（例: `type Call<'a> = (&'a str, &'a str, Range);`）を導入し、利用側の意図を明確化。

## Observability (Logging, Metrics, Tracing)

- ログ
  - `find_calls`内部でデバッグログ（抽出根拠、位置情報）を出力可能なfeatureを用意。テスト時は`RUST_LOG=debug`で確認。
- メトリクス
  - 検出された呼び出し数、モジュール/関数/メソッド別件数をカウントし、性能回帰テストで監視。
- トレーシング
  - 解析フェーズ（トークン化→AST生成→走査→抽出）のスパンを`tracing`で可視化し、どこで取りこぼしが起きるかを特定。

## Risks & Unknowns

- 返却型の不透明性
  - `find_calls`のタプル詳細型（`caller`, `callee`, `range`）はこのチャンクには現れないため不明。比較や所有権戦略の最適化可否も不明。
- 構文エラー処理
  - `parse`のエラー取り扱いが不明。壊れたコードをどう扱うかは仕様に依存。
- `<module>`の定義の一貫性
  - モジュールシンボル名とcaller名の正規化がライブラリ全体で統一されているか不明。
- スコープ解決の詳細
  - `callee`がシンボル解決済み名か、トークン文字列かは不明。エイリアスやインポート解決の可否も不明。
- 並行利用
  - `PythonParser`や`SymbolCounter`の`Send`/`Sync`境界はこのチャンクには現れない。並列解析時の安全性は不明。

以上により、本テストはモジュールレベル呼び出し検出の回帰防止に有用ですが、型の明確化とエラー/性能面の改善余地が存在します。