# io/parse.rs Review

## TL;DR

- 目的: tree-sitterで解析したASTをJSON Linesとしてストリーミング出力するためのコマンド実装（execute_parse）と、ノード走査（walk_and_stream）のコア。
- 主要公開API: ParseError（詳細なエラー型）、NodeInfo（出力データ契約）、ParseOutput（出力ハンドラ）、walk_and_stream（AST走査）、execute_parse（ファイル→AST→JSONL）。
- 複雑箇所: walk_and_streamの再帰処理（max_depthとall_nodesによるフィルタリング）、言語判定とtree-sitter言語セットアップ。
- 重大リスク: write_nodeで毎行flushするため高頻度I/Oで性能劣化、非常に深いASTでの再帰スタック枯渇、Textをlossy変換しているため非UTF-8ファイルで内容が変わり位置ズレの可能性。
- Rust安全性: unsafe不使用で所有権/借用は明瞭。並行性は不使用。Resultエラー設計はthiserrorで充実。
- セキュリティ: 外部入力はファイルパスと拡張子。インジェクションの可能性は低いが、巨大ファイルや深いASTへの対策が未整備。
- パフォーマンス改善余地: BufWriter導入とバッチflush、言語セットアップ/パーサ再利用の検討。

## Overview & Purpose

このモジュールは、外部コマンド「parse」の実体として、指定ファイルをtree-sitterで解析し、ASTノードを1行1JSON（JSONL）で標準出力またはファイルへストリーム出力します。主な関心は以下のとおりです。

- 言語検出（拡張子→crate::parsing::Language）とtree-sitterの言語設定。
- ファイル読み込み（UTF-8へのlossy変換）。
- ASTの再帰走査と、識別子系ノード名の抽出。
- 出力フォーマットの固定（NodeInfo型のJSON）。
- 詳細なエラー型（ParseError）とExitCodeへのマッピング。

この機能により、外部ツールや可視化システムがASTを機械可読形式で扱えるようになります。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | ParseError | pub | 実行時エラー分類とメッセージ／ExitCode対応 | Low |
| Struct | NodeInfo | pub | 出力するASTノードのデータ契約（JSONシリアライズ） | Low |
| Struct | ParseOutput | pub | 出力先（stdout/ファイル）を抽象化しJSONLを書き出す | Low |
| Func | ParseError::exit_code | pub | エラー→ExitCodeの変換 | Low |
| Func | ParseOutput::new | pub | 出力先の初期化（ファイル作成またはstdout） | Low |
| Func | ParseOutput::write_node | pub | NodeInfoをJSONLとして1行出力＋flush | Low |
| Func | walk_and_stream | pub | AST再帰走査＋階層・親ID追跡＋フィルタリング | Med |
| Func | execute_parse | pub | ファイル→言語設定→解析→出力までの総合実行 | Med |

### Dependencies & Interactions

- 内部依存
  - execute_parse → ParseOutput::new, walk_and_stream, ParseError（すべてのエラー報告）
  - walk_and_stream → ParseOutput::write_node（出力）, NodeInfo（データ契約）
  - ParseError::exit_code → crate::io::ExitCode
- 外部依存（表）

| クレート/モジュール | 用途 |
|---------------------|------|
| thiserror::Error | カスタムエラー定義（メッセージ/ソース付与） |
| serde::Serialize | NodeInfoのJSONシリアライズ可能化 |
| serde_json | NodeInfoをJSON文字列へ変換 |
| tree_sitter | Parser/Node/LanguageError |
| 各言語クレート（rust, python, typescript, javascript, php, go, c, cpp, c_sharp, gdscript, kotlin） | 言語ごとのLanguage提供 |
| std::fs, std::io::{Write}, std::path::{Path, PathBuf} | ファイルI/O、出力、パス操作 |

