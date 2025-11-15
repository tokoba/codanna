# tables.rs Review

## TL;DR

- 目的: CLI/ターミナル向けの整形済みテーブル（UTF-8罫線・角丸・強調属性・色）を生成するユーティリティを提供
- 主要公開API: **TableBuilder**（ビルダー）、**create_benchmark_table**、**create_summary_table**
- コアロジック: comfy_tableのプリセットとモディファイア適用、ヘッダ/行追加、最終的な文字列化
- 複雑箇所: ほぼ直線的だが、ANSIカラー利用コメントと実装が一部矛盾・ゼロ時間の合計行抑制などの仕様判断
- 重大リスク: エラー/unsafe/並行性は存在せず安全だが、行数不一致時の表示の乱れ・大規模データ時のメモリ/文字列化負荷は注意
- パフォーマンス: **O(n)**（summaryで結果件数に比例）、その他は定数規模だが出力文字列長に比例してコスト増
- テスト: 単体テストはビルダーの基本動作のみ。ゼロ時間や大量データなどのエッジケースの追加が有用

## Overview & Purpose

このファイルは、構造化された出力のためにテーブルを生成する**表示ユーティリティ**です。内部的に**comfy_table**クレートを利用し、以下を簡便に行えるようにします。

- **TableBuilder**で柔軟にヘッダ/行を追加してからまとめて出力
- **create_benchmark_table**でベンチマーク結果を2列のメトリクス表として出力
- **create_summary_table**で言語ごとのファイル数/シンボル数/時間/レートの一覧と合計行を出力

出力は**UTF8_FULL**プリセットと**UTF8_ROUND_CORNERS**モディファイアにより視覚的に整えられます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | TableBuilder | pub | テーブルのヘッダ/行をビルドして文字列化する | Low |
| Impl Method | TableBuilder::new | pub | プリセット・角丸適用済みのテーブル初期化 | Low |
| Impl Method | TableBuilder::set_headers | pub | ヘッダ行の設定（太字属性付与） | Low |
| Impl Method | TableBuilder::add_row | pub | データ行の追加 | Low |
| Impl Method | TableBuilder::build | pub | テーブルを文字列にレンダリング | Low |
| Function | create_benchmark_table | pub | ベンチ結果（言語/ファイル/記号数/平均時間/レート/性能指標）を2列表で出力 | Low |
| Function | create_summary_table | pub | 言語別の集計行とトータル行の一覧出力 | Med |

### Dependencies & Interactions

- 内部依存
  - 各関数/メソッドは独立しており、相互呼び出しはありません
  - すべてが**comfy_table::Table**に対する操作の薄いラッパです

- 外部依存（クレート/モジュール）
  | クレート/モジュール | 用途 |
  |---------------------|------|
  | comfy_table::{Table, Cell, Attribute, Color} | テーブル生成・セル作成・装飾（太字/色） |
  | comfy_table::presets::UTF8_FULL | 表のスタイルプリセット適用 |
  | comfy_table::modifiers::UTF8_ROUND_CORNERS | 角丸の外観適用 |
  | std::time::Duration | ベンチ/集計の時間表現 |

  バージョンや機能差分はこのチャンクには現れない（不明）。

- 被依存推定
  - CLIの出力やログの整形に使われる可能性が高い（具体的呼び出し元は不明）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| TableBuilder::new | fn new() -> Self | テーブルの初期化（プリセット・角丸適用） | O(1) | O(1) |
