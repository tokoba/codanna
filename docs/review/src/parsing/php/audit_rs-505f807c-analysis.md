## parsing/php/audit.rs Review

## TL;DR

- 目的: PHP用ASTパーサが「実際に処理しているノード」と「文法上存在するノード」のギャップを可視化し、シンボル抽出の抜け漏れを発見するための監査モジュール。
- 主な公開API: PhpParserAudit::audit_file, PhpParserAudit::audit_code, PhpParserAudit::generate_report（いずれもResultやStringを返す純粋関数）。
- コアロジック: tree-sitterでASTノード種別を列挙→自前パーサでシンボル抽出→扱ったノード一覧を突き合わせ→カバレッジレポート生成。
- 複雑箇所: 二重パース（tree-sitterと自前パーサ）、シンボル種別の収集、ノード種別IDの安定性（tree-sitter依存）。
- 重大リスク: FileIdが固定値(1)でハードコード、SymbolKindのDebug表現に依存したテストの脆さ、非UTF-8ファイルの読込失敗、空入力時の挙動がtree-sitter依存で「ParseFailure」になりうる。
- 安全性: unsafe未使用、メモリ管理は安全。I/Oはread_to_stringのみで最小限。並行性の懸念は低い（関数スコープのローカルインスタンスのみ）。
- 推奨改善: 事前構築したtree-sitterのTreeを自前パーサに渡す設計（可能なら）で二重パース削減、FileIdの受け渡し、シンボル種別の安定文字列化、カバレッジ率などの指標をレポートへ追加。

## Overview & Purpose

本モジュールは、PHPコードに対して以下を行い、パーサのカバレッジを可視化します。

1) tree-sitterを用いて当該ファイル内に出現したASTノード種別を列挙  
2) 自作のPhpParserでシンボル抽出を実施  
3) 自作パーサ側で「処理済み」と動的にトラッキングされたノード集合、抽出済みシンボル種別集合を取得  
4) 上記を比較したカバレッジレポート文字列を生成

これにより、文法上存在するが未対応のノード（ギャップ）や、サンプルファイルに出現していないノード（テストデータ不足）を特定できます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | AuditError | pub | 監査処理で発生しうるエラー種別（I/O, 言語設定, パース失敗, パーサ生成失敗） | Low |
| Struct | PhpParserAudit | pub | 監査結果の格納（grammar_nodes, implemented_nodes, extracted_symbol_kinds） | Low |
| Impl fn | PhpParserAudit::audit_file | pub | ファイルから文字列を読み取り、audit_codeを呼び出す | Low |
| Impl fn | PhpParserAudit::audit_code | pub | コア監査ロジック：tree-sitter列挙＋自前パーサ実行＋集合を作成 | Med |
| Impl fn | PhpParserAudit::generate_report | pub | カバレッジレポートの文字列生成（要約、表、推奨事項） | Low |
| fn | discover_nodes | private | AST全体を走査し、出現ノード種別→IDをHashMapへ登録 | Low |
| mod | tests | private | 代表的な正例のユニットテスト | Low |

コード引用（短関数は全体、長関数は抜粋）:

```rust
// PhpParserAudit::audit_file（短いので全体）
pub fn audit_file(file_path: &str) -> Result<Self, AuditError> {
    let code = std::fs::read_to_string(file_path)?;
    Self::audit_code(&code)
}
```

```rust
// discover_nodes（短いので全体）
fn discover_nodes(node: Node, registry: &mut HashMap<String, u16>) {
    registry.insert(node.kind().to_string(), node.kind_id());

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        discover_nodes(child, registry);
    }
}
```