- 被依存推定
  - CLIサブコマンドの実行ロジック（このモジュールを呼び出して外部にASTを出力）
  - 分析・可視化ツールの前段としてJSONLを取り込むパイプライン
  - テストや社内ツールがAST検査・メトリクス収集のために活用

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ParseError::exit_code | fn exit_code(&self) -> ExitCode | エラーを適切な終了コードに変換 | O(1) | O(1) |
| NodeInfo | struct NodeInfo { … } | ASTノードのJSON出力データ契約 | - | - |
| ParseOutput::new | fn new(output_path: Option<PathBuf>) -> Result<Self, ParseError> | 出力先（ファイル/標準出力）の初期化 | O(1)（ファイル作成はO(1)I/O） | O(1) |
| ParseOutput::write_node | fn write_node(&mut self, node: &NodeInfo) -> Result<(), ParseError> | NodeInfoを1行JSONLで出力しflush | O(n)（n=JSON長） | O(1) |
| walk_and_stream | fn walk_and_stream(node: tree_sitter::Node, code: &str, writer: &mut ParseOutput, depth: usize, parent_id: Option<usize>, node_counter: &mut usize, max_depth: Option<usize>, all_nodes: bool) -> Result<(), ParseError> | ASTを再帰で走査し、深さ/親ID付きで出力 | O(N)（N=訪問ノード数） | O(H)（H=最大再帰深さ） |
| execute_parse | fn execute_parse(file_path: &Path, output_path: Option<PathBuf>, max_depth: Option<usize>, all_nodes: bool) -> Result<(), ParseError> | ファイルの解析準備〜出力までの総合実行 | O(F + P + N)（F=読込, P=解析, N=出力） | O(F)一時文字列 |

以下、各APIの詳細です。

### ParseError::exit_code

1. 目的と責務
   - エラータイプに応じて、CLIの終了コード（ExitCode）に変換します。
2. アルゴリズム（ステップ）
   - selfのバリアントをmatchし、対応するExitCodeを返す。
3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| self | &self | 現在のエラーインスタンス |

4. 戻り値

| 型 | 説明 |
|----|------|
| ExitCode | 終了コード（NotFound/UnsupportedOperation/IoError/ParseError/GeneralError） |

5. 使用例
```rust
fn handle_error(e: &ParseError) {
    let code = e.exit_code();
    eprintln!("error: {e}");
    std::process::exit(code as i32);
}
```

6. エッジケース
- 特になし（全バリアントを網羅）。行番号根拠: ParseError::exit_code（行番号不明）

### NodeInfo（データ契約）

1. 目的と責務
   - 出力されるASTノード情報のJSON表現を定義（serde::Serialize）。
2. フィールド（公開）

| フィールド | 型 | 説明 |
|-----------|----|------|
| node | String | ノード種別（例: function_declaration） |
| start | [usize; 2] | 開始位置 [row, column]（0-based） |
| end | [usize; 2] | 終了位置 [row, column]（0-based） |
| kind_id | u16 | tree-sitterのkind ID |
| depth | usize | ASTの深さ（0=ルート） |
| id | usize | ファイル内一意のノードID |
| parent | Option<usize> | 親ノードID（ルートはNone） |
| name | Option<String> | 識別子系ノードの名前（identifier等） |

3. 使用例
```rust
let node_info = NodeInfo {
    node: "identifier".to_string(),
    start: [1, 4],
    end: [1, 8],
    kind_id: 42,
    depth: 2,
    id: 10,
    parent: Some(5),
    name: Some("foo".to_string()),
};
```

4. エッジケース
- start/endは0始まりの行・列である点に注意
- nameはidentifier系のみ設定（identifier/property_identifier/type_identifier/field_identifier）。根拠: walk_and_stream内の分岐（行番号不明）

### ParseOutput::new

1. 目的と責務
   - 出力先を初期化。パスが指定されればファイル作成、なければstdoutへ。
2. アルゴリズム
   - output_path Some → File::createでWriter取得（失敗はOutputCreateError）
   - output_path None → io::stdoutをWriterとして使用
3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| output_path | Option<PathBuf> | 出力ファイルパス（Noneならstdout） |

4. 戻り値

| 型 | 説明 |
|----|------|
| Result<Self, ParseError> | 成功でParseOutput、失敗でOutputCreateError |

5. 使用例
```rust
let out = ParseOutput::new(Some(std::path::PathBuf::from("ast.jsonl")))?;
```

6. エッジケース
- ファイル作成権限/存在しないディレクトリ → OutputCreateError
- stdout使用時は権限問題なし（ただし書き込みエラーはwrite_nodeで検出）

### ParseOutput::write_node

1. 目的と責務
   - NodeInfoをJSON化して1行出力し、即時flushする。
