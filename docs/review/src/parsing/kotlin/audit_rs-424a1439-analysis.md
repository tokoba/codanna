# parsing\kotlin\audit.rs Review

## TL;DR

- 目的: tree-sitter-kotlin が生成するAST上のノード種別と、社内実装の KotlinParser が実際に扱っているノード・抽出できたシンボル種別を照合し、抽出ギャップを可視化するレポートを生成する。
- 主な公開API:
  - KotlinParserAudit::audit_file, KotlinParserAudit::audit_code, KotlinParserAudit::generate_report
  - データ契約: KotlinParserAudit { grammar_nodes, implemented_nodes, extracted_symbol_kinds } と AuditError
- コアロジック: tree-sitter によるAST走査（DFS）でノード種別を収集し、自前パーサを実行して取り扱いノードと抽出シンボル種別を収集、Markdownレポート化。
- 安全性: unsafeは未使用。例外的に FileId::new(1).unwrap()（L61）がpanicを誘発し得る。外部ライブラリ tree-sitter 操作は所有権/借用が妥当。
- 重大リスク: 大規模/深いASTでの再帰（discover_nodes, L184-191）によるスタック消費。レポートの「ノード発見数」は出現回数ではなく「種類の集合」のみで、カバレッジの粒度が粗い。
- 並行性: シングルスレッド前提。Parserなどはスレッド間共有なし。キャンセルやタイムアウトの仕組みは未実装。
- テスト: 正常系の簡単なE2Eのみ（tests::test_audit_simple_kotlin, L197-235）。エラー系・巨大入力・不正構文などのケースは不足。

## Overview & Purpose

本モジュールは、Kotlinソースに対し以下を実行する監査ユーティリティである。

- tree-sitter-kotlin で得られる AST を直接走査し、出現したノード種別（kind）を収集する（discover_nodes, L184-191）。
- 社内の KotlinParser（super::KotlinParser）を同一コードに対して実行し、実装がハンドルしているノードや抽出されたシンボル種別を収集する（audit_code, L58-L79）。
- 差分を Markdown でレポート化し、実装ギャップを可視化する（generate_report, L83-L181）。

用途:
- CI/ドキュメント用カバレッジ可視化
- 新規ノード対応時の抜け漏れ検知
- フィクスチャ不備（未出現ノード）の指摘

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Enum | AuditError | pub | 監査処理全体のエラー型集約（入出力、言語設定、解析、パーサ生成） | Low |
| Struct | KotlinParserAudit | pub | 監査結果の保持（grammar_nodes, implemented_nodes, extracted_symbol_kinds）と実行API、レポート生成 | Med |
| Impl Fn | KotlinParserAudit::audit_file | pub | ファイル読み込み→audit_code 実行 | Low |
| Impl Fn | KotlinParserAudit::audit_code | pub | tree-sitter 走査→社内パーサ実行→各集合の構築 | Med |
| Impl Fn | KotlinParserAudit::generate_report | pub | Markdown レポート生成 | Low |
| Fn | discover_nodes | private | AST再帰走査でノード種別とkind_idを登録 | Low |
| Mod | tests | private(cfg(test)) | 簡単なE2E検証 | Low |

### Dependencies & Interactions

- 内部依存
  - KotlinParserAudit::audit_code → discover_nodes（L56）
  - KotlinParserAudit::audit_code → KotlinParser::new, KotlinParser::parse, KotlinParser::get_handled_nodes（L59-L73）
  - KotlinParserAudit::generate_report → format_utc_timestamp（L87）

- 外部依存

