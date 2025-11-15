# parsers\kotlin\test_extension_calls.rs Review

## TL;DR

- このファイルは、Kotlinの拡張関数とリテラル型推論に関する解析機能を、Rustのテストで検証するためのもの。
- 主要な外部APIは、**KotlinParser::new**, **LanguageParser::parse**, **KotlinParser::find_method_calls**, **KotlinParser::find_variable_types**。
- コアの複雑箇所は、拡張関数のレシーバ型をシグネチャに反映しているか、メソッド呼び出し検出でレシーバが正しくトラッキングされるか、リテラルからの型推論が期待通りかの検証。
- 重大リスクは、API仕様がこのチャンクには現れないため、返却型や契約の一部が不明な点、unwrapによるパニック、シグネチャがNoneの場合のテスト前提崩壊。
- パフォーマンスは解析対象文字列長に対して概ね線形（推定）。大きなI/Oや並列処理は無し。
- セキュリティ上の懸念は小さいが、ログへの過度な出力やunwrapの乱用は改善余地あり。

## Overview & Purpose

このファイルは、codannaクレートのKotlinパーサ（KotlinParser）を用いて、Kotlinコード断片に対する以下の解析機能が正しく動作するかを確認するRustのユニットテストを含む。

- 拡張関数のシグネチャにレシーバ型が含まれるかの検証（test_extension_function_signatures, L4-L34）。
- 関数型パラメータを介した呼び出し後に拡張関数が正しく解決され、メソッド呼び出しとして検出されるかの検証（test_extension_function_call_tracking, L36-L72）。
- リテラル値（整数、文字列、真偽値）からの型推論が期待通りかの検証（test_literal_type_inference, L74-L106）。

本ファイル自体は公開APIを提供せず、テストのみを実装している。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_extension_function_signatures | private(test) | 拡張関数シグネチャにレシーバ型が含まれるかの検証 | Low |
| Function | test_extension_function_call_tracking | private(test) | メソッド呼び出し検出とレシーバ追跡の検証 | Low |
| Function | test_literal_type_inference | private(test) | リテラルからの型推論の検証 | Low |
| Struct(外部) | KotlinParser | pub（推定、外部） | Kotlinコードの解析器 | Med |
| Trait(外部) | LanguageParser | pub（推定、外部） | 共通のparseインターフェース | Low |
| Struct(外部) | SymbolCounter | pub（推定、外部） | 記号（シンボル）集計の補助 | Low |

### Dependencies & Interactions

- 内部依存
  - なし。各テスト関数は独立している。

- 外部依存（codannaクレート）
  | モジュール | 使用項目 | 用途 |
  |-----------|---------|------|
  | codanna::parsing | LanguageParser, kotlin::KotlinParser | 解析トレイトとKotlinパーサ本体 |
  | codanna::types | SymbolCounter | 解析中のシンボル集計 |

- 被依存推定
  - このテストは、codannaのKotlinパーサ機能の品質保証に依存。CIや開発者ローカルのテストスイートから実行される。

## API Surface (Public/Exported) and Data Contracts

このファイルから公開されるAPIはないため、ここでは「本テストが利用している外部API」を列挙する。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| KotlinParser::new | fn new() -> Result<KotlinParser, E>（推定） | パーサの生成 | O(1) | O(1) |
| LanguageParser::parse | fn parse(&mut self, code: &str, file_id: codanna::FileId, counter: &mut SymbolCounter) -> Vec<Symbol>（推定） | コードからシンボル抽出 | O(n) | O(k) |
| KotlinParser::find_method_calls | fn find_method_calls(&self, code: &str) -> Vec<MethodCall>（推定） | メソッド呼び出し検出 | O(n) | O(m) |
| KotlinParser::find_variable_types | fn find_variable_types(&self, code: &str) -> Vec<(String, String, Range)>（推定） | リテラルや変数の型推論 | O(n) | O(p) |