| TableBuilder::set_headers | fn set_headers(self, headers: Vec<&str>) -> Self | ヘッダ行設定（太字） | O(h) | O(Σlen(headers)) |
| TableBuilder::add_row | fn add_row(self, row: Vec<String>) -> Self | データ行追加 | O(m) | O(Σlen(row)) |
| TableBuilder::build | fn build(self) -> String | テーブル文字列化 | O(L) | O(L) |
| TableBuilder::default | fn default() -> Self | newの委譲 | O(1) | O(1) |
| create_benchmark_table | fn create_benchmark_table(language: &str, file_path: Option<&str>, symbols: usize, avg_time: Duration, rate: f64) -> String | ベンチメトリクス2列表 | O(1) + O(L) | O(L) |
| create_summary_table | fn create_summary_table(results: Vec<(String, usize, usize, Duration)>) -> String | 言語別集計とトータル行 | O(n) + O(L) | O(L) |

ここで、h/mはそれぞれヘッダ/行のセル数、nは結果件数、Lは最終出力文字列長に比例します。

### TableBuilder::new

1) 目的と責務
- **UTF8_FULL**プリセットと**UTF8_ROUND_CORNERS**を適用したTableを初期化

2) アルゴリズム
- Table::newを呼び出し
- load_preset(UTF8_FULL)
- apply_modifier(UTF8_ROUND_CORNERS)
- Selfに格納して返す

3) 引数
| 名称 | 型 | 説明 |
|------|----|------|
| なし | - | なし |

4) 戻り値
| 型 | 説明 |
|----|------|
| TableBuilder | 初期化済みビルダー |

5) 使用例
```rust
let builder = TableBuilder::new();
```

6) エッジケース
- 特になし（const動作）

### TableBuilder::set_headers

1) 目的と責務
- ヘッダ文字列を**太字**セルに変換しテーブルに設定

2) アルゴリズム
- headersをCellへマップ（Attribute::Bold付与）
- table.set_headerで設定
- selfを返す（ビルダー連鎖）

3) 引数
| 名称 | 型 | 説明 |
|------|----|------|
| headers | Vec<&str> | カラム名の配列 |

4) 戻り値
| 型 | 説明 |
|----|------|
| Self | ビルダー継続用 |

5) 使用例
```rust
let builder = TableBuilder::new()
    .set_headers(vec!["Col1", "Col2"]);
```

6) エッジケース
- 空配列の場合、ヘッダなしのテーブルになる

### TableBuilder::add_row

1) 目的と責務
- 1行のデータを追加

2) アルゴリズム
- table.add_row(row) を呼ぶ（StringはCellへ自動変換）

3) 引数
| 名称 | 型 | 説明 |
|------|----|------|
| row | Vec<String> | セル文字列の配列 |

4) 戻り値
| 型 | 説明 |
|----|------|
| Self | ビルダー継続用 |

5) 使用例
```rust
let builder = TableBuilder::new()
    .set_headers(vec!["Col1", "Col2"])
    .add_row(vec!["A".to_string(), "B".to_string()]);
```

6) エッジケース
- 行のセル数がヘッダ数と不一致でもcomfy_tableは受理するが、体裁が崩れる可能性あり

### TableBuilder::build

1) 目的と責務
- テーブルを**String**にレンダリングする

2) アルゴリズム
- table.to_string()

3) 引数
| 名称 | 型 | 説明 |
|------|----|------|
| なし | - | selfを消費 |

4) 戻り値
| 型 | 説明 |
|----|------|
| String | 完成した表の文字列 |

5) 使用例
```rust
let s = TableBuilder::new()
    .set_headers(vec!["Col1", "Col2"])
    .add_row(vec!["A".to_string(), "B".to_string()])
    .build();
println!("{s}");
```

6) エッジケース
- テーブルが空でも空の枠やヘッダのみが出力される（表示はプリセット次第）

### create_benchmark_table

1) 目的と責務
- ベンチマーク結果を「Metric」「Value」の2列表で表示し、**性能指標**を色+太字で強調

2) アルゴリズム
- 新規Tableにプリセット/角丸適用
- ヘッダを設定（太字）
- 行を追加（Language、File、Symbols parsed、Average time、Rate）
- rate/10_000を性能比とし、しきい値を1.0で分岐
  - 1.0以上: 緑色で「✓ x.xx faster than target」
  - 未満: 黄色で「⚠ x.xx of target」