```rust
// PhpParserAudit::audit_code（重要部分のみ抜粋）
pub fn audit_code(code: &str) -> Result<Self, AuditError> {
    // tree-sitterでAST構築
    let mut parser = Parser::new();
    let language = tree_sitter_php::LANGUAGE_PHP.into();
    parser.set_language(&language)
        .map_err(|e| AuditError::LanguageSetup(e.to_string()))?;
    let tree = parser.parse(code, None).ok_or(AuditError::ParseFailure)?;
    let mut grammar_nodes = HashMap::new();
    discover_nodes(tree.root_node(), &mut grammar_nodes);

    // 自前パーサでシンボル抽出
    let mut php_parser = PhpParser::new()
        .map_err(|e| AuditError::ParserCreation(e.to_string()))?;
    let file_id = FileId(1);
    let mut symbol_counter = crate::types::SymbolCounter::new();
    let symbols = php_parser.parse(code, file_id, &mut symbol_counter);

    // 収集
    let mut extracted_symbol_kinds = HashSet::new();
    for symbol in &symbols {
        extracted_symbol_kinds.insert(format!("{:?}", symbol.kind));
    }
    let implemented_nodes: HashSet<String> = php_parser
        .get_handled_nodes()
        .iter()
        .map(|handled_node| handled_node.name.clone())
        .collect();

    Ok(Self { grammar_nodes, implemented_nodes, extracted_symbol_kinds })
}
```

### Dependencies & Interactions

- 内部依存
  - PhpParserAudit::audit_file → PhpParserAudit::audit_code を呼ぶ
  - PhpParserAudit::audit_code → discover_nodes を呼ぶ
  - PhpParserAudit::generate_report は自身のフィールドを整形して出力
- 外部依存

| 依存先 | 用途 | 備考 |
|--------|------|------|
| tree_sitter::{Parser, Node} | AST構築・走査 | 言語設定・パース |
| tree_sitter_php::LANGUAGE_PHP | PHP言語定義 | 言語セットアップ |
| super::PhpParser | 自前パーサ | シンボル抽出、対応ノード取得 |
| crate::types::{FileId, SymbolCounter} | パース補助 | FileIdは現状固定1 |
| crate::io::format::format_utc_timestamp | レポート生成時のタイムスタンプ | 実装詳細は不明 |
| thiserror::Error | エラー定義の派生 | 柔軟なエラー表現 |

- 被依存推定
  - CLI/開発者向けツールからの呼び出し、CIでのカバレッジ監視。不明（このチャンクには現れない）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| PhpParserAudit::audit_file | fn audit_file(file_path: &str) -> Result<PhpParserAudit, AuditError> | ファイルを読み込みaudit_codeを実行 | O(n) 読込+パース | O(k) |
| PhpParserAudit::audit_code | fn audit_code(code: &str) -> Result<PhpParserAudit, AuditError> | tree-sitter列挙＋自前パーサ実行＋集合作成 | O(n)×2 | O(k) |
| PhpParserAudit::generate_report | fn generate_report(&self) -> String | カバレッジレポート文字列を生成 | O(m) | O(m) |

注:
- n: 入力コード長
- k: 出現ノード種別数（ユニーク）
- m: レポート長（key_nodesの固定長＋若干の文字列）

### 各APIの詳細

1) PhpParserAudit::audit_file
- 目的と責務
  - 指定パスのPHPファイルをUTF-8として読み込み、audit_codeを委譲して監査結果を得る。
- アルゴリズム
  1. read_to_stringでファイルを読み込む
  2. audit_code(&code) を呼ぶ
- 引数

| 名前 | 型 | 意味 |
|------|----|------|
| file_path | &str | 読み込むPHPファイルのパス |

- 戻り値

| 型 | 説明 |
|----|------|
| Result<PhpParserAudit, AuditError> | 監査結果またはエラー |

- 使用例

```rust
let audit = PhpParserAudit::audit_file("examples/sample.php")?;
println!("{}", audit.generate_report());
```

- エッジケース
  - 非UTF-8ファイル: FileReadエラー
  - 存在しないパス: FileReadエラー

2) PhpParserAudit::audit_code
- 目的と責務
  - 文字列のPHPコードに対して、ASTノード種別の列挙と自前パーサの処理済みノード・抽出シンボル種別の収集を行い、監査結果を返す。
- アルゴリズム
  1. tree-sitter Parserを生成しPHP言語をセット
  2. parse(code)し、rootから全ノードを再帰走査してkind→kind_idを収集
  3. PhpParser::new()でパーサを用意し、parseでシンボル抽出
  4. 抽出シンボルのkind（Debug表示）を集合に格納
  5. get_handled_nodes()で処理済みノード名集合を得る
  6. 収集した3集合を保持したPhpParserAuditを返す