nは入力コード長、k/m/pは検出されたシンボル/呼び出し/型推論対象の数。

以下、各APIの詳細（このチャンクに現れた使用状況ベースで記述）。

1) KotlinParser::new
- 目的と責務
  - Kotlinコード解析器のインスタンス生成。
- アルゴリズム
  - 初期化のみ。詳細はこのチャンクには現れない。
- 引数
  | 名称 | 型 | 説明 |
  |------|----|------|
  | なし | - | 生成用 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Result<KotlinParser, E>（推定） | 生成成功/失敗 |
- 使用例
  ```rust
  let mut parser = KotlinParser::new().unwrap(); // L14, L53, L82
  ```
- エッジケース
  - newがErrを返す可能性。unwrapでパニック（テストなので許容だが、メッセージ付きexpectが望ましい）。

2) LanguageParser::parse
- 目的と責務
  - Kotlinコードから関数等のシンボル一覧を抽出。
- アルゴリズム（推定）
  - 構文解析→シンボルテーブル生成。拡張関数のレシーバ型をシグネチャに反映。
- 引数
  | 名称 | 型 | 説明 |
  |------|----|------|
  | code | &str | 解析するKotlinコード |
  | file_id | codanna::FileId | ファイル識別子 |
  | counter | &mut SymbolCounter | シンボル集計器 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<Symbol>（推定） | nameやsignatureを持つシンボル |
- 使用例
  ```rust
  let mut counter = SymbolCounter::new();
  let symbols = parser.parse(code, codanna::FileId(1), &mut counter); // L16-L18
  let bar_funcs: Vec<_> = symbols.iter().filter(|s| s.name.contains(".bar")).collect(); // L22
  ```
- エッジケース
  - signatureがNoneのシンボル（L26-L33ではSome前提）。拡張関数のレシーバ型がシグネチャに出ない場合はテスト失敗。

3) KotlinParser::find_method_calls
- 目的と責務
  - コード中のメソッド呼び出しを抽出し、呼び出し元・メソッド名・レシーバ・静的呼び出しかを提示。
- アルゴリズム（推定）
  - 構文木の探索→呼び出し式抽出→receiver/静的判定。
- 引数
  | 名称 | 型 | 説明 |
  |------|----|------|
  | code | &str | 解析するKotlinコード |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<MethodCall>（推定） | caller, method_name, receiver, is_staticフィールドを含む |
- 使用例
  ```rust
  let method_calls = parser.find_method_calls(code); // L54
  let bar_calls: Vec<_> = method_calls.iter().filter(|c| c.method_name == "bar").collect(); // L63-L66
  ```
- エッジケース
  - レシーバ推定がNone/誤判定、拡張関数とメンバ関数の解決順序差異、ジェネリクス経由の型推論失敗。

4) KotlinParser::find_variable_types
- 目的と責務
  - コード内のリテラル・変数に対する型名推論。
- アルゴリズム（推定）
  - リテラルパターンマッチ→型表への変換、または簡易型推論。
- 引数
  | 名称 | 型 | 説明 |
  |------|----|------|
  | code | &str | 解析対象 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | Vec<(String, String, Range)>（推定） | (var_name, type_name, range)のタプル |
- 使用例
  ```rust
  let var_types = parser.find_variable_types(code); // L83
  let int_literal = var_types.iter().find(|(var, _, _)| *var == "42"); // L89
  ```
- エッジケース
  - 文字列リテラルのエスケープ/クォート扱い、真偽値の大小文字、拡張関数連鎖時の型再推論。

データ契約（観測されたフィールド）
- Symbol（推定）: name: String, signature: Option<String>（L18-L22, L26）
- MethodCall（推定）: caller: String, method_name: String, receiver: Option<...>, is_static: bool（L56-L60）
- find_variable_typesの戻り: (var_name: String, type_name: String, range: 不明)（L85）

## Walkthrough & Data Flow

