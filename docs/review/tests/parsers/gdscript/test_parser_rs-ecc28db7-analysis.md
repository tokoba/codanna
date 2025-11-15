# test_parser.rs Review

## TL;DR

- 目的: GDScriptコードから、**モジュール**, **クラス**, **フィールド**, **メソッド**, **関数**, **ドキュメンテーションコメント**, **可視性(Visibility)**, **継承関係(extends)** を抽出できることをテスト。
- 主要API: 外部のcodannaからの **GdscriptParser::new**, **parse**, **find_extends**, 型 **FileId**, **SymbolCounter**, **SymbolKind**, **Visibility** を利用。
- コアロジック: `symbols.iter().find(...)` による抽出検証、`parser.find_extends(code)` による継承関係確認。
- 複雑箇所: 先頭アンダースコアでの**プライベート可視性推定**、**ドキュメントコメントの紐付け**の正しさ、**継承情報のタプル**形式（第3要素が不明）。
- 重大リスク: 返却型が不明なため**データ契約の厳密性が不明**、`expect`による**テストパニック**依存、モジュール名を**"<script>"**と決め打ち。
- 不明点: `parse`の返却型詳細、`find_extends`のタプル第3要素、`symbols`内部構造、LOCメタ情報の関数数(3)と本チャンクの乖離。

## Overview & Purpose

このファイルは、codannaのGDScriptパーサが以下を正しく抽出できるかを単体テストする目的で作成されています。

- スクリプト単位の**モジュール記号**の生成（名前が"<script>"）
- **クラス宣言**（`class Player`）と直前の**ドキュメンテーションコメント**の取り込み
- クラス内の**フィールド**（`speed`）の抽出と`SymbolKind::Field`の付与
- 先頭アンダースコアの**コンストラクタ/メソッド**（`_init`）を**Private**として扱う可視性判定
- スクリプトスコープの**関数**（`helper`）の抽出とドキュメントコメントの保持
- **継承関係**の抽出（`Player extends CharacterBody2D`）

テストは、codannaの外部APIに依存しており、パーサが返すシンボル集合の構造体フィールド（`name`, `kind`, `signature`, `doc_comment`, `visibility`）を利用して検証します。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | build_parser | private (テスト内) | GdscriptParserとFileId、SymbolCounterの生成 | Low |
| Function | test_gdscript_parser_extracts_core_symbols | private (#[test]) | GDScriptサンプルの解析と抽出結果の検証一式 | Med |
| Variable | code (GDScriptサンプル) | 関数ローカル | 解析対象のGDScript入力 | Low |

注: メタ情報の「functions=3」に対し、本チャンクで確認できるRust関数は2つです。整合性は不明。

### Dependencies & Interactions

- 内部依存
  - `test_gdscript_parser_extracts_core_symbols` → `build_parser`（パーサとカウンタ、ファイルIDの用意）
  - `test_gdscript_parser_extracts_core_symbols` → `parser.parse`, `parser.find_extends`
  - `test_gdscript_parser_extracts_core_symbols` → `symbols.iter().find`, `symbols.iter().any`
- 外部依存（codanna）
  - | クレート/モジュール | アイテム | 用途 |
    |---------------------|---------|------|
    | codanna::parsing::gdscript | GdscriptParser | GDScriptパーサのインスタンス化と機能利用 |
    | codanna::parsing | LanguageParser | トレイト（推定）: `parse`が属する可能性 |
    | codanna | Visibility | シンボルの可視性判定 |
    | codanna::types | FileId | ファイル識別子の生成 |
    | codanna::types | SymbolCounter | シンボルカウンタの管理 |
    | codanna::types | SymbolKind | シンボル種別（Module, Field, Method, Functionなど） |
- 被依存推定
  - このモジュールは単体テストであり、外部から直接使用される想定はありません（テストランナーのみ）。

## API Surface (Public/Exported) and Data Contracts

このファイル自体に公開APIはありません。テストが利用する外部APIおよびローカルヘルパーを一覧します。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| build_parser | fn build_parser() -> (GdscriptParser, FileId, SymbolCounter) | テスト用のパーサ環境を構築 | O(1) | O(1) |
| GdscriptParser::new | fn new() -> Result<GdscriptParser, E>（推定） | パーサのインスタンス化 | O(1) | O(1) |
| FileId::new | fn new(u64) -> Result<FileId, E>（推定） | ファイルIDの生成 | O(1) | O(1) |
| SymbolCounter::new | fn new() -> SymbolCounter | シンボルカウンタの初期化 | O(1) | O(1) |
| GdscriptParser::parse | fn parse(&self or &mut self, &str, FileId, &mut SymbolCounter) -> Iterable<Symbol>（推定） | GDScriptの解析とシンボル抽出 | O(n)（入力長・記号数に依存） | O(k)（抽出記号数） |
| GdscriptParser::find_extends | fn find_extends(&self or &mut self, &str) -> Vec<(String, String, ?)>（推定） | 継承関係(派生, 基底, 付随情報)抽出 | O(m)（継承宣言数） | O(m) |

各APIの詳細説明:

1) build_parser
- 目的と責務
  - テストで必要なパーサと補助構造を初期化して返す。
