# exploration\abi15_exploration_common.rs Review

## TL;DR

- 目的: **ABI-15テスト**における各言語向けの共通ユーティリティ（tree-sitterのパーサ生成・解析・AST可視化）。
- 公開API: **create_parser**(L9-15), **parse_code**(L18-20), **print_node_tree**(L27-55)。
- 複雑箇所: 再帰で子ノードを巡回し可視化する**print_node_tree**（出力・文字列整形の負荷、深い再帰のリスク）。
- 重大リスク: `expect`による**panic**（L12-13, L19）、`code[node.byte_range()]`の**UTF-8境界**問題によるパニック可能性（L28）。
- 並行性: `parse_code`が`&mut Parser`を要求し**同時使用をコンパイル時に抑止**。`Parser`のSend/Syncはこのチャンクでは**不明**。
- 改善提案: **Result返却**に変更、**DEBUG_TREE**環境変数で出力制御、**再帰深さ制限**、**UTF-8境界チェック**導入。

## Overview & Purpose

このモジュールは、ABI-15に関連する言語別テストで共通的に使う最小限の**tree-sitterユーティリティ**を提供します（L1-4）。主な責務は以下の通りです。

- **Parserの生成**（言語設定を含む）
- **コードの解析**（AST `Tree` の取得）
- **ASTの可視化**（ノードと子ノードのツリー表示。開発時のデバッグ用）

ドキュメンテーションでは「DEBUG_TREE環境変数設定時のみ使用」と記載されていますが、現行コードには環境変数のチェックはありません（L22-26のコメントのみ、チェック処理は「このチャンクには現れない」）。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | create_parser | pub | 指定言語用のParser生成と言語設定 | Low |
| Function | parse_code | pub | Parserでコードを解析してTree取得 | Low |
| Function | print_node_tree | pub | ノードツリーを整形して標準出力に表示 | Med |
| Module | tests | private | ユーティリティのスモークテスト | Low |

### Dependencies & Interactions

- 内部依存
  - `parse_code`は`Parser`に依存（引数で`&mut Parser`、L18）。
  - `create_parser`は`Parser::new`と`set_language`を使用（L9-15）。
  - `print_node_tree`は`Node.walk`/`children`/`field_name_for_child`に依存し、自身を**再帰**呼び出し（L43-55）。

- 外部依存（このチャンク内に現れるもの）

| クレート/モジュール | 使用箇所 | 役割 |
|--------------------|----------|------|
| tree_sitter::{Language, Node, Parser} | L6, L9-20, L27-55 | パーサ生成・解析・ASTノード操作 |
| tree_sitter_rust::LANGUAGE | L64 | テストでRust言語の`Language`取得 |

- 被依存推定
  - ABI-15探索テストの各言語モジュールからこのユーティリティが**import**される想定（L1-4）。具体的な呼び出し元は「不明」。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| create_parser | `pub fn create_parser(language: Language) -> Parser` | 指定言語のパーサを生成し設定 | O(1) | O(1) |
| parse_code | `pub fn parse_code(parser: &mut Parser, code: &str) -> tree_sitter::Tree` | コード文字列を解析し`Tree`を返す | O(n) | O(tree) |
| print_node_tree | `pub fn print_node_tree(node: Node, code: &str, indent: usize)` | ノードと子ノードをツリー形式で標準出力へ | O(N) | O(depth) |

Nはノード数、nは入力コード長、depthはASTの深さ。

### create_parser

1) 目的と責務  
- 指定された`Language`に対して**tree-sitterのParser**を生成し、言語を設定します（L9-15）。

2) アルゴリズム（ステップ分解）  
- `Parser::new()`でパーサ生成（L10）。  
- `set_language(&language)`で言語設定し、失敗時は`expect`で**panic**（L12-13）。  
- 設定済みの`Parser`を返却（L14）。

3) 引数

| 引数名 | 型 | 説明 |
|-------|----|------|
| language | Language | 解析対象の言語定義 |

4) 戻り値

| 型 | 説明 |
|----|------|
| Parser | 言語設定済みのパーサ |

5) 使用例
```rust
use tree_sitter::Language;

let language: Language = tree_sitter_rust::LANGUAGE.into();
let mut parser = create_parser(language);
```