2. アルゴリズム
   - serde_json::to_string(node) → writeln(writer, "{json}") → writer.flush()
3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| &mut self | ParseOutput | 書き込み先 |
| node | &NodeInfo | 出力するノード |

4. 戻り値

| 型 | 説明 |
|----|------|
| Result<(), ParseError> | シリアライズ/書き込み/flushエラーを変換して返す |

5. 使用例
```rust
let mut out = ParseOutput::new(None)?;
out.write_node(&node_info)?;
```

6. エッジケース
- serde_jsonのエラー → SerializationError
- 書き込み/flush失敗 → OutputWriteError
- 高頻度flushにより性能劣化（大規模ASTで顕著）

### walk_and_stream

1. 目的と責務
   - ASTを深さ・親IDを追跡しながら再帰的に走査し、必要ノードのみJSONLで出力。
2. アルゴリズム
   - current_id = *node_counter; *node_counter += 1
   - all_nodes || node.is_named() のとき出力
     - 識別子系kindならname = utf8_text
     - NodeInfo構築してwrite_node
   - max_depthがSomeでdepth >= maxなら子走査を停止
   - node.walk()で子ノード列挙し、各childに対して再帰呼び出し
3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| node | tree_sitter::Node | 現在のノード |
| code | &str | 元コード（utf8_text抽出に使用） |
| writer | &mut ParseOutput | 出力先 |
| depth | usize | 現在深さ |
| parent_id | Option<usize> | 親ノードID |
| node_counter | &mut usize | 連番IDカウンタ |
| max_depth | Option<usize> | 走査最大深さ（Noneで制限なし） |
| all_nodes | bool | 匿名ノードも含めるか |

4. 戻り値

| 型 | 説明 |
|----|------|
| Result<(), ParseError> | write_nodeで発生したエラーなど |

5. 使用例
```rust
// 例: 事前にParserでtreeを得てから
let mut out = ParseOutput::new(None)?;
let mut counter = 0;
walk_and_stream(
    tree.root_node(),
    &code_str,
    &mut out,
    0,
    None,
    &mut counter,
    Some(3),   // 深さ3まで
    false      // 匿名ノードはスキップ
)?;
```

6. エッジケース
- max_depth=Some(0) → ルートのみ出力
- all_nodes=false → punctuation/keywordなど匿名はスキップ
- 非UTF-8からのlossy変換によりutf8_text抽出が変化する可能性
- 非常に深い木でスタックオーバーフローのリスク

### execute_parse

1. 目的と責務
   - 入力ファイルの存在確認、言語判定、ファイル読み込み、tree-sitter解析、出力初期化、AST走査までをまとめて実行。
2. アルゴリズム
   - file_path.exists()チェック → FileNotFound
   - 拡張子取得 → Language::from_extension → UnsupportedLanguage
   - fs::read → FileReadError
   - String::from_utf8_lossy → code
   - Parser::new → set_language(ts_language) → LanguageSetupError
   - parser.parse(&code, None) → ParseFailure
   - ParseOutput::new(output_path)
   - walk_and_stream(root_node, …)
3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| file_path | &Path | 入力ファイル |
| output_path | Option<PathBuf> | 出力ファイル（Noneでstdout） |
| max_depth | Option<usize> | 走査最大深さ |
| all_nodes | bool | 匿名ノードも含めるか |

4. 戻り値

| 型 | 説明 |
|----|------|
| Result<(), ParseError> | 全工程中のエラーをParseErrorで返す |

5. 使用例
```rust
execute_parse(
    std::path::Path::new("src/main.rs"),
    Some(std::path::PathBuf::from("ast.jsonl")),
    Some(5),
    false
)?;
```

6. エッジケース
- 存在しないファイル → FileNotFound
- 未対応拡張子 → UnsupportedLanguage
- 読み込み失敗 → FileReadError
- 言語セット失敗 → LanguageSetupError
- 解析失敗（None） → ParseFailure
- 出力先作成失敗 → OutputCreateError
- 走査中出力失敗 → OutputWriteError

## Walkthrough & Data Flow

