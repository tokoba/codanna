# io\test.rs Review

## TL;DR

- 目的: I/Oモジュールの**JSON出力**と**フォーマット切替**、**ExitCode**の整合性をテストする（JsonResponse, OutputManager, OutputFormat）。
- 主要API: **JsonResponse::success / not_found**, **OutputManager::new_with_writers / success**, **OutputFormat::from_json_flag / is_json**、および**Symbol**のビルダー的API。
- 複雑箇所: OutputManagerに対する**所有権移動**により、テストから**キャプチャした出力を取り出しづらい**（Vec<u8>をBox<dyn Write>へ移動）。
- 重大リスク: テスト内の**unwrap**使用によるパニック可能性、**文字列包含による脆弱なアサーション**（JSON構造の厳密検証ができていない）。
- セキュリティ: JSONはエスケープされるが、**ログ出力（println!）へ生文字列**を流しているため、大規模データでの可観測性低下やログ肥大化に注意。
- 並行性: このチャンクでは**非同期/並行処理は不明**。Send/Syncやawait境界の検証は未実施。
- 改善提案: **Cursor<Vec<u8>>**で出力を取り回し、**serde_jsonで構造体にデコード**して厳密に検証。エラーケース・大規模データ・境界値のテスト追加。

## Overview & Purpose

このファイルは、I/O関連のユーティリティ（主にJSONレスポンス生成と出力管理）をテストするためのモジュールです。具体的には:

- Symbolエンティティを**JsonResponse**で包み、**serde_json::to_string_pretty**で**整形JSON**へシリアライズし、その中身に期待フィールドが含まれることを検証します。
- **OutputManager**を**OutputFormat::Json**で生成し、成功パスにおける**ExitCode::Success**の返却を検証します。なお、出力バッファを**Box<dyn Write>**へ移動するため、テストから出力内容の直接取得は困難です。
- **OutputFormat**のフラグ変換と判定（is_json）をテストし、フォーマット切替の基本的な挙動を確認します。
- **NOT_FOUND**ケースのJSONを検証し、エラー時のコード・ステータス・追加情報（suggestions）の存在を確認します。

このチャンクでは実装詳細（構造体定義や関数本体）は現れていないため、APIの正確なシグネチャは推定で記述し、契約は「テストが前提とする期待値」として整理します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_symbol_json_output | 非公開（#[test]） | SymbolのJSON化と期待フィールド検証 | Low |
| Function | test_output_manager_simple | 非公開（#[test]） | OutputManagerの成功パス検証（ExitCode） | Low |
| Function | test_multiple_symbols_json | 非公開（#[test]） | 複数SymbolのJSON化・基本フィールド包含検証 | Low |
| Function | test_not_found_json | 非公開（#[test]） | NOT_FOUNDレスポンスの構造検証 | Low |
| Function | test_output_format_flag | 非公開（#[test]） | フォーマットフラグとis_jsonの基本挙動検証 | Low |

### Dependencies & Interactions

- 内部依存
  - 各テスト関数は互いに独立（共通ヘルパーなし）。全てが外部モジュールのAPI呼び出しとアサーション中心。
- 外部依存（このチャンクで使用）
  | クレート/モジュール | 用途 | 備考 |
  |--------------------|------|------|
  | crate::io::{ExitCode, JsonResponse, OutputFormat, OutputManager} | I/Oユーティリティ | 実装はこのチャンクには現れない |
  | crate::symbol::Symbol | テスト対象データモデル | ビルダー的API（with_signature, with_doc） |
  | crate::types::{FileId, Range, SymbolId, SymbolKind} | Symbol構築用の基本型 | new() + unwrapで作成 |
  | serde_json | JSON整形シリアライズ | to_string_prettyを使用 |
  | std::io::Write（トレイト） | Box<dyn Write>で出力捕捉 | Vec<u8>がWrite実装を満たす |
- 被依存推定
  - テストモジュールのため、プロダクションコードからの利用は**該当なし**。

## API Surface (Public/Exported) and Data Contracts

このファイル自体の公開APIはありません（テストのみ）。以下はテストから見える外部APIの「推定」一覧です。正確な型は「このチャンクには現れない」ため、実際の定義は元モジュールを参照してください。