- アルゴリズム（ステップ）
  - `GdscriptParser::new()`を呼び出し`expect`でエラー時にパニック。
  - `FileId::new(1)`を呼び出し`expect`でエラー時にパニック。
  - `SymbolCounter::new()`でカウンタ初期化。
  - タプルで返却。
- 引数
  - なし
- 戻り値
  - (GdscriptParser, FileId, SymbolCounter)
- 使用例
  ```rust
  let (mut parser, file_id, mut counter) = build_parser();
  ```
- エッジケース
  - GdscriptParserの生成失敗 → `expect`で即パニック。
  - FileId生成失敗 → `expect`で即パニック。

2) GdscriptParser::parse（外部API、署名は推定）
- 目的と責務
  - 入力GDScript文字列を解析し、**シンボル集合**を返す。
- アルゴリズム（ステップ）
  - 不明（このチャンクには現れない）。結果は`symbols.iter()`で反復可能。
- 引数
  - | 引数 | 型 | 説明 |
    |------|----|------|
    | code | &str | 解析対象のGDScript |
    | file_id | FileId | ファイル識別子 |
    | counter | &mut SymbolCounter | シンボル採番/統計用（推定） |
- 戻り値
  - 反復可能なシンボル集合（具象型は不明）
- 使用例
  ```rust
  let symbols = parser.parse(code, file_id, &mut counter);
  let module_symbol = symbols.iter().find(|s| s.kind == SymbolKind::Module).unwrap();
  ```
- エッジケース
  - 空文字列の入力。
  - ドキュメントコメントの紐付け位置が曖昧な場合。
  - 先頭アンダースコア可視性の判定ルールが一致しない場合。

3) GdscriptParser::find_extends（外部API、署名は推定）
- 目的と責務
  - `class X extends Y`の関係抽出。
- アルゴリズム（ステップ）
  - 不明（このチャンクには現れない）。テストでは`Vec<(derived, base, _)>`として利用。
- 引数
  - | 引数 | 型 | 説明 |
    |------|----|------|
    | code | &str | 解析対象のGDScript |
- 戻り値
  - `Vec<(String, String, ?)>`（第3要素は不明）
- 使用例
  ```rust
  let extends = parser.find_extends(code);
  assert!(extends.iter().any(|(d, b, _)| *d == "Player" && *b == "CharacterBody2D"));
  ```
- エッジケース
  - `extends`の複数指定、ネスト、前後の空白やコメント混在。

4) symbols（返却値の要素構造）
- データ契約（利用しているフィールドのみ）
  - name: 参照可能な文字列（`as_ref()`が呼べる）
  - kind: `SymbolKind`（Module, Field, Method, Function 等）
  - signature: Option<&str>（`as_deref()`で参照取得）
  - doc_comment: Option<&str>
  - visibility: `Visibility`（Private等）
- 使用例
  ```rust
  let player_class = symbols.iter()
      .find(|s| s.signature.as_deref() == Some("class Player"))
      .unwrap();
  assert_eq!(speed_field.kind, SymbolKind::Field);
  assert_eq!(init_method.visibility, Visibility::Private);
  ```

（根拠: いずれも `test_gdscript_parser_extracts_core_symbols` 内でのフィールドアクセス。行番号はこのチャンクに含まれず不明）

## Walkthrough & Data Flow

- 入力準備
  - GDScript文字列`code`を定義（クラス、シグナル、フィールド、定数、メソッド、関数を含む）
- パーサ初期化
  - `build_parser()`で`parser`, `file_id`, `counter`を取得
- 解析
  - `symbols = parser.parse(code, file_id, &mut counter)`