- 入力: file_path, 出力: stdout or output_path, オプション: max_depth, all_nodes
- execute_parseのデータフロー
  1. file_path検査（存在しない→FileNotFound）
  2. 拡張子→Language判定（未知→UnsupportedLanguage）
  3. fs::readでバイト列読込（失敗→FileReadError）
  4. from_utf8_lossyで文字列化（非UTF-8は変換）
  5. Parser作成→set_language（失敗→LanguageSetupError）
  6. parseでTree作成（None→ParseFailure）
  7. ParseOutput初期化（file作成失敗→OutputCreateError）
  8. walk_and_streamで再帰走査（出力失敗→OutputWriteError）
- walk_and_streamのデータフロー
  - ノードID採番→出力フィルタ判定→NodeInfo構築→write→深さ制限確認→子ノード走査（再帰）

### Flowchart: execute_parseの主要分岐

```mermaid
flowchart TD
    A[Start] --> B{file exists?}
    B -- no --> E[Err(FileNotFound)]
    B -- yes --> C[ext = file_path.extension()]
    C --> D{Language::from_extension?}
    D -- no --> F[Err(UnsupportedLanguage)]
    D -- yes --> G[bytes = fs::read(file)]
    G --> H{read ok?}
    H -- no --> I[Err(FileReadError)]
    H -- yes --> J[code = from_utf8_lossy(bytes)]
    J --> K[parser = Parser::new()]
    K --> L{set_language ok?}
    L -- no --> M[Err(LanguageSetupError)]
    L -- yes --> N{parse ok?}
    N -- no --> O[Err(ParseFailure)]
    N -- yes --> P{output_path?}
    P -- Some --> Q[File::create -> ParseOutput]
    P -- None --> R[stdout -> ParseOutput]
    Q --> S[walk_and_stream]
    R --> S[walk_and_stream]
    S --> T{write ok?}
    T -- no --> U[Err(OutputWriteError)]
    T -- yes --> V[Ok(())]
```
上記の図は`execute_parse`関数の主要分岐を示す（行番号不明）。

### Flowchart: walk_and_streamの主要分岐

```mermaid
flowchart TD
    A[Enter node] --> B[current_id = node_counter; ++node_counter]
    B --> C{all_nodes || node.is_named()?}
    C -- no --> F[Skip output]
    C -- yes --> D{name = (identifier系ならutf8_text)}
    D --> E[NodeInfo構築→write_node]
    F --> G{max_depth設定ありか?}
    E --> G
    G -- yes --> H{depth >= max?}
    H -- yes --> J[Return]
    H -- no --> I[for child in node.children()]
    G -- no --> I
    I --> K[再帰呼び出し child]
    K --> L[Done]
```
上記の図は`walk_and_stream`関数の主要分岐を示す（行番号不明）。

## Complexity & Performance

- 時間計算量
  - execute_parse: O(F + P + N)
    - F: ファイル読込はO(file_size)
    - P: tree-sitter解析は概ねO(file_size)
    - N: ノード出力はO(num_nodes × 平均JSON長)
  - walk_and_stream: O(N)（ノード数に線形）
  - write_node: O(len(JSON))（文字列化＋1行書き込み＋flush）
- 空間計算量
  - execute_parse: O(file_size)（lossy文字列の保持）、ASTはtree-sitter側管理
  - walk_and_stream: O(H)（再帰深さ分のコールスタック）
- ボトルネック
  - flushを各行で行うことによりI/Oが高頻度になり、出力ファイル/パイプで大幅な性能低下が予想される。
  - 巨大ファイルの解析はCPU使用率が高く、ノード数が多いとJSONシリアライズのオーバーヘッドも増加。
- スケール限界
  - 深いASTでの再帰スタック枯渇（深さに比例）。
  - 32-bit環境ではidやdepthが極端に増加するとusizeがオーバーフローしうる（現実的には稀）。
- 実運用負荷要因
  - I/O: 毎行flushによるシステムコール頻発。
  - CPU: JSONシリアライズおよびtree-sitter解析。
  - メモリ: 入力全文の文字列化（lossy）。

## Edge Cases, Bugs, and Security

