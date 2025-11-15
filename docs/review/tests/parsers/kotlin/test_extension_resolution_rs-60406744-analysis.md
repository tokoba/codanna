# parsers\kotlin\test_extension_resolution.rs Review

## TL;DR

- 目的: Kotlinの拡張関数解析において、リテラル受け手のメソッド呼び出しと型推論が正しく行われることを、Rust製のテストで検証。
- 主な期待契約: KotlinParser.parseで拡張関数がレシーバ型付き名（例: Int.double）で抽出されること／find_method_callsでリテラル受け手が保持されること／find_variable_typesでリテラルに対応する型が推論されること。
- 複雑箇所: パーサの戻り値仕様（シンボル名の命名規則・メソッド呼び出しのレシーバ表現・型推論の返却タプル構造）に強く依存するアサーション。
- 重大リスク: 出力フォーマット変更（例: 拡張関数名の表記、リテラルの表現）に弱く、テストが脆くなる。unwrap()/assert!によるパニック発生点がある。
- 安全性: このテスト自体はunsafeを使わず、単一スレッド・メモリ安全・競合なし。外部API（KotlinParser）の契約はこのチャンクでは不明。
- パフォーマンス: 入力文字列長に対して実質O(n)想定の処理を3回（parse/find_method_calls/find_variable_types）。本テストの規模では問題なし。
- 改善案: 出力契約を構造化（レシーバ型・関数名を分離フィールドに）し、脆い文字列一致を削減。否定系や競合ケースも追加テストする。

## Overview & Purpose

このファイルはRustの単体テストで、Kotlinのソースコード断片に対して以下を検証します。

- 拡張関数の抽出（受け手型つきのシンボル名を期待）
- リテラルを受け手にしたメソッド呼び出しの抽出（受け手文字列表現を期待）
- リテラルの型推論（Int, String など）結果の取得

対象コード（Kotlin）は Int と String への拡張関数 double/shout を定義し、それをリテラル 42, "hello" から呼び出す最小ケース。テストは codanna::parsing::kotlin::KotlinParser のAPI群に対する期待動作を明確化し、将来的な回 regress を検出する目的です。

関数名: test_extension_function_resolution_with_literals（行番号: 不明 — このチャンクには行番号情報がない）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | test_extension_function_resolution_with_literals | test（非pub） | KotlinParserを用いた拡張関数抽出・メソッド呼び出し抽出・型推論の検証（アサーション含む） | Med |
| 外部型 | KotlinParser | 外部（codanna） | Kotlinコード解析のエントリポイント（parse、find_method_calls、find_variable_types 提供） | 不明 |
| 外部型 | LanguageParser（トレイト） | 外部（codanna） | KotlinParserが実装していると推測される汎用解析トレイト | 不明 |
| 外部型 | SymbolCounter | 外部（codanna） | 解析中のシンボル数え上げ/集計用ユーティリティ | Low |

### Dependencies & Interactions

- 内部依存
  - このファイル内の独自関数/構造体は test 関数のみで、内部呼び出し関係は単純（直列進行）。assert!, println! と標準マクロを使用。
- 外部依存

  | クレート/モジュール | アイテム | 用途 |
  |--------------------|----------|------|
  | codanna::parsing | LanguageParser, kotlin::KotlinParser | Kotlinコードの解析（パース、呼び出し抽出、型推論） |
  | codanna::types | SymbolCounter | シンボル集計の補助 |

- 被依存推定
  - このテストを実行するのはRustのテストハーネス（cargo test）。
  - チーム/CIがKotlinパーサの回 regress 検出に利用。

## API Surface (Public/Exported) and Data Contracts

このファイルからエクスポートされる公開APIはありません（全てテストスコープ）。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| 該当なし | 該当なし | 該当なし | 該当なし | 該当なし |

ただし、外部API（KotlinParser）の「期待されるデータ契約」は以下の通り（このチャンクでは定義不明だが、使用方法から逆推定）。

- KotlinParser::new
  - 期待: Resultのような戻り値を返し、unwrap()可能（失敗しない前提）。型やエラー詳細は不明。