| 依存 | 用途 | 備考 |
|------|------|------|
| tree_sitter::{Parser, Node} | AST生成と走査 | 言語設定（L50-L53）、パース（L53）、ノード走査（L184-L191） |
| tree_sitter_kotlin::language() | Kotlin言語定義 | 言語セットに必須（L49） |
| thiserror::Error | エラー導出 | AuditError（L13-L26） |
| crate::parsing::parser::LanguageParser | トレイト導入 | KotlinParser::parse 呼び出しのためのスコープ導入（L7, L62） |
| super::KotlinParser | 社内Kotlinパーサ | new/parse/get_handled_nodes（L59-L73） |
| crate::types::{FileId, SymbolCounter} | パーサ入力 | file_idとシンボルカウンタ（L60-L63） |
| crate::io::format::format_utc_timestamp | タイムスタンプ | レポートヘッダ（L87） |

- 被依存推定
  - CIレポート生成コマンド、開発用検証ツール、ドキュメント生成パイプラインからの利用が想定されるが、このチャンクには現れない（呼び出し元は不明）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| AuditError | pub enum AuditError | 監査時のエラー分類 | - | - |
| KotlinParserAudit | pub struct KotlinParserAudit { grammar_nodes: HashMap<String,u16>, implemented_nodes: HashSet<String>, extracted_symbol_kinds: HashSet<String> } | 監査結果のデータコンテナ | - | O(G+I+E) |
| audit_file | pub fn audit_file(path: &str) -> Result<Self, AuditError> | ファイルから読み込み監査を実行 | O(F + T_ts + P_kp + S) | O(G+I+E) |
| audit_code | pub fn audit_code(code: &str) -> Result<Self, AuditError> | 文字列コードに対して監査を実行 | O(T_ts + P_kp + S) | O(G+I+E) |
| generate_report | pub fn generate_report(&self) -> String | Markdownレポート文字列を生成 | O(G + K) | O(R) |

凡例:
- F: ファイル読み込みコスト（バイト数）
- T_ts: tree-sitter パース・走査コスト（ASTノード数）
- P_kp: KotlinParser の解析コスト（実装依存）
- S: 抽出シンボル数
- G: grammar_nodes（発見ノード種別数）
- I: implemented_nodes（実装済みノード種別数）
- E: extracted_symbol_kinds（抽出シンボル種別数）
- K: key_nodes の数（固定配列、現状16）
- R: レポート文字列サイズ

### AuditError

1) 目的と責務
- 監査パイプラインで発生し得る失敗を分類するためのエラー型（L13-L26）。

2) バリアント
- FileRead(std::io::Error)（L15-L16）
- LanguageSetup(String)（L18-L19）
- ParseFailure（L21-L22）
- ParserCreation(String)（L24-L25）

3) 使用例
```rust
use parsing::kotlin::audit::{KotlinParserAudit, AuditError};

fn run(path: &str) -> Result<(), AuditError> {
    let audit = KotlinParserAudit::audit_file(path)?;
    println!("{}", audit.generate_report());
    Ok(())
}
```

### KotlinParserAudit

1) 目的と責務
- 監査結果を保持する不変データ。フィールドはすべて公開（L29-L36）。

2) データ契約
- grammar_nodes: ノード種別名 → kind_id（u16）。出現回数は保持しない点に注意（L31, L185）。
- implemented_nodes: KotlinParser が「対応済み」と報告したノード種別名セット（L69-L73）。
- extracted_symbol_kinds: 抽出できたシンボルの kind の Debug 文字列集合（L64-L67）。

3) 使用例
```rust
let audit = KotlinParserAudit::audit_code("class C {}")?;
assert!(audit.grammar_nodes.contains_key("class_declaration"));
println!("{}", audit.generate_report());
```

### KotlinParserAudit::audit_file

1) 目的と責務
- パスからソースを読み込み、audit_code に委譲（L40-L43）。

2) アルゴリズム
- read_to_string → audit_code

3) 引数
| 名前 | 型 | 意味 |
|------|----|------|
| path | &str | ファイルパス |

4) 戻り値
| 型 | 意味 |
|----|------|
| Result<KotlinParserAudit, AuditError> | 監査結果またはエラー |

5) 使用例
```rust
let audit = KotlinParserAudit::audit_file("sample.kt")?;
```