| API名 | シグネチャ（推定） | 目的 | Time | Space |
|-------|---------------------|------|------|-------|
| JsonResponse::success | fn success<T: serde::Serialize>(data: &T) -> JsonResponse | 成功レスポンス生成 | O(n) | O(n) |
| JsonResponse::not_found | fn not_found(entity: &str, name: &str) -> JsonResponse | NOT_FOUNDエラーレスポンス生成 | O(1) | O(1) |
| OutputManager::new_with_writers | fn new_with_writers(fmt: OutputFormat, stdout: Box<dyn Write>, stderr: Box<dyn Write>) -> OutputManager | 出力先指定のマネージャ生成 | O(1) | O(1) |
| OutputManager::success | fn success<T: serde::Serialize>(&mut self, data: &T) -> Result<ExitCode, E> | 成功メッセージ出力とExitCode返却 | O(n) | O(n) |
| OutputFormat::from_json_flag | fn from_json_flag(json: bool) -> OutputFormat | boolからフォーマット選択 | O(1) | O(1) |
| OutputFormat::is_json | fn is_json(&self) -> bool | JSON判定 | O(1) | O(1) |
| Symbol::new | fn new(id: SymbolId, name: &str, kind: SymbolKind, file_id: FileId, range: Range) -> Symbol | Symbolインスタンス生成 | O(1) | O(1) |
| Symbol::with_signature | fn with_signature(self, sig: &str) -> Symbol | 署名付与（ビルダー） | O(1) | O(1) |
| Symbol::with_doc | fn with_doc(self, doc: &str) -> Symbol | ドキュメント付与（ビルダー） | O(1) | O(1) |

詳細説明（推定・このチャンクのテスト根拠）:

1) JsonResponse::success
- 目的と責務: 任意のシリアライズ可能データを成功レスポンスとして包み、JSON化可能にする。
- アルゴリズム: status="success", code="OK", exit_code=0、dataへ入力を格納（推定）。
- 引数:
  | 名前 | 型 | 説明 |
  |------|----|------|
  | data | &T | シリアライズ可能なデータ |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | JsonResponse | 成功レスポンス |
- 使用例:
  ```rust
  let response = JsonResponse::success(&symbol);
  let json_string = serde_json::to_string_pretty(&response).unwrap();
  ```
- エッジケース:
  - dataに循環参照や非シリアライズ型: シリアライズ失敗（このチャンクには現れない）。
  - 大規模データ: メモリ使用増加。

2) JsonResponse::not_found
- 目的と責務: 指定エンティティ名・識別子に対する未検出レスポンス生成。
- アルゴリズム: status="error", code="NOT_FOUND", exit_code=3、追加情報としてsuggestions等（推定）。
- 引数:
  | 名前 | 型 | 説明 |
  |------|----|------|
  | entity | &str | 種類（例: "Symbol"） |
  | name | &str | 探索対象名 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | JsonResponse | エラーレスポンス |
- 使用例:
  ```rust
  let response = JsonResponse::not_found("Symbol", "undefined_function");
  let json = serde_json::to_string_pretty(&response).unwrap();
  ```
- エッジケース:
  - suggestionsが空/非存在: UIでの取り扱い要注意（このチャンクには現れない）。

3) OutputManager::new_with_writers
- 目的と責務: 出力フォーマットと宛先（stdout/stderr）を外部から注入可能な形で構築。
- アルゴリズム: 与えられたBox<dyn Write>を内部に保持。
- 引数:
  | 名前 | 型 | 説明 |
  |------|----|------|
  | fmt | OutputFormat | JSON/Text選択 |
  | stdout | Box<dyn Write> | 標準出力の書き込み先 |
  | stderr | Box<dyn Write> | 標準エラーの書き込み先 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | OutputManager | マネージャ |
- 使用例:
  ```rust
  let stdout = Vec::new();
  let stderr = Vec::new();
  let mut manager = OutputManager::new_with_writers(
      OutputFormat::Json, Box::new(stdout), Box::new(stderr)
  );
  ```
- エッジケース:
  - 渡されたWriterがエラーを返す: 後続操作でResultがErr（このチャンクには現れない）。
  - 所有権移動により出力取得が困難。

4) OutputManager::success
- 目的と責務: 成功データを適切なフォーマットで出力し、ExitCodeを返す。
- アルゴリズム: fmtがJsonならJsonResponse::successを経由してJSON出力（推定）。
- 引数:
  | 名前 | 型 | 説明 |
  |------|----|------|
  | self | &mut OutputManager | マネージャ |
  | data | &T | 出力対象 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Result<ExitCode, E> | 成功時ExitCode::Success |
- 使用例:
  ```rust
  let exit_code = manager.success(&symbol).unwrap();
  assert_eq!(exit_code, ExitCode::Success);
  ```
- エッジケース:
  - Writerへの書き込み失敗: Errを返す（このチャンクには現れない）。