- 引数

| 名前 | 型 | 意味 |
|------|----|------|
| code | &str | 監査対象のPHPコード |

- 戻り値

| 型 | 説明 |
|----|------|
| Result<PhpParserAudit, AuditError> | 監査結果またはエラー |

- 使用例

```rust
let code = r#"<?php class A { function x() {} }"#;
let audit = PhpParserAudit::audit_code(code)?;
assert!(audit.implemented_nodes.contains("class_declaration"));
```

- エッジケース
  - 空文字列: tree-sitterの仕様依存。NoneならParseFailure、Someなら空ASTで成功（不明、要確認）。
  - 文法エラーのあるコード: tree-sitterはエラーノードを含むTreeを返すため通常成功。

3) PhpParserAudit::generate_report
- 目的と責務
  - 監査結果（集合）を整形し、概要・カバレッジ表・凡例・推奨アクションを含むMarkdown文字列を生成。
- アルゴリズム
  1. タイムスタンプ出力
  2. 要約（集合のサイズ）
  3. 重要ノード（key_nodes）のカバレッジ表を構築（✅/⚠️/❌）
  4. 推奨アクション（ギャップ・不足例の列挙）
- 引数

| 名前 | 型 | 意味 |
|------|----|------|
| self | &PhpParserAudit | 監査結果 |

- 戻り値

| 型 | 説明 |
|----|------|
| String | レポートMarkdown |

- 使用例

```rust
let report = audit.generate_report();
println!("{report}");
```

- エッジケース
  - 集合が空でも安全に動作（表は❌が増えるだけ）

### Data Contracts

- PhpParserAudit
  - grammar_nodes: HashMap<String, u16>
    - キー: tree-sitterのNode.kind()（例: "class_declaration"）
    - 値: Node.kind_id()（言語定義に依存、バージョン間で安定とは限らない）
  - implemented_nodes: HashSet<String>
    - 自前パーサが「処理済み」と報告するノード名（PhpParser::get_handled_nodes由来）
  - extracted_symbol_kinds: HashSet<String>
    - 自前パーサが抽出したシンボルの種別（Debug表現の文字列）
- AuditError
  - FileRead(std::io::Error)
  - LanguageSetup(String)
  - ParseFailure
  - ParserCreation(String)

根拠（関数名:行番号）: 行番号はこのチャンクでは不明。定義は当該ファイル内のenum/struct/implブロック参照（行番号:不明）。

## Walkthrough & Data Flow

以下は audit_code の主要フローです。

```mermaid
sequenceDiagram
    actor Caller
    participant Audit as PhpParserAudit
    participant TS as tree-sitter::Parser
    participant PHP as PhpParser
    participant SC as SymbolCounter

    Caller->>Audit: audit_code(code)
    activate Audit
    Audit->>TS: Parser::new; set_language(LANGUAGE_PHP)
    TS-->>Audit: Tree
    Audit->>Audit: discover_nodes(root_node)
    Audit->>PHP: PhpParser::new()
    PHP-->>Audit: Ok(parser)
    Audit->>SC: SymbolCounter::new()
    Audit->>PHP: parse(code, FileId(1), &mut SC)
    PHP-->>Audit: Vec<Symbol>
    Audit->>Audit: collect extracted_symbol_kinds
    Audit->>PHP: get_handled_nodes()
    PHP-->>Audit: Vec<HandledNode>
    Audit-->>Caller: PhpParserAudit { grammar_nodes, implemented_nodes, extracted_symbol_kinds }
    deactivate Audit
```

上記の図は`PhpParserAudit::audit_code`関数（行番号:不明）の主要フローを示す。

## Complexity & Performance

- 時間計算量
  - audit_file: O(n)（読み込み）＋O(n)（パース）×2 ≒ O(n)
  - audit_code: O(n)（tree-sitterパース＋走査）＋O(n)（自前パーサ）＝ O(n)
  - generate_report: O(m)（固定長のkey_nodes中心で実質一定に近い）
