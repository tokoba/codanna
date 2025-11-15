# test_call_tracking.rs Review

## TL;DR

- 目的: TypeScriptParser の呼び出し検出機能（find_calls）が基本的に動作するかを確認する単体テスト。
- 公開API: このファイル自体の公開APIは存在しない。外部APIとして TypeScriptParser::new と find_calls を使用。
- コアロジック: TypeScriptコード文字列を解析し、検出された呼び出し一覧を出力・非空であることを検証。
- 複雑箇所: 複雑な分岐なし。実質的な複雑さは外部の find_calls 実装に依存。
- 重大リスク: 期待値が「非空」のみで具体性欠如。誤検知・未検知・行番号のズレなどを捕捉できない。
- Rust安全性: unsafeなし、同期なし、エラーは expect によりテスト時に panic へ。所有権/借用の範囲は単純。
- 不明点: find_calls の返却型・計算量・検出仕様（メソッド呼び出し、this参照、矢印関数などの取り扱い）はこのチャンクには現れない。

## Overview & Purpose

このファイルは Rust のテストモジュールであり、TypeScriptParser を用いて TypeScriptコード内の関数・メソッド呼び出しを抽出する機能の動作確認を目的としています。テストでは複数の形態（通常関数、矢印関数、クラスメソッド）に含まれる呼び出し（console.log、otherFunction、helperFunction、this.otherMethod）を含むコードスニペットを解析し、検出結果が空でないことを確認します。出力ログは解析された呼び出しの呼び元・呼び先・行番号を表示します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Module | tests | private (cfg(test)) | テストをグルーピング | Low |
| Function | test_typescript_call_tracking | private (#[test]) | TypeScriptParser の呼び出し検出の基本ケースを検証 | Low |

### Dependencies & Interactions

- 内部依存:
  - 該当なし（このファイル内では単一のテスト関数のみ。補助関数や構造体は未定義）

- 外部依存（このチャンクでは定義なし／参照のみ）:

  | 依存 | 種別 | 由来 | 役割 | 備考 |
  |------|------|------|------|------|
  | codanna::parsing::LanguageParser | Trait | 外部クレート内 | TypeScriptParser が実装していると推測される共通インターフェース | 詳細不明（このチャンクには現れない） |
  | codanna::parsing::typescript::TypeScriptParser | Struct | 外部クレート内 | TypeScriptコードの解析器。new と find_calls を使用 | 実装不明（このチャンクには現れない） |

- 被依存推定:
  - このテストは CI や `cargo test` により実行されるユニットテストの一部として機能。TypeScript の呼び出し解析機能の開発・回帰検知に依存される。

## API Surface (Public/Exported) and Data Contracts

このファイルに公開APIは存在しません（すべてテスト用・非公開）。外部API使用状況を含めた一覧を示します。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| tests::test_typescript_call_tracking | fn test_typescript_call_tracking() | TypeScript の呼び出し検出が少なくとも1件以上返ることを検証 | 不明（find_calls依存） | 不明（find_calls依存） |
| TypeScriptParser::new | 不明（Result/Optionを返す可能性） | パーサインスタンス生成 | 不明 | 不明 |
| LanguageParser::find_calls（TypeScriptParser 実装） | 不明（&str入力、コレクション返却を推測） | 渡されたコードから呼び出しを解析して列挙 | 不明 | 不明 |

詳細（このファイル内での使用状況の説明）:
1. 目的と責務
   - test_typescript_call_tracking: TypeScriptParser の基本的な呼び出し検出が機能するかのスモークテスト。
   - TypeScriptParser::new: パーサを初期化。失敗時は expect によりテストが panic。
   - find_calls: コード文字列から呼び出しの一覧を抽出。返却要素はループで `(caller, called, range)` と分解でき、`range.start_line` が参照できることから各要素は少なくとも3要素のタプル（または同様の構造）であると推測。

2. アルゴリズム（test_typescript_call_tracking のステップ）
   - TypeScriptコードスニペットを文字列で構築。
   - TypeScriptParser を new で生成し、expect で生成失敗時に即時 panic。
   - find_calls で呼び出し一覧を取得。
   - 得られた一覧を出力（呼び元・呼び先・開始行）。
   - calls が空でないことを assert。

3. 引数（test_typescript_call_tracking）
   | 名前 | 型 | 必須 | 説明 |
   |------|----|------|------|
   | なし | - | - | テスト関数のため引数なし |

4. 戻り値（test_typescript_call_tracking）
   | 型 | 説明 |
   |----|------|
   | () | テストフレームワークによって成功/失敗が評価される |

5. 使用例
   このファイル内のテストがそのまま使用例です。抜粋します。

   ```rust
   #[test]
   fn test_typescript_call_tracking() {
       let code = r#"
   function test() {
       console.log('hello');
       otherFunction();
   }

   const arrow = () => {
       console.log('arrow');
       helperFunction();
   };

   class MyClass {
       method() {
           console.log('method');
           this.otherMethod();
       }
   }
   "#;

       let mut parser = TypeScriptParser::new().expect("Failed to create parser");
       let calls = parser.find_calls(code);

       for (caller, called, range) in &calls {
           println!("{} -> {} @ {}", caller, called, range.start_line);
       }

       assert!(!calls.is_empty(), "Should find at least some function calls");
   }
   ```

6. エッジケース
   - パーサ生成に失敗する（new が Err/None を返す）
   - 空文字列やコメントのみのコード
   - this.メソッド呼び出しの扱い
   - 矢印関数内の呼び出し検出
   - 同一行に複数の呼び出しがある場合の行番号
   - テンプレート文字列内の括弧や識別子の誤検出

注: find_calls の正確なシグネチャや返却型・複雑度はこのチャンクには現れないため不明。

## Walkthrough & Data Flow

- 入力データ: TypeScriptソース文字列（複数の関数・メソッド・矢印関数を含む）
- 処理の流れ:
  1. 文字列 code を定義。
  2. TypeScriptParser::new でパーサインスタンスを生成（失敗時は expect により panic）。
  3. parser.find_calls(code) を呼び、呼び出しの一覧 calls を得る。
  4. calls を反復し、各要素 `(caller, called, range)` を取り出してログ出力。range.start_line を使用。
  5. calls が空でないことを assert。
- データ構造:
  - calls: 反復可能なコレクション（推測: Vec<(String, String, Range)> など）。このチャンクでは具体型は不明。
  - range: start_line フィールドを持つ構造体（詳細不明）。

この流れは直線的で条件分岐が少なく、Mermaid図を作成するほどの複雑さはありません。

## Complexity & Performance

- 時間計算量:
  - test_typescript_call_tracking 自体: O(n) 推測（n は code の長さ）。主に find_calls に依存。
  - 出力ループ: O(k)（k は検出された呼び出し数）。
- 空間計算量:
  - calls の格納に O(k)。その他は定数オーバーヘッド。
- ボトルネック:
  - 実質的に find_calls の字句/構文解析の計算量が支配的（詳細不明）。
- スケール限界・負荷要因:
  - 巨大な TypeScript ソースを解析する場合、find_calls のアルゴリズム特性（正規表現ベースか AST ベースか等）により性能が大きく変動。I/O やネットワークは関与しない。

## Edge Cases, Bugs, and Security

- 全般
  - 期待値の弱さ: 非空のみの assert では品質を担保できない。誤検出・漏れ検出・位置情報の誤りを見逃す可能性あり。
  - 標準出力: `cargo test` では既定で標準出力が抑制されるため、ログは失敗時にのみ表示されることが多い（`-- --nocapture` で表示可能）。

- セキュリティチェックリスト
  - メモリ安全性: unsafe 使用なし。このチャンクではバッファオーバーフロー・Use-after-free・整数オーバーフローの懸念なし。
  - インジェクション:
    - SQL/Command/Path traversal: 関与なし。入力は固定のコード文字列。
  - 認証・認可: 関与なし。
  - 秘密情報: ハードコードされた秘密情報なし。ログにセンシティブ情報は出力しない。
  - 並行性:
    - レースコンディション/デッドロック: 同期処理のみで関連なし。

- Rust特有の観点（詳細チェック）
  - 所有権: `code` はローカル所有の `String`（厳密には `&'static str` リテラル）であり、`find_calls(code)` に借用で渡されると推測。calls はローカル所有で、`&calls` でイテレーション（不変借用）。
  - 借用: 可変借用は `parser` に対して行われる可能性がある（`let mut parser`）。find_calls のレシーバが `&mut self` か `&self` かは不明（このチャンクには現れない）。
  - ライフタイム: 明示的ライフタイム指定は不要。関数スコープに収まる。
  - unsafe 境界: unsafe ブロックなし。
  - 並行性・非同期: Send/Sync の議論不要。await 境界なし。キャンセル対応不要。
  - エラー設計:
    - Result vs Option: `new().expect(...)` から、`new` は `Result<T, E>` もしくは `Option<T>` を返すと推測。
    - panic 箇所: `expect("Failed to create parser")` はテストでは妥当。実運用コードではエラー伝播が望ましい。
    - エラー変換: From/Into の使用なし。

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | 呼び出しなし（空の結果） | テスト未実施 | 不明 |
| コメントのみ | "// a\n/* b */" | 呼び出しなし | テスト未実施 | 不明 |
| 矢印関数内呼び出し | `const f=()=>{helper();}` | helper の検出 | 現テストに含まれるが具体的アサートなし | 要強化 |
| this 経由メソッド | `this.m();` | otherMethod/method の検出（呼び元の特定可否は仕様次第） | 現テストに含まれるが具体的アサートなし | 要強化 |
| 同一行複数呼び出し | `a(); b();` | 2件検出、行番号が同一 | テスト未実施 | 不明 |
| テンプレート文字列内の擬似呼び出し | `` `${fn()}` `` | 誤検出しない | テスト未実施 | 不明 |
| ネスト・コールチェーン | `obj.a().b()` | 各段階の検出仕様に従う | テスト未実施 | 不明 |

根拠（重要箇所）：`test_typescript_call_tracking` 関数内の `expect` と `assert!(!calls.is_empty())` のみで検証が終わっている点（このチャンクには行番号情報が一致しないため、関数名のみを根拠として明記）。

## Design & Architecture Suggestions

- 期待値の明確化:
  - 検出される呼び出しの正確な集合（caller, called, line）をテストで明示し、完全一致または部分一致を検証。
  - メソッド呼び出し（this.someMethod）の caller 名やクラス・メソッド境界の扱いを仕様化し、それに準じたアサーションを追加。
- テストのパターン化:
  - テーブルドリブンテスト（入力・期待出力の組を列挙）で多様なケースを網羅。
- ヘルパーの導入:
  - 比較用の正規化関数（並び順非依存の比較、重複除去、行番号検証）を用意。
- OS/改行差異対応:
  - CRLF/LF の違いで行番号がずれないかを検証（Windows/Unix）。
- 出力抑制と診断性:
  - `println!` 依存を減らし、失敗時のみ差分を表示するヘルパー（例: pretty_assertions）を活用。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテストの強化例（期待値の明示）

```rust
#[test]
fn test_typescript_call_tracking_strict() {
    let code = r#"
function test() {
    console.log('hello');
    otherFunction();
}

const arrow = () => {
    console.log('arrow');
    helperFunction();
};

class MyClass {
    method() {
        console.log('method');
        this.otherMethod();
    }
}
"#;

    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    let calls = parser.find_calls(code);

    // 期待される called 名のセット（caller/line の扱いは仕様次第）
    let expected_called = ["console.log", "otherFunction", "console.log", "helperFunction", "console.log", "this.otherMethod"];

    // called 名のみ抽出（返却型詳細は不明のため擬似コード的に記述）
    // 実際には `for (caller, called, range)` を利用して `called` を収集
    let mut called_names = Vec::new();
    for (_, called, _) in &calls {
        called_names.push(called.as_str()); // as_str は仮。実際の型に合わせて調整
    }

    // 少なくとも期待対象を包含していることを確認
    for name in expected_called {
        assert!(called_names.contains(&name), "missing call: {}", name);
    }

    assert!(!calls.is_empty(), "Should find at least some function calls");
}
```

- エッジケーステスト例（空文字列）

```rust
#[test]
fn test_typescript_call_tracking_empty() {
    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    let calls = parser.find_calls("");
    assert!(calls.is_empty(), "Empty code should yield no calls");
}
```

- 位置情報のテスト例（行番号検証）

```rust
#[test]
fn test_typescript_call_tracking_line_numbers() {
    let code = "function f(){\n  a(); b();\n}\n";
    let mut parser = TypeScriptParser::new().expect("Failed to create parser");
    let calls = parser.find_calls(code);

    // 同一行に2件あることを確認（仕様上可能であれば）
    let mut lines = Vec::new();
    for (_, _, range) in &calls {
        lines.push(range.start_line);
    }
    assert!(lines.iter().filter(|&&l| l == 2).count() >= 2,
            "Expected multiple calls on line 2");
}
```

- プロパティベーステスト（提案）
  - 無害な文字挿入（空白・コメント）で検出結果が不変であること。
  - テンプレート文字列内の括弧を追加しても誤検出しないこと。

## Refactoring Plan & Best Practices

- テスト構造の改善:
  - テーブルドリブンヘルパーを作成し、入力と期待結果を簡潔に記述できるようにする。
- 期待値の型定義:
  - 返却型がタプルなら比較用の型（例えば `struct Expected { caller: String, called: String, line: usize }`）を定義。
- アサーションの詳細化:
  - 完全一致（件数・要素内容・位置情報）で検証し、回帰を早期検知。
- ログの削減:
  - `println!` を必要なときのみ出す（`-- --nocapture` 前提の依存を避ける）。

## Observability (Logging, Metrics, Tracing)

- テスト時の観測:
  - 失敗時にのみ差分を表示（例: 期待 vs 実際の一覧を整形）。
- パーサ側（外部実装への提案）:
  - 検出件数・解析時間のメトリクス化（計測はテスト内でベンチ的に行うことも可能）。
  - ログレベルで字句/構文段階の診断を切り替え可能に（debug ログ）。

## Risks & Unknowns

- 返却型とデータ契約の不明確さ:
  - `(caller, called, range)` の具体型、range の定義がこのチャンクには現れない。
- 検出仕様の不明確さ:
  - メソッドチェーン、動的プロパティアクセス、import/require 経由の呼び出し、ジェネリック・オーバーロードの取り扱いなど。
- プラットフォーム差異:
  - 改行コード差による行番号ズレの可能性。
- テストの脆弱性:
  - 非空判定のみのため、誤検出・未検出の回帰を見逃す危険が高い。