5) OutputFormat::from_json_flag / is_json
- 目的と責務: フォーマットの切替と判定。
- 引数・戻り値: いずれもO(1)の軽量操作。
- 使用例:
  ```rust
  assert_eq!(OutputFormat::from_json_flag(true), OutputFormat::Json);
  assert!(OutputFormat::Json.is_json());
  ```

データ契約（テストが前提とするJSONフィールド）:
- success: {"status": "success", "code": "OK", "exit_code": 0, ...}
- not_found: {"status": "error", "code": "NOT_FOUND", "exit_code": 3, "suggestions": ...}
- Symbolシリアライズ: {"name": "...", "kind": "Function|Struct|Method", ...}
- 正確なスキーマはこのチャンクには現れないが、テストは上記フィールドの存在を要求。

## Walkthrough & Data Flow

テスト関数ごとの処理の流れを、主要ステップに分解します。

1) test_symbol_json_output
- フロー
  1. Symbolをnewで生成し、with_signature, with_docで拡張。
  2. JsonResponse::successでレスポンス生成。
  3. serde_json::to_string_prettyで文字列へ整形。
  4. 期待フィールドの包含をassert。
  5. println!で出力（目視確認用）。
- 抜粋コード:
  ```rust
  let symbol = Symbol::new(
      SymbolId::new(42).unwrap(),
      "calculate_similarity",
      SymbolKind::Function,
      FileId::new(1).unwrap(),
      Range::new(100, 4, 120, 5),
  )
  .with_signature("fn calculate_similarity(a: &[f32], b: &[f32]) -> f32")
  .with_doc("Calculate cosine similarity between two vectors");

  let response = JsonResponse::success(&symbol);
  let json_string = serde_json::to_string_pretty(&response).unwrap();

  assert!(json_string.contains(r#""status": "success""#));
  assert!(json_string.contains(r#""code": "OK""#));
  assert!(json_string.contains(r#""exit_code": 0"#));
  assert!(json_string.contains(r#""name": "calculate_similarity""#));
  assert!(json_string.contains(r#""kind": "Function""#));
  ```

2) test_output_manager_simple
- フロー
  1. Symbol生成。
  2. Vec<u8>をBox<dyn Write>に包み、OutputManagerを生成。
  3. manager.success(&symbol)を呼び、ExitCode::Successを検証。
  4. 出力内容は所有権の都合で取得せず。
- 抜粋コード:
  ```rust
  let stdout = Vec::new();
  let stderr = Vec::new();
  let mut manager =
      OutputManager::new_with_writers(OutputFormat::Json, Box::new(stdout), Box::new(stderr));

  let exit_code = manager.success(&symbol).unwrap();
  assert_eq!(exit_code, ExitCode::Success);
  ```

3) test_multiple_symbols_json
- フロー
  1. 複数Symbolを生成（Function, Struct, Methodなど）。
  2. JsonResponse::successで配列を包む。
  3. 整形JSONへ変換し、各名称・種類が含まれることをassert。
- 抜粋コード（重要部分のみ）:
  ```rust
  let symbols = vec![
      Symbol::new(SymbolId::new(1).unwrap(), "main", SymbolKind::Function, FileId::new(1).unwrap(), Range::new(10, 0, 15, 1))
          .with_signature("fn main()"),
      Symbol::new(SymbolId::new(2).unwrap(), "Config", SymbolKind::Struct, FileId::new(1).unwrap(), Range::new(20, 0, 30, 1)),
      Symbol::new(SymbolId::new(3).unwrap(), "parse", SymbolKind::Method, FileId::new(2).unwrap(), Range::new(40, 0, 50, 1))
          .with_signature("fn parse(&self) -> Result<(), Error>"),
  ];

  let response = JsonResponse::success(&symbols);
  let json = serde_json::to_string_pretty(&response).unwrap();

  assert!(json.contains(r#""main""#));
  assert!(json.contains(r#""Config""#));
  assert!(json.contains(r#""parse""#));
  assert!(json.contains(r#""Function""#));
  assert!(json.contains(r#""Struct""#));
  assert!(json.contains(r#""Method""#));
  ```

4) test_not_found_json
- フロー
  1. JsonResponse::not_foundでエラーレスポンス生成。
  2. 整形JSONへ変換。
  3. status, code, exit_code, suggestionsフィールドの存在をassert。
- 抜粋コード:
  ```rust
  let response = JsonResponse::not_found("Symbol", "undefined_function");
  let json = serde_json::to_string_pretty(&response).unwrap();

  assert!(json.contains(r#""status": "error""#));
  assert!(json.contains(r#""code": "NOT_FOUND""#));
  assert!(json.contains(r#""exit_code": 3"#));
  assert!(json.contains("undefined_function"));
  assert!(json.contains("suggestions"));
  ```