6) エッジケース
- 言語設定に失敗すると`expect`により**panic**（L12-13）。  
- `Language`が不適切（不一致や非対応）な場合の動作は**不明**（tree-sitter側の仕様次第）。

### parse_code

1) 目的と責務  
- 渡された`&mut Parser`と`&str`コードから**解析結果Tree**を生成します（L18-20）。

2) アルゴリズム（ステップ分解）  
- `parser.parse(code, None)`で解析（L19）。  
- 失敗（`None`）時は`expect`で**panic**（L19）。  
- 成功時、`Tree`を返す。

3) 引数

| 引数名 | 型 | 説明 |
|-------|----|------|
| parser | &mut Parser | 言語設定済みのパーサ |
| code | &str | 解析対象のソースコード |

4) 戻り値

| 型 | 説明 |
|----|------|
| tree_sitter::Tree | 解析済みのAST |

5) 使用例
```rust
let language = tree_sitter_rust::LANGUAGE.into();
let mut parser = create_parser(language);
let tree = parse_code(&mut parser, "fn main() {}");
assert_eq!(tree.root_node().kind(), "source_file");
```

6) エッジケース
- 解析不能（キャンセルや内部エラー）で`None`になり得るが、現実装は**panic**（L19）。  
- 極端に大きいコードでは解析時間・メモリ増加。  
- インクリメンタルパース（第2引数に前回Tree）は使用していない（L19: `None`固定）。

### print_node_tree

1) 目的と責務  
- `Node`と対応する`code`のテキストを抽出し、**階層的に可視化**（標準出力）します（L27-55）。*開発時デバッグ用途*。

2) アルゴリズム（ステップ分解）  
- `node.byte_range()`でテキスト範囲取得し、`code[...]`で部分文字列を抽出（L28）。  
- 長さ60超は先頭57文字＋`...`へ短縮、改行は空白へ置換（L29-33）。  
- `println!`で`[kind] 'text'`の行をインデント付き表示（L35-41）。  
- `node.walk()`から子を列挙し、可能ならフィールド名を表示（L43-52）。  
- 各子ノードに対して**再帰的**に同処理（L53-54）。

3) 引数

| 引数名 | 型 | 説明 |
|-------|----|------|
| node | Node | 表示対象のASTノード |
| code | &str | 元ソースコード（テキスト抽出に使用） |
| indent | usize | インデント用の空白幅 |

4) 戻り値

| 型 | 説明 |
|----|------|
| () | 標準出力に副作用出力 |

5) 使用例
```rust
let language = tree_sitter_rust::LANGUAGE.into();
let mut parser = create_parser(language);
let tree = parse_code(&mut parser, "fn main() { let x = 1; }");
let root = tree.root_node();
print_node_tree(root, "fn main() { let x = 1; }", 0);
```

6) エッジケース
- `code[node.byte_range()]`が**UTF-8の文字境界**でない場合は**panic**（L28）。  
- 非常に深いASTで**再帰が深くなりスタックオーバーフロー**の可能性（L53-54）。  
- 巨大コードで出力量が膨大、I/Oボトルネック。  
- `field_name_for_child`は`None`を返す可能性あり、その場合はフィールド行の出力をスキップ（L45-52）。

## Walkthrough & Data Flow

- 一般的な使用フロー
  1. `create_parser(language)`でパーサ生成・言語設定（L9-15）。
  2. `parse_code(&mut parser, code)`でコードを解析し`Tree`取得（L18-20）。
  3. 必要に応じて`tree.root_node()`などから`Node`を取り出し、`print_node_tree(node, code, indent)`で**ツリー可視化**（L27-55）。

- `print_node_tree`のデータフロー詳細
  - ノード種別: `node.kind()`を取得して見出し表示（L38-39）。
  - テキスト抽出: `node.byte_range()`→`code[range]`→短縮・改行置換（L28-33）。ここで**UTF-8境界の検証がない**ため、非ASCIIコードで例外の可能性。  
  - 子巡回: `node.walk()`→`children(...).enumerate()`で順走査（L43-45）。フィールド名がある場合のみ追加行出力（L45-52）。  
  - 再帰: `print_node_tree(child, ...)`で深さを増やしながら辿る（L53-54）。

## Complexity & Performance