- table.to_string()

3) 引数
| 名称 | 型 | 説明 |
|------|----|------|
| language | &str | 言語名 |
| file_path | Option<&str> | 対象ファイルパス（None時はプレースホルダ表示） |
| symbols | usize | 解析シンボル数 |
| avg_time | Duration | 平均処理時間 |
| rate | f64 | シンボル/秒 |

4) 戻り値
| 型 | 説明 |
|----|------|
| String | 表示用の2列テーブル文字列 |

5) 使用例
```rust
let s = create_benchmark_table(
    "Rust",
    Some("/path/to/file.rs"),
    120_000,
    std::time::Duration::from_millis(250),
    480_000.0,
);
println!("{s}");
```

6) エッジケース
- file_path=Noneの場合「<generated benchmark code>」
- rateが0.0のとき性能比は0.0で黄色警告
- rateが極端に大きい/小さい場合でもフォーマットは成功する
- ANSIカラーに関するコメントと実装が部分的に矛盾（後述）

### create_summary_table

1) 目的と責務
- 複数言語の解析結果を表で表示し、**合計行**（TOTAL）を追加

2) アルゴリズム
- 新規Tableにプリセット/角丸適用
- ヘッダ（Language, Files, Symbols, Time, Rate）を太字で設定
- 各結果行を走査し、合計（files/symbols/time）を加算
- 各行のrate = symbols / time（秒）; time=0なら0
- total_time>0のときのみTOTAL行を追加
- table.to_string()

3) 引数
| 名称 | 型 | 説明 |
|------|----|------|
| results | Vec<(String, usize, usize, Duration)> | (言語, ファイル数, シンボル数, 時間)のタプル配列 |

4) 戻り値
| 型 | 説明 |
|----|------|
| String | 表示用のテーブル文字列 |

5) 使用例
```rust
let summary = vec![
    ("Rust".to_string(), 10, 200_000, std::time::Duration::from_secs(2)),
    ("Python".to_string(), 8, 150_000, std::time::Duration::from_secs(3)),
];
let s = create_summary_table(summary);
println!("{s}");
```

6) エッジケース
- timeが0秒のエントリはrate=0/sとして表示
- 総計timeが0秒のときはTOTAL行が追加されない（設計判断）
- 極端に大きな件数でテーブルが長くなると文字列化/メモリ負荷が増える

## Walkthrough & Data Flow

- TableBuilderの一般的利用フロー
  - new → set_headers → add_row（繰り返し） → build
  - 各ステップは**selfを消費して返す**チェーンスタイル。所有権が遷移するため誤用が減る。

- create_benchmark_table
  - 入力（言語/パス/シンボル/時間/レート）→ 固定ヘッダ設定 → メトリクス行追加 → 性能比計算（rate/10_000）→ 色付きセル追加 → 文字列化

- create_summary_table
  - 入力（タプル配列）→ 走査で各行を追加しつつ合計を累積 → total_time>0でTOTAL行追加 → 文字列化

いずれも外部I/Oや状態共有はなく、**純粋な計算→フォーマット→文字列化**の直線的データフローです。

## Complexity & Performance

- 時間計算量
  - TableBuilder::new/build: O(1)（ただしbuildは出力長Lに比例）
  - set_headers: O(h)
  - add_row: O(m)
  - create_benchmark_table: O(1)（固定行）+ O(L)
  - create_summary_table: O(n)（結果件数）+ O(L)

- 空間計算量
  - いずれも**最終文字列長L**に比例したメモリ確保が発生。行数・セル内容に比例して増加。

- ボトルネック/スケール限界
  - 大規模データ（nが大）の場合、テーブル文字列化（to_string）で大量の連結/割り当てが発生し、**CPU/メモリ**を消費
  - ANSI装飾（太字/色）はレンダリングコストをわずかに増やすが、支配的ではない
  - I/O/ネットワーク/DBは関与しないため、主な負荷は文字列処理のみ