5) test_output_format_flag
- フロー
  1. OutputFormat::from_json_flagの真偽に応じたフォーマットを検証。
  2. is_jsonの戻り値検証。
- 抜粋コード:
  ```rust
  assert_eq!(OutputFormat::from_json_flag(true), OutputFormat::Json);
  assert_eq!(OutputFormat::from_json_flag(false), OutputFormat::Text);
  assert!(OutputFormat::Json.is_json());
  assert!(!OutputFormat::Text.is_json());
  ```

このチャンクでは条件分岐や状態遷移の複雑なロジックは存在しないため、Mermaid図は該当なし。

## Complexity & Performance

- JsonResponseの生成とserde_json::to_string_prettyによる整形は、入力サイズnに対して時間O(n)、空間O(n)。
- 文字列containsによる検証は、各assertが対象文字列長mに対してO(m)。複数assertの合計は定数回なので全体O(n + k·m)（kはassert数、mは検索パターン長）。
- OutputManager::successは内部でJSON/Textフォーマットに応じたシリアライズとWriteへ出力（推定）。Writeのコストはデータ長に比例。
- 実運用負荷要因（推定）
  - I/O: **Writer**への書き込み速度がボトルネックになり得る。
  - JSON整形: pretty-printは余分な空白・改行でサイズ増加し、CPUとメモリ消費が上がる。

## Edge Cases, Bugs, and Security

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空のSymbol名 | "" | JSONへは空文字として出力 | このチャンクには現れない | 不明 |
| 無効なSymbolId | SymbolId::new(0)? | newがErr/Noneを返す、unwrapでpanic | unwrap使用 | 要改善 |
| Range不正（開始>終了） | Range::new(120,5,100,4) | 生成拒否/検証エラー | このチャンクには現れない | 不明 |
| 巨大ドキュメント文字列 | 1MB doc | JSON出力成功だがメモリ増加 | このチャンクには現れない | 注意 |
| Writer書き込み失敗 | BrokenWriter | successがErr返却 | このチャンクには現れない | 不明 |
| suggestions未定義 | not_found | JSONにフィールドなし | このチャンクには現れない | 不明 |

- セキュリティチェックリスト
  - メモリ安全性: unsafeは使用なし。Buffer overflow/Use-after-freeはRustの型安全により通常防止。unwrapの使用は**パニック**を引き起こす可能性あり（入力が不正な場合）→テストでは正常値だが、本質的には脆弱。
  - インジェクション: **JSONシリアライズはエスケープ**されるため、文字列ベースのインジェクションリスクは低い。ただし、後段でこのJSONをコマンドに入力するようなパイプラインがある場合は**Command/Path traversal**に注意（このチャンクには現れない）。
  - 認証・認可: 該当なし（テストのみ）。
  - 秘密情報: Hard-coded secretsなし。ログ漏えいは小規模（println!のみ）だが、実運用では**大量出力**でログ肥大化に注意。
  - 並行性: このチャンクには並行処理が現れない。Race condition/Deadlockは**該当なし**。

## Design & Architecture Suggestions

- JSON検証の厳密化: 文字列包含ではなく、serde_jsonで**中間構造体にデコード**し、フィールド値を**型安全**に検証する。
- 出力取り回し: OutputManagerには**Cursor<Vec<u8>>**を渡し、テスト後に**into_inner()**でバッファ内容を取得して**出力内容の完全検証**を行う。
- 共通ヘルパー: Symbol生成やレスポンス検証を**ヘルパー関数**化して重複を削減、可読性を向上。
- スキーマ合意: JsonResponseの**スキーマ（status/code/exit_code/data/suggestions等）**をドキュメント化し、将来の変更による破壊的影響を抑制。
- フォーマット拡張: OutputFormatに**YAML/NDJSON**などが増える可能性に備え、テストを**パラメータ化**する設計に。

## Testing Strategy (Unit/Integration) with Examples

- 単体テストの強化
  - エラーケース（Writer失敗）の検証
  - 巨大入力に対するパフォーマンステスト（ベンチは別途）
  - not_foundの**suggestions**の具体的内容（型・件数）検証