- create_parser: 時間O(1)、空間O(1)。`set_language`は定数時間相当。
- parse_code: 時間O(n)（入力長に比例）。空間は`Tree`サイズに依存（O(tree)）。I/Oなし。
- print_node_tree: 時間O(N)（ノード数）。ただし各ノード毎の文字列抽出・置換・整形で**追加コスト**がかかるため、実効的にはO(N + total_text_processed)。空間は**再帰スタック**O(depth)。標準出力へのI/Oがボトルネックになり得る。

スケール限界:
- 大規模コードでは解析時間・`Tree`のメモリが増加。
- 大きいASTの全出力は**非常に遅い**（標準出力I/O、文字列整形）。
- 再帰深さが極端に増えると**スタック枯渇**の危険。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| UTF-8境界でないスライス | `"fn main() { let é = 1; }"` | パニックせず安全に表示（代替処理やlossy変換） | `code[node.byte_range()]`（L28） | 改善要 |
| set_language失敗 | 不正な`Language` | `Err`を返す | `.expect("Failed to set language")`（L12-13） | 改善要 |
| parse失敗（None） | 解析不能・キャンセル | `Err`を返す | `.expect("Failed to parse code")`（L19） | 改善要 |
| 非常に深いAST | 極端なネスト | 安全に打ち切り/非再帰化 | 再帰継続（L53-54） | 改善要 |
| 巨大出力 | 数万ノード | 出力制限・サマリ化・ページング | 全ノードを表示 | 改善要 |
| フィールド名無し | 多くの言語要素 | 正常表示（フィールド無し行スキップ） | `if let Some(field_name)`（L45-52） | OK |
| 大きなindent指定 | indent=1000 | 過剰空白を制限 | インデント無制限（L35-41） | 改善要 |

セキュリティチェックリスト:
- メモリ安全性
  - unsafe未使用（このチャンクでは**unsafeなし**）。  
  - ただし`&str`のスライスが**不正境界**でパニック（L28）。Buffer overflow/Use-after-free はなし。
  - 整数オーバーフロー: `i as u32`（L45）は通常安全だが、子数が`u32::MAX`超なら不正。実際には現実的ではない。
- インジェクション
  - SQL/Command/Path: 該当なし。
- 認証・認可
  - 該当なし。
- 秘密情報
  - 解析対象コードに秘密が含まれる場合、**print_node_tree**が標準出力に漏洩する可能性（L35-41, L47-51）。*デバッグ限定用途の明示とゲートが必要*。
- 並行性
  - `parse_code`が`&mut Parser`を要求し、**同時実行のデータ競合**は型システムで防止。`Parser`の`Send/Sync`はこのチャンクでは**不明**。共有して並行使用は避けるのが安全。

Rust特有の観点:
- 所有権
  - `create_parser`は`Parser`を新規生成して**所有権を返却**（L9-15）。
  - `parse_code`は`&mut Parser`を借用し**可変借用の期間**は関数スコープ内（L18-20）。
- 借用
  - `print_node_tree`は`code: &str`を不変借用（L27）。**再帰**でも同じ借用を共有。
- ライフタイム
  - 明示的ライフタイムパラメータは不要。`Node`・`Tree`はクレート側で管理。
- unsafe境界
  - なし。
- 並行性・非同期
  - `Send/Sync`: **不明**（このチャンクには現れない）。  
  - 非同期/await/キャンセル: 使用なし。
- エラー設計
  - `expect`による**panic**が2箇所（L12-13, L19）。`Result`に置換を推奨。

## Design & Architecture Suggestions

- エラーを返すAPIへ
  - `create_parser` → `Result<Parser, E>`（`set_language`の失敗を返す）。  
  - `parse_code` → `Result<Tree, E>`（`parse`の`None`をエラー化）。
- **DEBUG_TREE**ゲートの実装
  - `std::env::var("DEBUG_TREE")`をチェックし、未設定時は出力をスキップ。
- UTF-8安全なテキスト抽出
  - `code.get(node.byte_range())`で**境界検証**し、`None`なら`String::from_utf8_lossy(&code.as_bytes()[range])`などの**lossy**表示にフォールバック。