6) エッジケース
- 存在しない/読み込み不可のパス → AuditError::FileRead

### KotlinParserAudit::audit_code

1) 目的と責務
- 文字列コード1つに対しAST走査と社内パーサ実行を行い、3つの集合を構築（L46-L80）。

2) アルゴリズム（主ステップ）
- Parser::new → language設定（L48-L53）
- parser.parse → ルートから DFS して grammar_nodes 構築（L53-L56, L184-L191）
- KotlinParser::new → parse 実行（L59-L63）
- シンボルkindの収集（L64-L67）
- implemented_nodes の収集（L69-L73）
- Self にまとめて返す（L75-L79）

3) 引数
| 名前 | 型 | 意味 |
|------|----|------|
| code | &str | Kotlinソース文字列 |

4) 戻り値
| 型 | 意味 |
|----|------|
| Result<KotlinParserAudit, AuditError> | 監査結果またはエラー |

5) 使用例
```rust
let code = r#"package p; class C { fun f() {} }"#;
let audit = KotlinParserAudit::audit_code(code)?;
assert!(!audit.extracted_symbol_kinds.is_empty());
```

6) エッジケース
- language設定失敗 → AuditError::LanguageSetup（L50-L53）
- parser.parseがNone → AuditError::ParseFailure（L53）
- KotlinParser::new失敗 → AuditError::ParserCreation（L59）
- FileId::new(1).unwrap()のpanic可能性（L61）→ 改善余地あり

### KotlinParserAudit::generate_report

1) 目的と責務
- 監査結果をMarkdown文字列で出力（L83-L181）。

2) アルゴリズム（主ステップ）
- ヘッダ/生成日時（L86-L88）
- Summary（集合サイズ）（L90-L102）
- Coverage Table（key_nodesを横断し、implemented/gap/not found を判定）（L105-L144）
- Legend, Recommended Actions（ギャップ/サンプル不足に応じた提案）（L146-L178）

3) 引数
- なし（&self）

4) 戻り値
| 型 | 意味 |
|----|------|
| String | Markdown レポート |

5) 使用例
```rust
let report = audit.generate_report();
println!("{report}");
```

6) エッジケース
- key_nodes が grammar_nodes に存在しない場合は「⭕ not found」（L139-L144）
- gaps/missing とも空なら「All tracked nodes are currently implemented ✅」（L176-L178）

## Walkthrough & Data Flow

1) audit_file(path)
- ファイル読み込み（std::fs::read_to_string, L41）→ audit_code 委譲（L42）

2) audit_code(code)
- tree-sitter 初期化（Parser::new, language設定, L48-L53）
- 解析（parser.parse）→ ルートノード取得（L53）
- discover_nodes により DFS でノード種別収集（L55-L56, L184-L191）
  - registry.insert(kind, kind_id)（L185）
  - 子ノードを cursor でたどって再帰（L187-L191）
- KotlinParser::new で社内パーサ作成（L59）
- SymbolCounter, FileId（L60-L61）
- KotlinParser::parse 実行でシンボル列を得る（L62）
- シンボルkindのDebug表現を HashSet へ（L64-L67）
- KotlinParser::get_handled_nodes から実装済みノード名集合を作成（L69-L73）
- KotlinParserAudit 構築（L75-L79）

3) generate_report(&self)
- 要約（ノード数、実装済み数、抽出シンボル種別数）（L90-L102）
- key_nodes の各要素に対し、grammar_nodes の有無/implemented_nodes の包含でステータス振り分け（L131-L144）
- 伝説と推奨アクションを追記（L146-L178）

この処理は直線的で分岐が少なく、Mermaid図の必要基準（分岐>=4, 状態>=3, アクター>=3）を満たさないため図は省略。

対象コード範囲:
- 上記の説明は audit_code（L46-L80）と generate_report（L83-L181）、discover_nodes（L184-L191）に対応。

## Complexity & Performance