- 検証（線形走査）
  - モジュール記号を`kind == Module`で検索し、`name == "<script>"`を確認
  - `signature == "class Player"`のシンボルを検索し、`doc_comment`に「Player character implementation」を含むことを確認
  - `name == "speed"`のシンボルを検索し、`kind == Field`を確認
  - `name == "_init"`のシンボルを検索し、`kind == Method`, `visibility == Private`、`doc_comment`に「Creates a new player instance」を含むことを確認
  - `name == "helper"`のシンボルを検索し、`kind == Function`かつドキュメントに「Utility helper」を含むことを確認
- 継承関係抽出
  - `extends = parser.find_extends(code)`
  - `(derived, base, _) == ("Player", "CharacterBody2D", _)`が含まれることを確認

データフローの特性:
- `parse`はシンボルコレクションを返却し、テストは**不変参照**で走査（`iter()`）する。
- `counter`は**可変借用**で渡されるため、解析中に内部カウントが更新される可能性がある（具体は不明）。
- `find_extends`は入力文字列から継承関係の**リスト**を返し、テストは`any`で検証。

## Complexity & Performance

- 走査コスト
  - `symbols.iter().find(...)`を複数回実行しており、各検索は**O(n)**（n = シンボル数）。総計でもテストの観点で問題なし。
  - `extends.iter().any(...)`は**O(m)**（m = 継承関係抽出数）。
- 解析コスト（推定）
  - `parse`自体は入力長や構文構造に依存し、おそらく**O(|code|)**〜**O(number of tokens)**（詳細はこのチャンクに現れない）。
- スケール限界
  - 大規模スクリプトでは`find`の多用による線形検索の繰り返しがテスト時間を増やすが、単体テストとしては許容範囲。
- 実運用負荷要因
  - I/O無し。本テストは純粋CPU処理であり、外部リソース負荷はない。

## Edge Cases, Bugs, and Security

- メモリ安全性
  - 本テストは**安全なRust**のみ。`unsafe`は使用なし。所有権/借用は通常の範囲（`&mut counter`の可変借用は`parse`呼び出し期間に限定）。
- インジェクション
  - 外部I/OやDB無し、コマンド実行無しのため該当なし。
- 認証・認可
  - 該当なし。
- 秘密情報
  - ハードコード秘密なし。ログ出力なし。
- 並行性
  - 非同期/並行処理なし。**Race/Deadlock**の懸念なし。

詳細エッジケース表（本テストでの状態も記載）:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | Errや空シンボル | このチャンクには現れない | 未検証 |
| モジュール記号なし | codeにトップレベル要素なし | 生成またはNone | 生成されると期待（"<script>"） | 検証済み（期待通り） |
| クラスDocなし | `class X`のみ | `doc_comment == None` | このチャンクには現れない | 未検証 |
| 先頭アンダースコア可視性 | `_init` | Private扱い | 期待を検証 | 検証済み |
| フィールド種別誤判定 | `var speed`がField以外 | Fieldであるべき | 検証あり | 検証済み |
| 継承抽出の誤り | `class A extends B` | ("A","B")が含まれる | `find_extends`の利用 | 検証済み |
| 複数クラス | 2+クラス | 全て抽出 | このチャンクには現れない | 未検証 |
| シグナル抽出 | `signal x` | Signal種別抽出 | このチャンクには現れない | 未検証 |
| 定数抽出 | `const MAX_HEALTH` | Constant種別抽出 | このチャンクには現れない | 未検証 |
| 文字列エンコード | 非ASCII | 正しく扱う | このチャンクには現れない | 未検証 |

Rust特有の観点:
- 所有権/借用
  - `build_parser`で作成した`parser`と`counter`をテスト関数内で**可変借用**。`parse(code, file_id, &mut counter)`呼び出し時に`counter`は一時的に借用される（行番号不明）。
- ライフタイム
  - 明示的ライフタイムは不要。返却された`symbols`は関数ローカルで完結。
- unsafe境界
  - なし。
- Send/Sync/非同期
  - 非同期処理・`await`なし。共有状態保護構造なし。
- エラー設計
  - `new()`と`FileId::new()`に対し`expect`を使用し、失敗時は**panic**。テストとしては妥当だが、ライブラリ利用側では`Result`を伝播すべき。

## Design & Architecture Suggestions

- シンボル検索のヘルパー導入
  - 重複する`symbols.iter().find(...)`を関数化（例: `find_by_name`, `find_by_signature`, `find_by_kind`）し、テストの可読性・保守性を向上。
- 名前→シンボルのインデックス化
  - 大規模テスト向けに`HashMap<&str, &Symbol>`を作り、複数の検証を効率化。