- KotlinParser::parse(code, FileId, &mut SymbolCounter) -> symbols
  - 期待: symbolsはイテレータ/コレクションで、要素に name, kind, signature（Option）が存在。
  - 契約: Kotlin拡張関数は name に「受け手型.関数名」（例: "Int.double"）の形式で入る。
  - 計算量: 入力長 n に対し O(n) 予想。Space は O(s)（シンボル数）。
- KotlinParser::find_method_calls(code) -> method_calls
  - 期待: 要素が caller, method_name, receiver（Option<String>）を持つ。
  - 契約: "42.double()" から method_name="double", receiver=Some("42") が取れる。 "\"hello\".shout()" では receiver=Some("\"hello\"")。
  - 計算量: O(n) 予想。Space は O(c)（呼び出し数）。
- KotlinParser::find_variable_types(code) -> var_types
  - 期待: 要素が (var, typ, _) の3要素タプル。3番目はスパンやスコープ情報の可能性（不明）。
  - 契約: "42" -> "Int", "\"hello\"" -> "String" の対応を含む。
  - 計算量: O(n) 予想。Space は O(t)（推論対象数）。

各APIの使用例（このテストより抜粋）:

```rust
let mut parser = KotlinParser::new().unwrap();
let mut counter = SymbolCounter::new();
let symbols = parser.parse(code, codanna::FileId(1), &mut counter);

let method_calls = parser.find_method_calls(code);
let var_types = parser.find_variable_types(code);
```

エッジケース（期待契約に関わるもの）
- 拡張関数名の表記変更（例: "Int.double" → "double:Int"）によりテストが壊れる。
- 受け手の表現（リテラルのクォートやフォーマット）が変わると一致失敗。
- Kotlinの数値リテラル型推論がInt以外になるケース（例: 42L → Long）。

このチャンクには外部APIの正確な型や定義は現れないため、上記は使用実例からの逆推定です。

## Walkthrough & Data Flow

処理の流れ（test_extension_function_resolution_with_literals: 行番号不明）:
1. Kotlinのサンプルコード文字列（拡張関数 Int.double, String.shout とその呼び出し）を作成。
2. KotlinParser を生成（new().unwrap()）。
3. parse によりシンボル抽出。拡張関数が name="Int.double" / "String.shout" で現れることを assert。
4. find_method_calls によりメソッド呼び出し抽出。double/shout がそれぞれ receiver=Some("42") / Some("\"hello\"") であることを assert。
5. find_variable_types により型推論結果を取得。リテラル "42" → "Int", "\"hello\"" → "String" であることを assert。
6. 進捗を println! でデバッグ出力。

Mermaidフローチャート（主要分岐・アサーション）:
```mermaid
flowchart TD
  A[開始: Kotlinサンプル文字列を用意] --> B[KotlinParser::new().unwrap()]
  B --> C[parse(code, FileId(1), &mut counter)]
  C --> D{symbolsに\nInt.doubleがあるか}
  D -- Yes --> E{symbolsに\nString.shoutがあるか}
  D -- No --> DX[テスト失敗: panic]
  E -- Yes --> F[find_method_calls(code)]
  E -- No --> EX[テスト失敗: panic]
  F --> G{double呼び出しがあり\nreceiver==Some("42")か}
  G -- Yes --> H{shout呼び出しがあり\nreceiver==Some("\"hello\"")か}
  G -- No --> GX[テスト失敗: panic]
  H -- Yes --> I[find_variable_types(code)]
  H -- No --> HX[テスト失敗: panic]
  I --> J{"42" → "Int"か}
  J -- Yes --> K{"\"hello\"" → "String"か}
  J -- No --> JX[テスト失敗: panic]
  K -- Yes --> L[成功: 期待どおり]
  K -- No --> KX[テスト失敗: panic]
```

上記の図は test_extension_function_resolution_with_literals 関数（ファイル全体、行番号情報なし）の主要分岐を示す。

参考抜粋:
```rust
// 拡張関数の存在確認
let int_double = symbols.iter().find(|s| s.name.as_ref() == "Int.double");
assert!(int_double.is_some(), "Should find Int.double extension function");
/* ... 省略 ... */
let string_shout = symbols.iter().find(|s| s.name.as_ref() == "String.shout");
assert!(string_shout.is_some(), "Should find String.shout extension function");
```