- 再帰深さ・出力量の制限
  - 最大深さ/最大文字数/最大ノード数の**設定可能化**。オプションパラメータや`Printer`構造体で設定をまとめる。
- 非再帰化（任意）
  - 深いツリーに対する**明示的スタック（Vec）**による走査でスタックオーバーフロー回避。
- ログインフラ
  - `println!`から`log`クレート（`debug!`）へ切替し、出力レベル制御。
- フォーマットの改善
  - 長文の短縮・フィールド名表示の一貫性、インデント制限、色付け（任意）。*テストユーティリティとしての可読性向上*。

## Testing Strategy (Unit/Integration) with Examples

テストは現状スモークのみ（L57-68）。以下の追加テストを推奨。

- 正常系
  - Rustコードの解析・ルートノード種別検証（既存）。
  - 複数言語（利用側の言語ごと）。このチャンクでは詳細**不明**。
- エラー系（Result化後）
  - `set_language`失敗を検出。
  - `parse`の`None`（キャンセル/異常）を検出。
- UTF-8境界
  - 非ASCIIを含むコードを可視化して**panicしない**こと。
```rust
#[test]
fn print_tree_handles_non_ascii() {
    let language = tree_sitter_rust::LANGUAGE.into();
    let mut parser = create_parser(language);
    let code = "fn main() { let café = \"naïve\"; }";
    let tree = parse_code(&mut parser, code);
    let root = tree.root_node();
    // 未改善版はpanic可能。改善後は安全に表示。
    print_node_tree(root, code, 0);
}
```
- 深い再帰
  - 深いネストで**打ち切り**や**非再帰**の挙動確認。
```rust
#[test]
fn print_tree_limits_depth() {
    let language = tree_sitter_rust::LANGUAGE.into();
    let mut parser = create_parser(language);
    let code = "fn main() {".to_string() + &"{".repeat(5000) + &"}".repeat(5000) + "}";
    let tree = parse_code(&mut parser, &code);
    let root = tree.root_node();
    // 改善後: 最大深さを設定し、過剰な再帰を避ける
    print_node_tree(root, &code, 0);
}
```
- 出力ゲート
  - `DEBUG_TREE`環境変数の有無で出力の有無を検証。*環境依存テストは`serial`にするなど工夫*。
- パフォーマンス
  - 大規模コードでの実行時間・出力量計測。*CIでは短時間のサンプルで近似評価*。

## Refactoring Plan & Best Practices

1. エラー処理の非パニック化
   - `create_parser`/`parse_code`を`Result`返却に変更し、テスト更新。
2. デバッグ出力の制御
   - `print_node_tree`に`enabled: bool`や環境変数チェックを追加。
3. テキスト抽出の安全化
   - `code.get(range)`→`Option<&str>`、`None`時は`from_utf8_lossy`を使用。
4. 再帰の制御
   - 最大深さ・最大ノード数の設定を導入。必要なら**反復走査**へ。
5. 観測性の改善
   - `log`クレートへの移行、ノード訪問数・最大深さのメトリクス出力。
6. ドキュメント整備
   - 使い方、制限事項（UTF-8境界、出力量、深さ）を明記。

## Observability (Logging, Metrics, Tracing)

- ロギング
  - `println!`を`log::debug!`に置換し、**ログレベル**で出力制御。  
  - `DEBUG_TREE`環境変数と連動。
- メトリクス
  - 訪問ノード数、最大深さ、平均子数、抽出文字数などをカウントし、**開発時の性能把握**に役立てる。
- トレーシング
  - `tracing`クレートで`span`を貼り、ノード種別ごとの処理時間を追跡するのも有効（*任意*）。

## Risks & Unknowns

- `tree_sitter::Parser`の**Send/Sync**特性はこのチャンクでは「不明」。並行使用の安全性は未確認。
- `parse`が`None`を返す条件（キャンセル・内部エラーなど）の詳細は**不明**（クレート仕様依存）。
- `Node.byte_range`が常に**UTF-8文字境界**に一致する保証は**不明**。現実装では**panic**の可能性がある（L28）。
- `DEBUG_TREE`環境変数に関する**実装上のゲート**は存在しない（コメントのみ、L22-26）。出力制御は未実装。
- 大規模ASTに対する**出力戦略**（サマリ化・制限）の運用方針は**不明**。