## Edge Cases, Bugs, and Security

- セキュリティチェックリスト
  - メモリ安全性: Rust安全な標準APIのみ使用。**unsafeブロックなし**。Buffer overflow/Use-after-free/Integer overflowの懸念はなし。
  - インジェクション: 文字列フォーマットのみ。SQL/Command/Path traversalは該当なし。
  - 認証・認可: 該当なし。
  - 秘密情報: ハードコード秘密/ログ漏えいはなし。
  - 並行性: 共有状態なし、非同期なし。Race/Deadlockは該当なし。

- 仕様/表示上の懸念
  - コメントの矛盾: 「ANSI colorsは苦手」とコメントしつつ、**Performance**セルにColorを適用し、ヘッダにBold属性も付与。仕様整合を検討すべき。
  - 行/列不一致: ヘッダ数と行セル数が一致しない場合、comfy_tableは受理するが整形が崩れる可能性。
  - ゼロ時間時のTOTAL行非表示: total_time==0で合計行を出さない設計は、ユーザ期待と異なる場合あり。
  - 極端なrateやdurationの表示丸め: summaryのrateは「{:.0}/s」で小数を丸める。精度が必要なら可変精度が望ましい。

- Rust特有の観点
  - 所有権: ビルダーの各メソッドは**selfを消費**して返すため、誤った再利用が防止される（TableBuilder::{set_headers, add_row, build}）。
  - 借用/ライフタイム: headersはVec<&str>だが、Cell::newで文字列化されテーブルに所有される。外部参照のライフタイム問題はなし。
  - unsafe境界: 使用なし。
  - 並行性/非同期: Send/Syncの境界やawaitは登場しない（このチャンクには現れない）。
  - エラー設計: Result/Optionの使い分けは軽微。file_pathのみOption。その他はエラーを返さない設計。panic/unwrap/expectの使用なし。

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空ヘッダ | headers=[] | ヘッダ無しテーブルの生成 | set_headersは空を許容 | 想定内 |
| 行/列不一致 | headers=3列, row=2セル | 表示は可能だが体裁が崩れる | add_rowは検証しない | 改善余地 |
| rate=0 | rate=0.0 | ベンチ表の性能比が0.0で警告表示 | 分岐でYellow表示 | 想定内 |
| time=0（summary行） | Duration::ZERO | rate=0/s表示 | if time>0でrate計算 | 想定内 |
| TOTAL行の非表示 | total_time=0 | TOTAL行を出さない | total_time>0のみ追加 | 仕様要確認 |
| 非ASCII/長セル | 極端に長い文字列 | 正しく表示（折返しはプリセット依存） | comfy_tableへ委譲 | 想定内 |
| ANSIサポート不均一 | 古い端末 | 太字/色が効かない | 端末依存 | 既知リスク |

根拠は該当関数のロジック。行番号はこのチャンクでは不明。

## Design & Architecture Suggestions

- 一貫したスタイル方針
  - コメントに合わせて「ANSI装飾の有無」を**設定可能（フラグ）**にし、ヘッダBold/Performance色をトグルで制御
- 入力検証
  - **ヘッダと行セル数の整合**をチェックし、警告ログや補完（空セル追加）を行うオプション
- 表示精度
  - summaryのrateのフォーマット精度（小数桁）を**パラメータ化**し、用途別に調整
- 合計行の方針
  - total_time==0でもTOTAL行を表示し、rateは「N/A」などの表記にする方がユーザに一貫性を提供
- ビルダーの柔軟性
  - **set_headers**を`impl Into<Cell>`で受ける、**add_row**も`Into<Cell>`のジェネリック対応にして&str/String混在を許容（現在も内部でInto<Cell>に近いがAPI表面を明示）
  - 列アライメント/最大幅/ラップ設定などを**ビルダーメソッド**で提供