## Complexity & Performance

- 時間計算量（推定）
  - parse: O(n)
  - find_method_calls: O(n)
  - find_variable_types: O(n)
  - テスト全体: O(n)を3回。nは入力コード文字列長。
- 空間計算量（推定）
  - シンボル/呼び出し/型推論結果の個数に比例（O(s) + O(c) + O(t)）。
- ボトルネック
  - 実コードが短いため実質なし。大型ファイルではパース段階が支配的となる想定。
- 実運用負荷要因
  - I/O: println!の標準出力は微小。
  - ネットワーク/DB: なし。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト（このテスト単体に対して）
- メモリ安全性: unsafe未使用。所有権と借用はRust規則に従い安全。
- インジェクション: 外部入力なし、SQL/コマンド/パスは扱わないため影響低。
- 認証・認可: 該当なし。
- 秘密情報: ハードコードされた秘密なし。ログに機密なし。
- 並行性: 単一スレッド、レース/デッドロックの懸念なし。

Rust特有の観点
- 所有権/借用
  - KotlinParserは可変変数として保持（let mut parser）。parseは &mut SymbolCounter を借用。ライフタイム問題なし。
- unsafe境界
  - unsafeブロックなし（行番号不明）。
- 並行性・非同期
  - 非同期/スレッドなし。Send/Sync要件に関与せず。
- エラー設計
  - KotlinParser::new().unwrap() がパニック点。テストでは許容されうるが、失敗時の情報が限定的。
  - assert! と assert_eq! により、期待不一致でpanic。

潜在的なバグ/脆さ
- 文字列一致に依存: シンボル名の表記やリテラルの表現が変更されるとテストが壊れやすい。
- 型推論の地域差: Kotlinの将来変更や方言（Kotlin/Native, JS）で Int/Long 推論差異が出ると失敗する可能性。
- var_types のキーが「リテラル値」: 変数 x/y ではなく "42"/"\"hello\"" をキーとして扱う仕様に依存しており、設計的に直観的でない可能性。

エッジケース一覧（本テスト未カバーを含む）

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空入力 | "" | シンボル/呼び出し/型なし | 不明 | このチャンクには現れない |
| Longリテラル | 42L.double() | 受け手型がLong、Long向け拡張に解決 | 不明 | このチャンクには現れない |
| エスケープ文字列 | "\"he\"llo\"".shout() | 受け手が正しい文字列として抽出 | 不明 | このチャンクには現れない |
| 同名拡張の多重定義 | Int.double と Number.double | 最適な受け手型へ解決 | 不明 | このチャンクには現れない |
| 変数受け手 | val x=42; x.double() | receiver が "x" として抽出 | 不明 | このチャンクには現れない |
| 型注釈あり | val x: Int = 42; x.double() | 型注釈の型で解決 | 不明 | このチャンクには現れない |
| インポート必要 | 別モジュールの拡張 | 正しく解決/参照 | 不明 | このチャンクには現れない |

## Design & Architecture Suggestions

- シンボル表現の構造化
  - name="Int.double" のような結合文字列ではなく、receiver_type と function_name を分離したフィールドを持つ構造体で契約を固定化。将来の表記変更に強くなる。
- メソッド呼び出しの受け手
  - receiver を生文字列ではなく、種別（Literal/Identifier/This/Qualified）と値を分離した型で返すと頑健に。
- 型推論の結果
  - (var, typ, _) のタプルではなく、識別子種別（literal/var）、型、ソース範囲（span）などを持つ専用型を返す。
- エラー排出
  - KotlinParser::new の unwrap 依存を減らすため、テストでもエラー内容を検査できる形に（map_err + panic! で詳細出力など）。
- 将来機能に備えた拡張
  - マルチファイル/インポート解決を視野に、FileId とシンボルのスコープ/モジュール情報を強化。

## Testing Strategy (Unit/Integration) with Examples

追加でカバーすべきユースケース（ユニットテスト）