- 空間計算量
  - grammar_nodes: O(k)（ユニークなノード種別のみ）
  - implemented_nodes / extracted_symbol_kinds: O(k')
- ボトルネック
  - 二重パース（tree-sitterと自前パーサ）。入力が極端に大きい場合、CPU時間が2倍に近づく。
- スケール限界
  - discover_nodesは再帰だが、ASTの実用的な深さでは問題になりにくい。極端なネストでスタックが深くなる可能性は低だが存在。
- I/O要因
  - audit_fileのread_to_stringはファイルサイズに線形。非UTF-8で失敗する。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト観点とエッジケース一覧。

- メモリ安全性
  - unsafe未使用。tree-sitterのNodeは関数スコープでのみ使用し、所有権/借用も標準的で安全。
- インジェクション
  - コマンド/SQL/パス操作なし。audit_fileのfile_pathはそのまま読み込むため、サーバー側で外部入力を渡す設計の場合はパス検証が必要（利用側の責務）。
- 認証・認可
  - 対象外（ローカル関数）。
- 秘密情報
  - ハードコーディングなし。ログ/レポートに機密を出力しないが、入力コード内容次第ではレポートにノード名が出る程度。
- 並行性
  - 関数ローカルにインスタンスを生成し共有状態なし。データ競合やデッドロックはなし。

エッジケース詳細:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | 監査可能 or 明確なエラー | tree-sitterのparseがNoneならParseFailure。空でもTreeを返すかは不明 | 要確認 |
| 非UTF-8ファイル | バイナリ | エラー（理由が分かる） | read_to_stringでFileRead | OK |
| 文法エラーを含むPHP | "<?php class {" | 監査継続（エラーノードを許容） | tree-sitterはTree生成、自前パーサの挙動は不明だが本コードはErrにしない | 概ねOK |
| 言語設定失敗 | 内部的障害 | 明確なエラー | LanguageSetup(String) | OK |
| 自前パーサ生成失敗 | 内部的障害 | 明確なエラー | ParserCreation(String) | OK |
| 巨大ファイル | 数十MB | 完了、時間は線形増 | O(n)×2で遅くなる | 要検証 |
| FileId固定 | 任意 | ファイル識別に応じた動作 | FileId(1)固定で常に1 | 要改善 |
| SymbolKind文字列 | "Class"など | 安定した比較 | Debug表現をformat!("{:?}")で使用 | 脆い |
| ノードIDの安定性 | kind_id | 情報の参考 | tree-sitter更新で変化の可能性 | 注意 |
| 未使用import | NodeTracker | なし | 使われていない | 警告 |

根拠（関数名:行番号）: 行番号不明。各分岐はaudit_code内のエラーハンドリング（LanguageSetup, ParseFailure, ParserCreation）にて確認可能（行番号:不明）。

## Design & Architecture Suggestions

- 二重パース削減（パフォーマンス最適化）
  - 可能であれば、audit_codeで構築したtree-sitterのTreeを自前パーサに渡せるAPIを設計し、パースを一度に。
- FileIdの扱い
  - audit_codeにFileId参数を追加、audit_fileではパスから安定IDを割り当てるなど、固定値1の排除。
- シンボル種別の安定表現
  - Debug表現依存を排し、SymbolKindに安定文字列化API（to_str()）やDisplay実装を用意して利用。
- レポート指標の強化
  - カバレッジ率（実装済み∩出現）/ 出現、未実装ノード一覧（key_nodes以外も）を追記。
- エラー型の充実
  - LanguageSetupやParserCreationに元エラー型をラップし保持（Stringではなく具体型やsource()連鎖）。
- discover_nodesの非再帰化（任意）
  - 深いネストへの備えに明示スタックで走査。ただし現状でも実用上問題は少。
- 非UTF-8対応（任意）
  - audit_fileに“lossy読み込み”モードやエンコーディング検出オプションを追加。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト
  - 成功系（既存）に加え、エラー系・境界条件を追加。
  - SymbolKind表現の安定性を担保（Debug文字列に依存しない）。

```rust
#[test]
fn test_audit_empty_code() {
    let code = "";
    // 期待動作は環境依存。少なくともパニックしないことを保証。
    let res = PhpParserAudit::audit_code(code);
    assert!(res.is_ok() || matches!(res, Err(AuditError::ParseFailure)));
}

#[test]
fn test_audit_file_not_found() {
    let res = PhpParserAudit::audit_file("no_such_file.php");
    assert!(matches!(res, Err(AuditError::FileRead(_))));
}

#[test]
fn test_generate_report_contains_sections() {
    let code = "<?php function f() {}";
    let audit = PhpParserAudit::audit_code(code).unwrap();
    let report = audit.generate_report();
    assert!(report.contains("# PHP Parser Coverage Report"));
    assert!(report.contains("## Summary"));
    assert!(report.contains("## Coverage Table"));
}
```

- 統合テスト
  - 実在する複数のPHPファイル（namespace, class, trait, enum, attributesなど）に対し監査し、カバレッジが期待値以上かを検証。
- 回帰テスト
  - tree-sitterのバージョン更新時に、kind_idの変化に耐える（IDの一致は検証対象にしない）。
- 負荷テスト
  - 大規模ファイルでの処理時間が許容範囲内であることを測定。
- 例外経路テスト
  - LanguageSetup/ParserCreationのエラー注入（モック化やDIが必要。現状は不明）。

## Refactoring Plan & Best Practices

1) audit_codeのインターフェース拡張
- Option<FileId>や&Pathを受け取るAPI追加。audit_fileはこれを利用。