- 代表的エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 非存在ファイル | "missing.rs" | Err(FileNotFound) | execute_parseでexistsチェック | 実装済み |
| 未対応拡張子 | "file.xyz" | Err(UnsupportedLanguage) | Language::from_extensionの結果で判定 | 実装済み（詳細は他モジュール依存） |
| 読み込み失敗 | 権限なし | Err(FileReadError) | fs::readのエラーラップ | 実装済み |
| 言語設定失敗 | 内部不整合 | Err(LanguageSetupError) | set_languageのエラーラップ | 実装済み |
| 解析失敗 | 破損ファイル | Err(ParseFailure) | parseがNone | 実装済み |
| 出力ファイル作成失敗 | 書込不可dir | Err(OutputCreateError) | File::createのエラーラップ | 実装済み |
| 書き込み失敗 | ディスクフル | Err(OutputWriteError) | writeln/flushのエラーラップ | 実装済み |
| 非UTF-8入力 | バイナリ混在 | 文字化け（�）しつつ解析 | from_utf8_lossyで対応 | 実装済み/注意必要 |
| max_depth=0 | 0 | ルートのみ出力 | walk_and_streamでdepth>=maxで停止 | 実装済み |
| all_nodes=false | 記号ノード | 匿名ノードはスキップ | node.is_named()で判定 | 実装済み |
| 非常に深いAST | 深さ>数万 | スタック枯渇の可能性 | 再帰で走査 | 潜在的問題 |
| 巨大ノード数 | 数百万 | 高I/O・高CPU | 毎行flush＋JSON化 | 潜在的性能問題 |

- メモリ安全性
  - Buffer overflow: なし（Rust安全なAPIのみ使用）
  - Use-after-free: なし
  - Integer overflow: id/depthがusizeで極端な値で潜在的（現実的には稀）。kind_idはu16でtree-sitter側範囲内。
- インジェクション
  - SQL/Command: 該当なし
  - Path traversal: file_pathは呼び出し元から渡されるが、そのままfs::readを行う。意図しない場所の読込については呼び出し側の責務。必要ならばパス検証・制限を追加。
- 認証・認可
  - 該当なし（ローカルファイルI/Oのみ）
- 秘密情報
  - Hard-coded secrets: なし
  - Log leakage: なし（ロギング未実装）
- 並行性
  - Race condition / Deadlock: 該当なし（単一スレッド設計）
- Rust特有の観点
  - 所有権: writerはParseOutputが所有し、&mutで渡すことで排他的書込を保証。walk_and_streamは&mut ParseOutputを再帰的に共有（可変借用は呼び出し境界で直列化されるため安全）。根拠: ParseOutputフィールドと関数シグネチャ（行番号不明）
  - 借用: codeは&strとして不変参照、tree_sitter::Nodeはライフタイム的にTreeに紐づくが、関数内のみで使用され安全。
  - ライフタイム: 明示的ライフタイムは不要。Nodeの取得はtree.root_node()から局所的使用。
  - unsafe境界: なし（unsafe未使用）
  - Send/Sync: Box<dyn Write>は一般にSend/Syncではない可能性があるが、並行使用はしていない。並行処理はこのモジュールには存在しない。
  - await境界/キャンセル: 非async。該当なし。
- エラー設計
  - Result vs Option: 失敗はResult<_, ParseError>で伝搬。Optionはparseの戻り（None）をParseFailureに変換。
  - panic箇所: unwrap/expect不使用。すべてエラーはParseErrorにラップ。
  - エラー変換: thiserrorで#[source], #[from]を使用。serde_json::ErrorはSerializationErrorに自動変換。

## Design & Architecture Suggestions

- 出力バッファリング
  - writeln後に毎回flushするのではなく、BufWriterを用いてバッファリングし、定期的または終了時にflushすることで性能改善。
  - ParseOutputを内部的にBufWriter<Box<dyn Write>>へ変更しつつ、明示的flushメソッドを提供。
- 再帰の反復化
  - 非常に深いAST対策として、Vecスタックを用いた手続き的DFSに置換可能（メモリ使用は増えるがスタック枯渇を回避）。
- 出力フォーマット拡張
  - JSONL以外（CSV、Protobuf）の選択肢や、フィルタ条件（node種別ホワイトリスト/ブラックリスト）の導入。
- エラーメッセージの構造化
  - 現在は人間向けのメッセージ。機械可読なエラーコードフィールドの追加も有用。
- 言語セットアップの共通化
  - Language→tree_sitter::Languageへの変換を専用モジュールに切り出してテスト容易化。