- 例: OutputManagerの出力キャプチャ
  ```rust
  use std::io::Cursor;

  #[test]
  fn test_output_manager_captures_json() {
      let symbol = Symbol::new(
          SymbolId::new(1).unwrap(),
          "test_function",
          SymbolKind::Function,
          FileId::new(1).unwrap(),
          Range::new(10, 0, 20, 0),
      );

      let stdout = Cursor::new(Vec::<u8>::new());
      let stderr = Cursor::new(Vec::<u8>::new());
      let mut manager = OutputManager::new_with_writers(
          OutputFormat::Json,
          Box::new(stdout),
          Box::new(stderr),
      );

      let exit_code = manager.success(&symbol).unwrap();
      assert_eq!(exit_code, ExitCode::Success);

      // ここでmanagerからstdout/stderrを取得できるAPIがなければ、
      // new_with_writersに受け渡し用の型を工夫する（このチャンクには現れない）。
  }
  ```

- 例: JsonResponseの厳密検証（仮の構造体）
  ```rust
  #[derive(serde::Deserialize)]
  struct Resp<T> {
      status: String,
      code: String,
      exit_code: i32,
      data: T,
  }

  #[test]
  fn test_symbol_json_deserialize() {
      let symbol = Symbol::new(
          SymbolId::new(42).unwrap(),
          "calculate_similarity",
          SymbolKind::Function,
          FileId::new(1).unwrap(),
          Range::new(100, 4, 120, 5),
      );

      let response = JsonResponse::success(&symbol);
      let json_string = serde_json::to_string(&response).unwrap();
      let parsed: Resp<serde_json::Value> = serde_json::from_str(&json_string).unwrap();

      assert_eq!(parsed.status, "success");
      assert_eq!(parsed.code, "OK");
      assert_eq!(parsed.exit_code, 0);
      assert_eq!(parsed.data["name"], "calculate_similarity");
      assert_eq!(parsed.data["kind"], "Function");
  }
  ```

## Refactoring Plan & Best Practices

- unwrapの排除: テストでも**expect("理由")**を使って失敗時のコンテキストを明確化。
- ビルダーAPIの一貫性: with_signature/with_docの戻り型が**Self**で統一されていることを前提に、メソッドチェーンの意図がわかるようドキュメント化（このチャンクには現れない）。
- アサーションのまとまり: 同一JSONに対する複数assertは**構造体デコード**で1度に検証し、テスト失敗時の診断を改善。
- 出力のテスト容易性: OutputManagerに**テスト用の出力取り出しAPI**（例えば、get_stdout_bytes）を用意するか、注入するWriterを**参照で外部保持**できる設計を検討。

## Observability (Logging, Metrics, Tracing)

- ログ: テスト内のprintln!は目視確認用。実運用では**構造化ログ**（例: tracing）を用い、**JSONレスポンスの一部のみ**をログに出すことで過剰ログを防止。
- メトリクス: OutputManagerで**成功/失敗件数**、**出力バイト数**をカウントするメトリクスの導入を提案（このチャンクには現れない）。
- トレーシング: リクエストIDや対象SymbolIdをスパンタグとして出力に含めると、後続のデバッグが容易。

## Risks & Unknowns

- 実装不明点
  - JsonResponseの正確なフィールド構成、suggestionsの型・生成ロジックは**このチャンクには現れない**。
  - OutputManager::successのエラー型E、内部のフォーマット分岐やWriterエラー処理は**不明**。
  - Symbol/Range/Idの**バリデーションルール**（例: 有効範囲、負値禁止）は**不明**。
- テスト上のリスク
  - **文字列包含**ベースの検証は脆弱（並び順・空白・キー位置に依存しないが、誤検出の可能性）。
  - **unwrap**によるパニックで、環境や入力が変わった際の不安定性。
  - OutputManagerにWriterを移動しているため、**出力内容の検証ができない**。I/Oの正確性はExitCodeのみで間接的に検証されている。

## Complexity & Performance

- 時間計算量: JsonResponse生成 + JSON整形 O(n)、文字列検索 O(k·m)。
- 空間計算量: JSON文字列のサイズに比例 O(n)。
- ボトルネック: pretty-printのコスト、Writerへの書き込み速度、テストでは軽微。

## Edge Cases, Bugs, and Security

- Rust特有の観点（このチャンクから読み取れる範囲）
  - 所有権: Vec<u8>をBox<dyn Write>へ**移動**（test_output_manager_simple）。戻し手段がないため**出力検証が不可**。
  - 借用/ライフタイム: 参照引数（&symbol）の借用期間は**呼び出し中のみ**で安全。明示的ライフタイムは不要。
  - unsafe境界: **unsafeなし**。
  - 並行性/非同期: Send/Sync、await境界、キャンセル対応は**このチャンクには現れない**。
  - エラー設計: Resultをunwrapしており、**失敗時panic**。From/Intoのエラー変換は**不明**。Option/Resultの使い分けは**不明**。

（上記エッジケース表とチェックリストは先のセクション参照）