2) シンボル種別の安定化
- SymbolKindにDisplay実装を追加しreportでもDisplayを使用。テストもこれに追随。

3) パース回数の削減
- PhpParser::parseに既存Treeを渡せる新APIを検討。受け入れが難しければ、少なくとも二重パースのコストを明記。

4) レポートの拡張
- 実装済み比率、未実装ノード一覧（key_nodes外も）、抽出シンボル種別一覧（ソート）を追加。

5) エラー透過性
- LanguageSetup/ParserCreationのStringではなく、元エラーを保存（thiserrorの#[source]）してデバッグ容易化。

6) コードクリーンアップ
- 未使用import（NodeTracker）の削除。lint整備。

## Observability (Logging, Metrics, Tracing)

- ログ
  - 監査開始/終了、パース時間、抽出シンボル数などをdebug/infoで記録。
- メトリクス
  - 監査時間（ms）、ユニークノード数、実装ノード数、カバレッジ率。
- トレーシング
  - audit_code全体にspanを張り、tree-sitterパース、自前パースの子spanを付与。

例（tracingクレート想定、参考実装）:

```rust
use tracing::{info, instrument};

#[instrument(level = "info", skip_all, fields(code_len = code.len()))]
pub fn audit_code(code: &str) -> Result<Self, AuditError> {
    let start = std::time::Instant::now();
    // ... 現行処理 ...
    let elapsed = start.elapsed();
    info!(?elapsed, grammar_nodes = grammar_nodes.len(),
         implemented_nodes = implemented_nodes.len(),
         extracted_kinds = extracted_symbol_kinds.len(),
         "php audit completed");
    Ok(Self { grammar_nodes, implemented_nodes, extracted_symbol_kinds })
}
```

（上記はこのチャンクの関数に対応、行番号:不明）

## Risks & Unknowns

- tree-sitterの仕様依存
  - 空入力でTreeが返る保証、kind_idの安定性はバージョンに依存。
- 自前パーサの実装詳細
  - PhpParser::parse/get_handled_nodes/SymbolCounterの仕様はこのチャンクには現れない。エラー時の振る舞い、抽出シンボル種別の安定表現は不明。
- FileId固定の影響
  - 現状の使用範囲では無害だが、将来的にIDを前提とした処理が増えると衝突の温床に。
- テストの脆さ
  - "Class"/"Method" というDebug表現依存は将来的なリファクタリングに弱い。
- 非UTF-8の現場対応
  - 国際化対応や既存レガシー資産で問題化する可能性。オプション化の議論が必要。