## Testing Strategy (Unit/Integration) with Examples

- 既存テスト
  - TableBuilderの基本動作（ヘッダ/行の含有チェック）

- 追加が望ましい単体テスト
  1) ヘッダ無しテーブル
  ```rust
  #[test]
  fn test_empty_headers() {
      let s = TableBuilder::new()
          .set_headers(vec![])
          .add_row(vec!["A".to_string()])
          .build();
      assert!(s.contains("A"));
  }
  ```
  2) 行/列不一致
  ```rust
  #[test]
  fn test_mismatched_columns() {
      let s = TableBuilder::new()
          .set_headers(vec!["H1", "H2", "H3"])
          .add_row(vec!["A".to_string(), "B".to_string()])
          .build();
      // 体裁検証は難しいがとりあえず生成成功を確認
      assert!(s.contains("H1"));
      assert!(s.contains("A"));
  }
  ```
  3) ベンチテーブルのゼロレート
  ```rust
  #[test]
  fn test_benchmark_zero_rate() {
      let s = create_benchmark_table("Rust", None, 0, std::time::Duration::ZERO, 0.0);
      assert!(s.contains("Performance"));
      assert!(s.contains("⚠"));
  }
  ```
  4) サマリのゼロ時間とTOTAL行非表示
  ```rust
  #[test]
  fn test_summary_total_hidden_on_zero_time() {
      let s = create_summary_table(vec![
          ("Rust".to_string(), 1, 10, std::time::Duration::ZERO)
      ]);
      assert!(s.contains("Rust"));
      assert!(!s.contains("TOTAL"));
  }
  ```
  5) 大量行のパフォーマンス（簡易）
  ```rust
  #[test]
  fn test_summary_many_rows() {
      let mut rows = Vec::new();
      for i in 0..1000 {
          rows.push((format!("Lang{i}"), 1, 100, std::time::Duration::from_millis(10)));
      }
      let s = create_summary_table(rows);
      assert!(s.contains("Lang0") && s.contains("Lang999"));
  }
  ```

- 統合テスト（端末環境）
  - ANSI装飾の可視性（Bold/Color）が端末で意図通りかを確認（自動化は難しいためスナップショットテストで代替）

## Refactoring Plan & Best Practices

- APIの柔軟化
  - `set_headers<T: Into<Cell>>(&self, headers: Vec<T>)`、`add_row<T: Into<Cell>>(&self, row: Vec<T>)`のジェネリック化
- 装飾トグル
  - ビルダーに`enable_styles(bool)`を追加し、Bold/Colorの一括切替
- 合計行のポリシー
  - `include_total_when_zero_time(bool)`フラグや「N/A」表記の導入
- フォーマット責務の分離
  - レート計算/フォーマットを**小関数**へ分離してテスト容易性向上
- ドキュメント強化
  - ANSI可否、行/列整合性、ゼロ時間の扱いなどの仕様を**Rustdoc**に明記

## Observability (Logging, Metrics, Tracing)

- ロギング
  - 大量行時に「生成された行数/文字列サイズ」などの統計をdebugログ出力（このモジュール内では依存を増やしたくないため、呼び出し側で計測するのが無難）
- メトリクス
  - テーブル生成時間や出力長をメトリクスとして収集すると、規模増加時のボトルネック把握に役立つ
- トレース
  - 本処理は軽量のため必須ではないが、複合パイプラインの一部ならスパンを付与して可視化（このチャンクには現れない）

## Risks & Unknowns

- 呼び出し側の期待仕様が不明（合計行の非表示、ANSI装飾の方針、レートの小数桁）
- 端末/環境ごとの差異（ANSI可視性、フォント/幅計算）の影響範囲は不明
- comfy_tableのバージョン/設定（折返し、幅制御、アライメント）詳細は不明（このチャンクには現れない）