- 可視性ルールの明示
  - 先頭アンダースコアをPrivateとするルールは**慣習**の可能性あり。パーサ仕様書に明記し、テストメッセージも仕様リンクに言及すると誤解が減る。
- モジュール名の定数化
  - `"<script>"`文字列をテスト内定数やパーサ側の公開定数から参照することで、**マジック文字列**のリスクを低減。
- エラーハンドリング方針
  - テスト以外のユースケース向けには`expect`ではなく`?`で伝播する例を別途用意したい。

## Testing Strategy (Unit/Integration) with Examples

- 追加ユニットテスト提案
  - 定数の抽出と`SymbolKind::Constant`（推定）の確認
  - シグナルの抽出（`signal health_changed(new_value)`）の種別
  - クラスなしスクリプトでのモジュールのみ抽出
  - 複数クラス・複数`extends`の検証
  - ドキュメントコメントの多行や空行挿入時の紐付け挙動

- 例: 空スクリプトのテスト
  ```rust
  #[test]
  fn parse_empty_script_yields_module_only() {
      let (mut parser, file_id, mut counter) = build_parser();
      let symbols = parser.parse("", file_id, &mut counter);
      // Moduleのみ、他は存在しないことを確認（推定）
      assert!(symbols.iter().any(|s| s.kind == SymbolKind::Module));
      assert!(!symbols.iter().any(|s| s.kind != SymbolKind::Module));
  }
  ```

- 例: シグナル抽出のテスト
  ```rust
  #[test]
  fn extracts_signal_symbols() {
      let code = r#"
      class Player:
          ## Emitted when health changes
          signal health_changed(new_value)
      "#;
      let (mut parser, file_id, mut counter) = build_parser();
      let symbols = parser.parse(code, file_id, &mut counter);
      assert!(symbols.iter().any(|s| s.name.as_ref() == "health_changed"));
      // 種別やドキュメントの検証（SymbolKindやdoc_comment）
  }
  ```

- プロパティベーステスト
  - 無作為な識別子名・コメント位置に対し、ドキュメント紐付け・可視性判定が一貫しているか検証。

## Refactoring Plan & Best Practices

- 検索の抽象化
  ```rust
  fn find_symbol<'a>(symbols: &'a [Symbol], pred: impl Fn(&'a Symbol) -> bool) -> &'a Symbol {
      symbols.iter().find(|s| pred(s)).expect("symbol not found")
  }
  ```
  これにより重複ロジックを削減し、失敗時メッセージ一元化。

- テーブル駆動テスト
  - 名前・種別・可視性・ドキュメントの期待値を配列にして一括検証。

- スナップショットテストの活用
  - `insta`等で`symbols`全体のダンプをスナップショット化し、回帰検出を容易に。

- マジック値の定数化
  - `MODULE_NAME: &str = "<script>";` のような定数定義で明確化。

## Observability (Logging, Metrics, Tracing)

- ログ
  - パーサ内部でのトークナイズ・AST生成段階のデバッグログを有効化できるインターフェースがあれば、テスト失敗時の原因追跡が容易。
- メトリクス
  - 抽出されたシンボル数、クラス/関数/フィールド/シグナルの各カテゴリ数を`SymbolCounter`から取得して検証可能にすると、品質担保向上。
- トレーシング
  - 複雑な解析で`tracing`クレートを用い、関数境界・イベントを発火。テスト時に`fmt`レイヤで視認。

（本チャンクでは観測機能の実装は現れない）

## Risks & Unknowns

- 不明な返却型
  - `parse`の戻り値の正確な型が不明（ベクタ等を推定）。データ契約の厳密性が担保できない。
- `find_extends`タプル第3要素
  - 第3要素の意味が不明（位置情報等の可能性）。テストでは無視しているため仕様が隠蔽される。
- 可視性ルール
  - 先頭アンダースコアでPrivateとするのは**言語仕様**か**慣習**か不明。仕様変更に脆弱。
- マジック文字列
  - モジュール名`"<script>"`を直書き。仕様変更時にテストが壊れる恐れ。
- メタ情報の不整合
  - 「functions=3」と本チャンクの2関数の乖離。計測定義か抽出方法に依存する可能性。
- パフォーマンスの未知
  - `parse`の計算量・メモリ利用は不明。大規模入力時の振る舞いはテストしていない。

（重要主張の根拠は `test_gdscript_parser_extracts_core_symbols` 関数内の記述。行番号はこのチャンクに含まれず不明）