- ノード名抽出の拡張
  - 識別子以外にも関数名/クラス名等の抽出に対応（各言語のクエリ/field name活用）。現状コードには未実装なので別機能として提案。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト
  - 正常系: 簡単なRustコードを一時ファイルに書き出し、execute_parseでJSONLを生成して行数>0を検証。
  - フィルタ: all_nodes=falseとtrueで出力差分（匿名ノード有無）を確認。
  - 深さ制限: max_depth=1で子ノードが出力されないことを検証。
  - エラー系: 存在しないファイル、未対応拡張子、出力先作成不可（read-only dir）で適切なParseErrorが返ること。
  - Serialization: NodeInfoのnameがidentifierで設定されることの検証。
```rust
#[test]
fn parse_rust_file_basic() -> Result<(), Box<dyn std::error::Error>> {
    use std::{fs, path::PathBuf};
    // テスト用ファイル作成
    let tmp = PathBuf::from("tmp_test.rs");
    fs::write(&tmp, "fn main() { let x = 1; }")?;

    // 出力先
    let out = PathBuf::from("tmp_ast.jsonl");
    execute_parse(&tmp, Some(out.clone()), Some(3), false)?;

    // 検証: 出力が少なくとも1行
    let content = fs::read_to_string(&out)?;
    assert!(content.lines().count() > 0);

    // 片付け
    let _ = fs::remove_file(&tmp);
    let _ = fs::remove_file(&out);
    Ok(())
}

#[test]
fn unsupported_extension_error() {
    let p = std::path::Path::new("file.xyz");
    let err = execute_parse(p, None, None, false).unwrap_err();
    // 必ずUnsupportedLanguageとは限らない（存在しないとFileNotFound先行）
    // まず存在しないのでFileNotFoundが期待される
    match err {
        ParseError::FileNotFound { .. } => {}
        _ => panic!("expected FileNotFound"),
    }
}
```
- 統合テスト
  - 複数言語（rs, py, ts等）での解析出力確認（crate::parsing::Languageの動作依存で、このチャンク外）。
  - 大規模ファイルでの時間・メモリプロファイル（flush最適化が必要）。

- 失敗注入テスト
  - 出力先に意図的にエラーを発生させ、OutputWriteErrorを検証。例: 一杯のディスク、または特殊なWrite実装（このチャンク外でモックが必要）。

## Refactoring Plan & Best Practices

- ステップ1: ParseOutputにBufWriterを導入し、write_nodeでflushをやめる。明示的flushメソッドを追加。
- ステップ2: walk_and_streamを非再帰DFSに置換可能な抽象を用意（Stackベース）。深さ制限はロジック維持。
- ステップ3: 言語マッピングを専用関数へ切出し、テスト強化。
- ステップ4: NodeInfoのname抽出を関数化し、言語ごとの識別子判定を設定可能に。
- ステップ5: execute_parseに「最大ノード出力数」のガードを追加し、過負荷を防止。
- ベストプラクティス
  - エラーに対してユーザ支援メッセージを維持しつつ、機械可読なフィールド（code, context）追加。
  - APIのドキュメント化と例の充実（現在の実装は読みやすいが、行番号ベースのテスト・デバッグ支援も有用）。

## Observability (Logging, Metrics, Tracing)

- ログ
  - INFO: 解析対象ファイル、検出言語、出力先
  - WARN/ERROR: 読み込み失敗、言語設定失敗、解析失敗、書込み失敗詳細
- メトリクス
  - カウンタ: 出力ノード数、識別子名抽出数
  - ヒストグラム: write_node時間、全体処理時間
- トレーシング
  - スパン: execute_parse開始〜終了、walk_and_stream（深さごとの区切り）
  - コンテキスト: file_path, language, max_depth, all_nodes

## Risks & Unknowns

- crate::parsing::Languageの拡張子対応詳細はこのチャンクには現れない（未知）。KotlinやC#の拡張子とマッピングの整合性は外部次第。
- 非UTF-8のlossy変換による位置・文字列の変化は解析精度への影響有り（要要件確認）。
- 非常に深いASTや巨大ファイルでの性能/スタック問題への対策は未実装。
- Writeの動作（stdout/ファイル）におけるフラッシュ方針は要検討。パイプ連結時の背圧で性能差が顕著。
- tree_sitter各言語クレートのバージョン差異・互換性はこのチャンクからは不明。
- 行番号はこのチャンクには現れないため、正確な行参照ができない点に留意。