- test_extension_function_signatures（L4-L34）
  1. Kotlin拡張関数Int.barとString.barを定義したコード文字列を作成（L5-L13）。
  2. KotlinParserを生成（L14）。
  3. SymbolCounterを生成（L15）。
  4. parseでシンボル抽出（L16）。
  5. 抽出シンボルからnameに「.bar」を含むものをフィルタ（L22）。
  6. 2件であることをassert（L23）。
  7. signatureがSomeであり、"Int."または"String."を含むことをassert（L26-L33）。

- test_extension_function_call_tracking（L36-L72）
  1. ジェネリック関数foo、拡張関数bar（Int版/String版）、テスト関数でfoo(3).bar(), foo("abc").bar()を呼ぶコードを作成（L37-L52）。
  2. KotlinParser生成（L53）。
  3. find_method_callsで呼び出し一覧を抽出（L54）。
  4. method_nameが"bar"の呼び出しに絞り込み（L63-L66）。
  5. 少なくとも1件以上あることをassert（L71）。

- test_literal_type_inference（L74-L106）
  1. 42.double(), "hello".shout(), true.toString()を含む関数を定義（L75-L81）。
  2. KotlinParser生成（L82）。
  3. find_variable_typesで型推論結果を取得（L83）。
  4. "42"→"Int"、"\"hello\""→"String"、"true"→"Boolean"であることをassert（L89-L105）。

各テストは入力→解析→フィルタ→検証の直線的フローで、共有状態は存在しない。

## Complexity & Performance

- 時間計算量（推定）
  - parse/find_method_calls/find_variable_typesは入力長nに対してO(n)のスキャンまたは構文解析が中心と推定。
- 空間計算量（推定）
  - 出力ベクトルのサイズに比例（O(k), O(m), O(p)）。中間構造（AST等）があれば追加でO(n)。
- ボトルネック
  - 大規模入力時のAST生成/探索。テストの入力は小さいため影響軽微。
- スケール限界
  - 深いネスト/大量のシンボルでメモリ増加。並行解析はこのファイルでは検証していない。
- 実運用負荷要因
  - I/Oなし。CPUのみ。解析アルゴリズムの効率次第。

## Edge Cases, Bugs, and Security

- メモリ安全性
  - このファイルではunsafeなし。標準的な所有権と借用のみ。文字列スライスを渡すため借用の生存期間はテスト関数内に限定され安全。
- Use-after-free/Buffer overflow/Integer overflow
  - 該当なし（このチャンクでは検出されない）。
- インジェクション（SQL/Command/Path traversal）
  - 該当なし。文字列は解析対象であり実行されない。
- 認証・認可
  - 該当なし。
- 秘密情報
  - ハードコード秘密なし。printlnでのログ漏えいも解析情報のみ。
- 並行性
  - マルチスレッドなし。Race/Deadlockの懸念なし。

詳細エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空入力 | "" | シンボル/呼び出し/型は空の結果を返す | このチャンクには現れない | 不明 |
| シグネチャNone | 拡張関数を定義してもsignatureがNone | テスト失敗（panic）か明示的Err | L26-L27でSome前提 | リスクあり |
| レシーバ判定失敗 | foo(3).bar()のreceiver不明 | MethodCallのreceiverがNoneでも検出は継続 | このチャンクには現れない | 不明 |
| 文字列リテラル解析 | "\"hello\"" | Stringに正しくマップ | L92-L98で検証 | OK（テスト） |
| 真偽値の大小文字 | TRUE/True | Booleanにマップできない可能性 | このチャンクには現れない | 不明 |
| 拡張とメンバの競合 | クラスに同名メンバbarあり | メンバ優先で解決 | このチャンクには現れない | 不明 |

## Design & Architecture Suggestions

- テストの明確化
  - signatureのフォーマット（例: "Int.bar(): String"のような確定仕様）をドキュメント化し、テストで厳密比較することで回帰検知精度を上げる。
- データ契約の型定義
  - MethodCallやSymbolの構造体（caller/receiverの型など）を公開し、戻り値のOption/Result境界を明示すると利用側の堅牢性が向上。