1) 変数受け手と型注釈
```rust
#[test]
fn resolves_extension_on_variable_with_annotation() {
    let code = r#"
        fun Int.double(): Int = this * 2
        fun test() {
            val x: Int = 41
            val y = x.double()
        }
    "#;
    let mut parser = KotlinParser::new().unwrap();
    let calls = parser.find_method_calls(code);
    assert!(calls.iter().any(|mc| mc.method_name=="double" && mc.receiver.as_deref()==Some("x")));
    // 型推論で x: Int か、または "x" に対する型情報が得られることを確認
    let tys = parser.find_variable_types(code);
    assert!(tys.iter().any(|(v,t,_)| v=="x" && t=="Int"));
}
```

2) 数値リテラルの型差
```rust
#[test]
fn resolves_long_literal_extension() {
    let code = r#"
        fun Long.double(): Long = this * 2
        fun test() {
            val x = 42L.double()
        }
    "#;
    let mut parser = KotlinParser::new().unwrap();
    let syms = parser.parse(code, codanna::FileId(1), &mut SymbolCounter::new());
    assert!(syms.iter().any(|s| s.name.as_ref()=="Long.double"));
    let calls = parser.find_method_calls(code);
    assert!(calls.iter().any(|mc| mc.method_name=="double" && mc.receiver.as_deref()==Some("42L")));
}
```

3) 多重解決の曖昧性
```rust
#[test]
fn prefers_more_specific_extension() {
    let code = r#"
        fun Number.double(): Number = this.toInt() * 2
        fun Int.double(): Int = this * 2
        fun test() {
            val y = 1.double()
        }
    "#;
    let mut parser = KotlinParser::new().unwrap();
    // 望ましくは Int.double に解決されることを検証（契約にこのレベルの解決があるなら）
    // 解決先の識別方法が構造化されていない場合は name で代用
    let syms = parser.parse(code, codanna::FileId(1), &mut SymbolCounter::new());
    assert!(syms.iter().any(|s| s.name.as_ref()=="Int.double"));
}
```

4) インポートを跨ぐ拡張（将来の統合試験）
- 異なるファイルIDの拡張関数をimportで解決できるかを検証。

ベストプラクティス
- 期待結果の比較に文字列結合でなく構造化データを使用。
- println!を減らし、失敗時メッセージ（assert!の第2引数）で必要情報を提供。
- パラメタライズドテスト（複数ケースをデータ駆動で）にまとめる。

## Refactoring Plan & Best Practices

- アサーションヘルパの導入
  - 例: assert_has_symbol(&symbols, receiver, name) のようなヘルパで重複削減。
- 出力契約の明確化
  - シンボル/呼び出し/型推論の戻り値を構造体化し、テストはフィールド比較に移行。
- unwrapの置換
  - KotlinParser::new().expect("...") で失敗理由を明確化。
- ログ抑制
  - テスト出力を減らし、必要時のみ RUST_LOG/tracing で可視化。
- 名前のマジック文字列依存を低減
  - "Int.double", "\"hello\"" のような脆い文字列一致をフィールド比較に置換。

## Observability (Logging, Metrics, Tracing)

- ログ
  - println!ではなくtracing（info!/debug!/trace!）を採用し、テスト実行時に環境変数でログレベル制御。
- メトリクス
  - 解析で抽出したシンボル数・呼び出し数・型推論数をカウントし、ベンチマークで監視。
- トレーシング
  - parse/find_method_calls/find_variable_types に span を付与し、複雑な入力での問題切り分けを容易に。

## Risks & Unknowns

- Unknowns
  - KotlinParserと関連型（Symbol、MethodCall、VariableTypeなど）の正確な定義・戻り値・計算量はこのチャンクには現れない。
  - 受け手表現やシンボル名の表記は内部実装依存で、将来変更の可能性。
- Risks
  - テストが実装詳細（文字列表現）に強く依存しており、非本質的な変更で壊れる。
  - Kotlinの型推論仕様やターゲット（JVM/JS/Native）差異が反映されるとテストが誤検知を起こす可能性。
  - マルチファイル/名前解決・インポート・可視性（private/internal）などの実運用シナリオは未検証。