- audit_file
  - 時間: O(F) + audit_code
  - 空間: audit_code と同等

- audit_code
  - 時間:
    - tree-sitter パース: おおむね O(N)（N=ソース長 or ノード数）
    - DFS 走査: O(#ASTノード)
    - KotlinParser::parse: 実装依存（おそらく O(N)〜O(N log N)）
    - 集合構築（シンボルkind, 実装ノード名）: O(S + I)
  - 空間: HashMap/HashSet 合計 O(G + E + I)
  - ボトルネック: KotlinParser::parse と tree-sitter パース。大規模ファイルで DFS 再帰のスタック使用。

- generate_report
  - 時間: O(G) + O(K)（Kは固定配列長）
  - 空間: 文字列バッファ O(R)

スケール限界:
- 非ストリーミング/単発処理であり、大量ファイルをまとめて処理する場合に全体の並列化・キャッシュがない。
- discover_nodes の再帰深度が極端に深いASTでスタック制限にかかる可能性。

I/O/ネットワーク/DB:
- I/Oは audit_file のローカルファイル読み込みのみ。ネットワーク/DBアクセスはなし。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト観点:

- メモリ安全性
  - Buffer overflow / Use-after-free: Rust安全。unsafe未使用。tree-sitterのNode/TreeCursorはRAII・借用範囲内で使用（L187-L189）。
  - Integer overflow: kind_id は u16（L31, L185）。tree-sitter の kind_id 範囲内で安全と想定。
- インジェクション
  - SQL/Command/Path traversal: 外部コマンドやDBなし。audit_file の path は呼び出し元から渡されるが、そのまま read_to_string（L41）。サーバー環境ではパス検証が別途必要だが、このモジュール単体では該当なし。
- 認証・認可
  - なし。本モジュールはローカル文字列/ファイル解析のみ。
- 秘密情報
  - Hard-coded secrets: なし。
  - Log leakage: ログ出力なし。generate_report はコード内容ではなく統計のみ。
- 並行性
  - Race condition/Deadlock: シングルスレッドで共有可変状態なし。問題なし。

既知/潜在バグ・注意点:

- panicの可能性: FileId::new(1).unwrap()（L61）が None を返し得る設計の場合にpanic。エラーへ昇格すべき。
- 精度: grammar_nodes は「種別→id」のマップで回数を数えないため、出現頻度ベースのカバレッジや重みづけは不可。名称が “All node kinds discovered” で「集合」だが count に読める恐れ。命名や仕様明確化が望ましい。
- レポートの key_nodes は固定配列（L109-L126）。tree-sitter の実ノード名とズレがあると誤判定（⚠️/⭕）の恐れ。
- discover_nodes の再帰深度: 極端な入れ子でスタックオーバーフローのリスク（理論上）。現実的なKotlinコードで問題になる可能性は低いが、イテレーティブ化でより強靭に。
- LanguageParser トレイトの導入は parse 呼び出しのためだが、IDEの補完などで見えにくい依存。リファクタ時に削除するとコンパイルエラーになる点に注意。

エッジケース詳細:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | パースは成功し、grammar_nodesに少なくともroot相当が入る。シンボルは空。 | parser.parseの戻りがSomeならOK（L53）。 | 概ねOK |
| 不正構文 | "class {" | tree-sitterはエラーノードを含む木を返すため監査は継続。シンボルは少ない可能性。 | Noneの場合のみ ParseFailure（L53）。 | 概ねOK |
| 非対応ノードが多い | さまざまな言語機能 | Coverage Tableで⚠️ gapが増える | generate_reportで判定（L131-L144） | OK |
| 巨大ファイル | 数万行 | 時間/メモリ増大。再帰深度増大に注意。 | 再帰DFS（L184-L191） | 注意 |
| 言語設定失敗 | ランタイムに言語リンク不可 | AuditError::LanguageSetup を返す | map_errで変換（L50-L53） | OK |
| KotlinParser初期化失敗 | 依存が不完全 | AuditError::ParserCreation を返す | map_err（L59） | OK |
| FileId作成失敗 | FileId::newがNone | panic回避してエラー化すべき | unwrap使用（L61） | 要修正 |

Rust特有の観点:

- 所有権・借用
  - discover_nodes(node: Node, registry: &mut HashMap<...>) は Node を値で受け取り、内部で cursor を生成し子ノードを走査（L184-L191）。cursor の借用範囲は for ループ内に限定されており健全。
- ライフタイム
  - Node は treeへの参照を内部的に保持するが、ツリーは audit_code 内のローカル変数 tree に束縛される。走査完了まで tree は生存、問題なし（L53-L56）。
- unsafe境界
  - なし（このチャンクには現れない）。
- 並行性・非同期
  - 非同期/スレッド処理なし。Send/Sync 制約は不要。
- エラー設計
  - Result を適切に使用。thiserror でメッセージ明瞭。
  - unwrap（L61）は改善余地大。panic ではなく Result にすべき。
  - ParserCreation に String を包むのではなく元エラー型を保持できるなら型安全が向上。

## Design & Architecture Suggestions

- unwrapの排除
  - FileId::new(1).unwrap()（L61）をエラー連鎖に変更。例: FileId::new(1).ok_or_else(|| AuditError::ParserCreation("invalid file id".into()))?
- FileId の注入
  - audit_code に file_id を引数で受け取れるようにし、テストや呼び出し元で制御可能に。あるいは KotlinParser::parse の file_id 依存を緩和。
- ノード出現回数の追跡
  - grammar_nodes を HashMap<String, NodeStats> に拡張し、kind_id と出現カウントを保持。レポートの精度を上げる。
- key_nodes の外部化/設定化
  - ハードコード配列（L109-L126）を設定/引数で渡せるようにし、tree-sitter バージョン差異や運用方針の変更に追随しやすく。
- DFSのイテレーティブ化
  - 深いネストに耐えるため、Vec<Node> スタックでの非再帰実装に切替可能。
- KotlinParser との疎結合化
  - KotlinParser をトraitで抽象化し、テスト用フェイクの注入でエラー経路や境界ケースを容易に検証。
- 返却データの型安全性
  - extracted_symbol_kinds が Debug 文字列（L66）なのは脆弱。明示的な enum/新型で保持し、表示時にフォーマットする。

## Testing Strategy (Unit/Integration) with Examples

強化ポイント:
- エラー系ユニットテスト
  - LanguageSetup 失敗（モック/注入で再現）
  - ParseFailure（parser.parse が None を返すケースの強制、パーサ生成を抽象化して再現）
  - FileId::new 失敗時のハンドリング（unwrap 排除後）
- 大規模/深いASTのテスト
  - ネストを深くしたKotlinコードでスタックの安全性/性能を検証
- カバレッジ妥当性
  - key_nodes に示した各ノードが出現するフィクスチャを用意し、implemented/gap/not found が所期の値になることを検証

例: audit_code エラー経路（擬似コード、インターフェース抽象化後）

```rust
#[test]
fn audit_code_returns_language_setup_error_on_set_language_failure() {
    // 前提: ParserFactory/LanguageProvider を注入可能にし、set_languageでErrを返すフェイクを作る
    // 期待: AuditError::LanguageSetup が返る
    // 実装はこのチャンクには現れないが、設計上の方向性を示す
}
```

例: 出現回数トラッキング（仕様追加後の想定テスト）

```rust
#[test]
fn discover_nodes_counts_occurrences() {
    let code = "class A {}\nclass B {}";
    let audit = KotlinParserAudit::audit_code(code).unwrap();
    // 仕様追加で grammar_nodes を kind -> { id, count } に拡張した場合の検証
    // このチャンクには現れない
}
```

既存テストの改善提案:
- test_audit_simple_kotlin（L197-L235）にエラー時の挙動検証を追加。
- レポート文字列に Coverage Table の各行が想定通り出力されることのアサートを追加。

## Refactoring Plan & Best Practices

- API改善
  - audit_code に file_id の引数を追加、もしくは内部で安全に生成（Result化）。
  - KotlinParserAudit のフィールドは公開だが、将来の互換性を考え getter を介した読み取りに変更検討（不変のままでも可）。
- エラーハンドリング一貫性
  - ParserCreation(String) の String を具象エラー型または anyhow::Error などに統一。
- 命名/仕様明確化
  - grammar_nodes を grammar_kinds などにリネーム、または count を持つ構造に拡張。
- コンフィグ導入
  - key_nodes を外部設定/引数で差し替え可能に（generate_reportを options 受け取りに）。
- 反復可能性
  - レポートの並び順を安定化（grammar_nodes をソートして出力）。
- 非再帰DFS
  - 深いAST対策としてスタック実装への切替。

## Observability (Logging, Metrics, Tracing)

- Logging
  - auditのフェーズ開始/終了、発見種別数、ギャップ数を debug/info ログ。
  - エラー詳細は error ログで統一。
- Metrics
  - discovered_node_kinds_count, implemented_node_kinds_count, extracted_symbol_kinds_count をカウンタで記録。
  - gap_count, missing_sample_count をゲージ出力。
- Tracing
  - audit_code を span で囲み、tree-sitter parse と KotlinParser.parse の所要時間をイベントとして記録。
- 現状
  - ログ/メトリクス/トレースは未実装。このチャンクには現れない。

## Risks & Unknowns

- KotlinParser の実装詳細不明
  - get_handled_nodes の返却型や定義、parse の正確な性質とエラー挙動はこのチャンクには現れない。
- tree-sitter バージョン差異
  - kind 名称の変化で key_nodes 照合の妥当性が変動。
- 環境依存
  - tree_sitter_kotlin の言語設定失敗はビルド/リンク環境に依存（L49-L53）。
- 出力安定性
  - HashMap/HashSet の順序非決定性により、将来的なレポート差分が不安定になる可能性（現在は key_nodes 順固定で影響限定）。

## Edge Cases, Bugs, and Security

（詳細表は前掲の通り。Rust安全性/エラー/並行性に関する要点を以下に再掲）

- Rust安全性
  - unsafeなし、所有権/借用は妥当（discover_nodes の cursor ライフタイムがループに限定：L187-L189）。
- エラー設計
  - unwrap（L61）は panic を招き得るため Result へ昇格を推奨。
  - ParseFailure は parser.parse が None の稀ケースのみ。一般的な構文エラーはエラーノードを含みつつ Some を返す点に留意。
- 並行性
  - 非同期/並列処理なし。共有状態なし。Send/Sync の制約検討は不要。

## Complexity & Performance

（ダブル掲載を避けるためサマリ）

- 全体 O(N) 前後、ボトルネックは tree-sitter と KotlinParser。
- メモリ O(G+I+E)。巨大ファイル時は DFS 再帰によるスタック消費を考慮。

## Design & Architecture Suggestions

（サマリ）

- unwrap の除去、出現回数トラッキング、key_nodes の設定化、非再帰DFS、KotlinParser 抽象化、型安全なシンボルkind保持。

## Testing Strategy (Unit/Integration) with Examples

（サマリ）

- エラー系、巨大入力、key_nodes の網羅、レポート内容の厳密検証を追加。

## Refactoring Plan & Best Practices

（サマリ）

- API・エラーの一貫化、命名改善、設定注入、出力の安定化、非再帰化。

## Observability (Logging, Metrics, Tracing)

（サマリ）

- ログ/メトリクス/トレースの導入提案。現状は未実装。

## Risks & Unknowns

（サマリ）

- KotlinParser の内部・エラー仕様が不明、tree-sitter のバージョン差、環境依存の言語設定。