- エラー設計
  - new/parse/find_*の失敗時のエラー型を統一し、From/Intoで変換可能にする。テストでもexpectで意味のあるメッセージを付与。
- LanguageParserトレイトの利用範囲
  - parse以外のfind_*もトレイト化を検討すると、他言語パーサとの共通化が可能（このチャンクには現れないため推定）。
- 解析の拡張
  - 拡張関数解決順序（メンバ優先/拡張）やスコープ/インポート解決を含むと現実的な解析に近づく。

## Testing Strategy (Unit/Integration) with Examples

- 追加のユニットテスト案
  - メンバ関数と拡張関数同名競合解決
  - ジェネリック型境界（where句）を伴う拡張関数
  - Nullable型やプラットフォーム型（Java相互運用）でのreceiver解析
  - メソッドチェーン中間の型推論（map/filter等の高階関数経由）

- テスト例（メンバ優先の検証）
  ```rust
  #[test]
  fn test_member_vs_extension_resolution() {
      let code = r#"
      class A {
          fun bar(): String = "member"
      }
      fun A.bar(): String = "extension"
      fun test() {
          val a = A()
          val s = a.bar()
      }
      "#;
      let mut parser = KotlinParser::new().expect("parser init");
      let calls = parser.find_method_calls(code);
      // "a.bar()" がメンバとして解決されているかを確認（具体的なフィールドはこのチャンクには現れない）
      assert!(calls.iter().any(|c| c.method_name == "bar" /* && c.is_member == true */));
  }
  ```

- 型推論の連鎖検証
  ```rust
  #[test]
  fn test_chained_inference() {
      let code = r#"
      fun Int.inc2(): Int = this + 2
      fun test() {
          val x = 40.inc2().toString()
      }
      "#;
      let mut parser = KotlinParser::new().expect("parser init");
      let types = parser.find_variable_types(code);
      // "40" -> Int, "40.inc2()" -> Int, ".toString()" -> String の推論確認（このチャンクには現れない）
      assert!(types.iter().any(|(_, t, _)| t == "String"));
  }
  ```

## Refactoring Plan & Best Practices

- unwrapの使用をexpectに置換し、失敗時に文脈を提供
  ```rust
  let mut parser = KotlinParser::new().expect("Failed to initialize KotlinParser");
  ```
- アサートの厳密化
  - signatureに対して具体的な完全一致や正規表現を使用し、曖昧なcontains検査を減らす。
- 出力の抑制
  - printlnの代わりにテスト失敗時のみ詳細を表示する仕組み（またはtracing）へ移行。
- 再利用性
  - 入力コード断片の構築をヘルパ関数化し、テストの重複を削減。

## Observability (Logging, Metrics, Tracing)

- 現状printlnで観察（L17-L23, L55-L61, L84-L87）。テストでは冗長になりがち。
- 推奨
  - tracingクレートを導入し、debugレベルで出力。テスト実行時はRUST_LOGで制御。
  ```rust
  use tracing::{debug};
  // 例: debug!("Found {} method calls", method_calls.len());
  ```
- メトリクス
  - 解析時間や検出件数を計測する簡易ベンチ（#[bench]やcriterion）を別途用意すると性能回帰検知に有用。

## Risks & Unknowns

- 戻り値型の詳細不明
  - Symbol/MethodCall/Rangeのフィールド・契約はこのチャンクには現れないため、厳密な検証が難しい。
- 拡張関数解決戦略
  - Kotlinの仕様に準じた優先順位（メンバ関数優先、インポート、可視性）はこのチャンクには現れない。
- エラー型とハンドリング
  - new/parse/find_*の失敗時の型と詳細が不明。unwrap依存は回帰検知に有利だが、原因特定が難しい。
- Send/Sync/並行性
  - KotlinParserがSend/Syncかは不明。並列解析の安全性はこのチャンクには現れない。