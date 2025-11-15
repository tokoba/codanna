# indexing\progress.rs Review

## TL;DR

- 目的: インデックス処理の進捗・統計を収集し、簡易表示するためのシンプルなユーティリティ（構造体は**IndexStats**）。
- 公開API: **IndexStats::new**, **stop_timing**, **add_error**, **display**、および公開フィールド（files_indexed, files_failed, symbols_found, elapsed, errors）。
- コアロジック: 100件までのエラー記録、処理時間の計測（開始はnew、停止はstop_timing）、人間向けの表示（最大5件のエラー詳細）。
- 重要なリスク: 経過時間が0秒のとき、パフォーマンス計算で除算が0となり、∞が表示される可能性。公開フィールドにより整合性が崩れる危険（エラー数制限の迂回など）。
- 安全性: unsafeは不使用、所有権・借用は明瞭。並行使用は未対応（非Atomicなusize、Vecの共有は要Mutex等）。
- テスト: 表示関数のパニック無しとエラー100件制限のみを確認。ゼロ経過時間や並行更新のテストは未整備。
- パフォーマンス: すべてO(1)（add_errorはベクタpushの償却O(1)、displayは最大5件の出力でO(1)）。メモリは最大エラー100件分。

## Overview & Purpose

このファイルは、インデックス処理（ファイル解析等）における進捗と基本的な統計値を収集・表示するためのヘルパーです。主な機能は以下のとおりです。

- 処理開始時刻の記録と経過時間の計測（newで開始、stop_timingで停止）。
- 成功・失敗ファイル数と見つかったシンボル数のカウント。
- 失敗時のエラー情報（Pathとメッセージ）の保存。保存は先頭100件まで。
- 結果の人間可読出力（標準出力へのprintln）。

本チャンクにはインデックス処理自体の実装は存在せず、統計の収集・表示のみが定義されています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | IndexStats | pub | インデックス統計の保持（件数、時間、エラー一覧） | Low |
| Method | IndexStats::new | pub | 計測開始（start_time設定）と初期化 | Low |
| Method | IndexStats::stop_timing | pub | 経過時間の確定とstart_timeのクリア | Low |
| Method | IndexStats::add_error | pub | 失敗件数の増加とエラー100件までの保存 | Low |
| Method | IndexStats::display | pub | 統計の標準出力表示（最多5件のエラー詳細） | Low |

### Dependencies & Interactions

- 内部依存
  - **new**が**start_time**（Option<Instant>）をSomeに設定。
  - **stop_timing**が**start_time**を参照して**elapsed**（Duration）を更新し、**start_time**をNoneにする。
  - **add_error**が**errors**にpush（最大100件）し、**files_failed**をインクリメント。
  - **display**が**files_indexed**、**files_failed**、**symbols_found**、**elapsed**、**errors**を読み取り、整形出力。
- 外部依存（標準ライブラリのみ）
  | 依存 | 用途 |
  |------|------|
  | std::path::PathBuf | エラー発生ファイルのパス保持 |
  | std::time::{Duration, Instant} | 経過時間計測 |
  | println! | コンソールへの表示 |
- 被依存推定
  - インデックス実行モジュールやタスクランナーが、処理ループ中に**files_indexed**や**symbols_found**を更新し、失敗時に**add_error**を呼び、最終的に**stop_timing**と**display**を呼ぶ構成が想定されます（詳細はこのチャンクには現れない）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| IndexStats::new | `pub fn new() -> Self` | 統計の初期化と計測開始 | O(1) | O(1) |
| IndexStats::stop_timing | `pub fn stop_timing(&mut self)` | 経過時間の確定 | O(1) | O(1) |
| IndexStats::add_error | `pub fn add_error(&mut self, path: PathBuf, error: String)` | エラー記録（最大100件）と失敗数増加 | 償却O(1) | O(1)（最大100件まで増加） |
| IndexStats::display | `pub fn display(&self)` | 統計の人間可読表示 | O(1)（最大5件の出力） | O(1) |

データ契約（IndexStatsの公開フィールド）
- **files_indexed: usize** … 正常に処理されたファイル件数。外部から直接加算される。
- **files_failed: usize** … 失敗件数。通常は**add_error**で加算されるが、公開ゆえに外部からも変更可能。
- **symbols_found: usize** … 見つかったシンボル総数。外部から更新。
- **elapsed: Duration** … 経過時間。**stop_timing**により設定されるが、公開ゆえに外部からも変更可能。
- **errors: Vec<(PathBuf, String)>** … エラー一覧。**add_error**は100件に制限するが、公開ゆえに外部からpush可能（契約上の逸脱に注意）。
- start_time: Option<Instant> … 非公開。newでSomeに設定、stop_timingでNoneに。

詳細（各API）

1) IndexStats::new
- 目的と責務
  - 統計を初期化し、計測の開始時刻を設定します。
- アルゴリズム
  - Default値で構造体を初期化。
  - start_timeにInstant::now()を設定。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | なし | - | - |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | IndexStats | 初期化済みの統計オブジェクト |
- 使用例
  ```rust
  let mut stats = IndexStats::new();
  // ここから計測開始
  ```
- エッジケース
  - 特になし（計測開始前提）。

2) IndexStats::stop_timing
- 目的と責務
  - 計測を停止し、開始時刻からの経過時間をelapsedに記録します。
- アルゴリズム
  - start_timeがSomeなら、そのInstantからのelapsedを取得。
  - elapsedに格納し、start_timeをNoneに。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | &mut self | - | 経過時間を更新 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | () | なし |
- 使用例
  ```rust
  let mut stats = IndexStats::new();
  // ... インデックス処理 ...
  stats.stop_timing();
  ```
- エッジケース
  - start_timeがNoneの場合は何もしない（二重停止の安全性あり）。

3) IndexStats::add_error
- 目的と責務
  - 失敗件数を増やし、最初の100件までエラー詳細を保持します。
- アルゴリズム
  - errors.len() < 100なら (path, error) をpush。
  - files_failedをインクリメント。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | path | PathBuf | 失敗ファイルのパス（所有） |
  | error | String | エラーメッセージ（所有） |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | () | なし |
- 使用例
  ```rust
  stats.add_error(std::path::PathBuf::from("foo.rs"), "Parse error".to_string());
  ```
- エッジケース
  - 101件目以降は失敗件数のみ増加し、errorsには追加されない。

4) IndexStats::display
- 目的と責務
  - 統計を人間可読な形式で標準出力に表示します（最大5件のエラー詳細）。
- アルゴリズム
  - 件数と時間をprintln。
  - files_indexed > 0 の場合は、ファイル/秒と平均シンボル/ファイルを計算・表示。
  - errorsが空でなければ、最大5件を表示し、超過件数を通知。
- 引数
  | 名前 | 型 | 説明 |
  |------|----|------|
  | &self | - | 表示対象 |
- 戻り値
  | 型 | 説明 |
  |----|------|
  | () | なし |
- 使用例
  ```rust
  let mut stats = IndexStats::new();
  // 例: インデックス成功・失敗・シンボル数を更新
  stats.files_indexed = 42;
  stats.symbols_found = 420;
  stats.stop_timing();
  stats.display(); // 標準出力へ
  ```
- エッジケース
  - elapsedが0秒の場合、ファイル/秒の計算で除算が0となり∞が出力される可能性あり。

## Walkthrough & Data Flow

- 初期化
  - 呼び出し元で**IndexStats::new**を作成。start_timeが設定され、統計値は0/空に初期化。
- インデックス処理中
  - 成功時に呼び出し元で**files_indexed**をインクリメント。
  - シンボル検出時に呼び出し元で**symbols_found**を加算。
  - 失敗時に**add_error**で詳細を記録（最大100件）し、**files_failed**が加算。
- 処理終了
  - **stop_timing**で**elapsed**が確定し、**start_time**はNone。
- 表示
  - **display**で統計を標準出力へ整形表示。エラーは最大5件までの詳細を表示し、超過分は件数のみ示す。

データフローの要点
- start_time → stop_timing → elapsed（一方向の確定）。
- errorsは最大100件に制限されるが、公開フィールドゆえ外部変更で上限を超え得る（契約逸脱の可能性）。
- 表示は読み取り専用で、副作用は標準出力のみ。

上記の動作はコード上の該当関数に基づいています（関数名は明示、行番号はこのチャンクには提示されていないため不明）。

## Complexity & Performance

- 時間計算量
  - **new**, **stop_timing**: O(1)
  - **add_error**: 償却O(1)（Vecのpush）。最大100件に制限されているため、再割り当て回数も限定的。
  - **display**: O(1)。エラー表示は最大5件のみループ。
- 空間計算量
  - 定数オーバーヘッド。**errors**は最大100件まで増加するためO(100)≒O(1)。
- ボトルネック
  - 出力I/O（println!）が支配的。大量の連続呼び出しは標準出力のロック/フラッシュで遅くなる可能性。
- スケール限界
  - エラー保存上限（100件）によりメモリは安定。ただし、各エントリのPathBufとStringが巨大な場合はそれ相応のメモリを消費。
- 実運用負荷要因
  - 標準出力のI/O待ち。
  - 呼び出し元の統計更新頻度（このモジュール自体は軽量）。

## Edge Cases, Bugs, and Security

エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 経過時間0でのパフォーマンス計算 | files_indexed=10, elapsed=0s | 安全にスキップまたは0/不明の表示 | elapsedで除算0→∞表示の可能性 | 要改善 |
| files_indexed=0での平均計算 | files_indexed=0 | 平均シンボル/ファイルの計算をスキップ | if files_indexed>0でガード | OK |
| エラー数上限 | add_errorを150回呼ぶ | 最初の100件のみ保存、失敗件数は150 | len<100でpush、files_failed++ | OK（テストあり） |
| 公開フィールドによる契約逸脱 | stats.errors.push(...) | 上限を超えないように制御 | フィールドがpubのため制御不能 | リスク |
| 非常に長いメッセージ/パス | 1件あたりMB級のString | ログの肥大を抑制 | 制限なし（そのまま表示） | リスク |
| stop_timing未呼び出し | new直後にdisplay | 時間未確定でも合理的な表示 | 0秒で表示、性能計算が∞の可能性 | 要改善 |
| 二重stop_timing | stop_timingを2回 | 2回目は無視される | Noneチェックで安全 | OK |

セキュリティチェックリスト

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 不明瞭な危険はなし（unsafe未使用、usizeの加算のみ）。極端なカウントでも安全に表現可能。ただし極端に大きなString/PathBufに伴うメモリ使用増はあり得る。
- インジェクション
  - SQL/Command/Path traversal: 対象外（このモジュールは出力のみ）。ただし、外部から受け取るエラーメッセージをそのまま表示するため、ターミナル制御文字によるログ汚染の可能性は理論上あり。
- 認証・認可
  - 該当なし（このチャンクには現れない）。
- 秘密情報
  - Hard-coded secrets: なし。
  - Log leakage: エラーメッセージに機密が含まれる場合そのまま表示されるため、秘匿化やフィルタリングが望ましい。
- 並行性
  - Race condition / Deadlock: 本モジュール単体では同期制御無し。複数スレッドから共有更新する場合はArc<Mutex<IndexStats>>等が必要。Atomicでのカウンタに置換も検討。

Rust特有の観点

- 所有権
  - **add_error**が`PathBuf`と`String`を所有で受け取り、`Vec`へムーブ（関数名:行番号不明）。過剰なクローンは発生せず明瞭。
- 借用
  - 可変借用は**stop_timing**/**add_error**で短期間のみ。データ競合の可能性は外部共有の仕方次第。
- ライフタイム
  - 明示的パラメータ不要。すべて所有データ。
- unsafe境界
  - 使用なし。
- 並行性・非同期
  - Send/Syncは自動導出に依存（PathBuf, String, VecはSend）。ただし内部変更は外部の同期が必要。
  - await境界/キャンセル: 該当なし。
- エラー設計
  - Result/Optionは**start_time**にOptionを使用。APIはResultを返さず、失敗しない設計。
  - panic箇所: 直接的なunwrap/expectは無し。println!はI/O失敗でパニックしない設計（標準出力書き込みエラーは無視されがち）。

## Design & Architecture Suggestions

- フィールドの可視性の見直し
  - ✅ **errors**や**elapsed**を非公開にし、アクセサメソッドを提供。上限や整合性を強制できます。
  - ✅ **files_indexed**/**files_failed**/**symbols_found**も非公開にし、インクリメントAPI（例: inc_indexed, inc_symbols, record_failure）を用意。
- 安全な表示ロジック
  - ✅ **elapsed==0**のとき、パフォーマンス表示をスキップするか「N/A」を表示。
  - ✅ 出力は`std::fmt::Display`の実装や`fn write_to<W: Write>(&self, w: &mut W)`に分離してテスト容易化。
  - ✅ 構造化ログ（例: `tracing`で`info!(files_indexed=?, ...)`）を活用し、テキスト整形よりも機械処理可能なログに。
- API改善
  - ✅ **add_error**は`impl AsRef<Path>`と`impl Into<Cow<'a, str>>`等の受け口にしてコールサイトの負担軽減。
  - ✅ 上限値（100）を定数化（`const MAX_ERRORS: usize = 100;`）し、可視化・再利用を容易に。
  - ✅ エラー構造体（path, kind, message, severity等）に拡張可能な型を導入。
- 並行性対応
  - ✅ カウンタを`AtomicUsize`に、エラー格納を`Mutex<Vec<...>>`/`RwLock`に。もしくは全体を`Arc<Mutex<IndexStats>>`で包む。
  - ✅ 高頻度更新ならロック粒度を最適化（カウンタはAtomic、エラーはロック）し、ロック競合を最小化。
- ユーザビリティ
  - ✅ エラーの保存方針（先頭N件vs末尾N件）を要件に合わせて選択。一般には「最近のN件」が有用なことが多く、`VecDeque`で末尾N件維持を検討。
  - ✅ 表示の小数点精度や単位を設定可能に。

## Testing Strategy (Unit/Integration) with Examples

追加で望ましいテスト

- 経過時間0のときの表示スキップ/「N/A」ロジック（改善後）
- 二重stop_timingの安全性検証
- エラー保存の上限（既存テスト済み）に加え、公開フィールドを通じた逸脱が起きないよう可視化改善後のテスト
- 巨大メッセージ/パスの扱い（表示のトリミングがあればその確認）
- 並行更新（Mutex/Atomic導入後のレース無しを検証）

表示のテスト容易化のため、Writer受け取り関数へのリファクタ例（提案用コード）

```rust
use std::io::{self, Write};

impl IndexStats {
    // 提案: 標準出力へ直接書かず、任意のWriterに出力
    pub fn write_to<W: Write>(&self, mut w: W) -> io::Result<()> {
        writeln!(w, "\nIndexing Complete:")?;
        writeln!(w, "  Files indexed: {}", self.files_indexed)?;
        writeln!(w, "  Files failed: {}", self.files_failed)?;
        writeln!(w, "  Symbols found: {}", self.symbols_found)?;
        writeln!(w, "  Time elapsed: {:.2}s", self.elapsed.as_secs_f64())?;

        if self.files_indexed > 0 {
            let elapsed_s = self.elapsed.as_secs_f64();
            if elapsed_s > 0.0 {
                let files_per_sec = self.files_indexed as f64 / elapsed_s;
                writeln!(w, "  Performance: {files_per_sec:.0} files/second")?;
            } else {
                writeln!(w, "  Performance: N/A (elapsed=0)")?;
            }
            let symbols_per_file = self.symbols_found as f64 / self.files_indexed as f64;
            writeln!(w, "  Average symbols/file: {symbols_per_file:.1}")?;
        }

        if !self.errors.is_empty() {
            let shown = self.errors.len().min(5);
            writeln!(w, "\nErrors (showing first {}):", shown)?;
            for (path, error) in &self.errors[..shown] {
                writeln!(w, "  {}: {}", path.display(), error)?;
            }
            if self.errors.len() > shown {
                writeln!(w, "  ... and {} more errors", self.errors.len() - shown)?;
            }
        }
        Ok(())
    }
}
```

使用したテスト例（提案）

```rust
#[test]
fn test_display_zero_elapsed() {
    let mut stats = IndexStats::new();
    stats.files_indexed = 10;
    stats.elapsed = std::time::Duration::from_secs(0);

    let mut buf = Vec::new();
    stats.write_to(&mut buf).unwrap();
    let out = String::from_utf8(buf).unwrap();
    assert!(out.contains("Performance: N/A"));
}

#[test]
fn test_double_stop_timing_is_safe() {
    let mut stats = IndexStats::new();
    stats.stop_timing();
    // 再度呼んでもパニックしない
    stats.stop_timing();
}
```

このテストは提案リファクタ後に有効です。現行displayは標準出力直書きのため、直接の出力検証は困難です。

## Refactoring Plan & Best Practices

- フィールド非公開化とアクセサ導入
  - errors/elapsed/カウンタ類をprivateにし、更新APIで不変条件（上限100など）を強制。
- 定数化
  - MAX_ERRORSを明示定数に。
- 出力分離
  - displayをWriter受け取りに分離、`impl Display for IndexStats`で整形責務を集約。
- 0除算対策
  - elapsed.as_secs_f64() == 0.0 の場合はパフォーマンス表示をスキップ/特別表示。
- APIの受け取り型改善
  - add_errorに`impl AsRef<Path>`と`impl Into<String>`/`Cow<'_, str>`を導入し、利便性と不要なclone回避。
- 並行性
  - 高頻度更新のためAtomicUsize、error一覧はMutex/RwLockなど。
- ロギング
  - printlnではなく`tracing`/`log`を使用し、運用環境で制御可能なログ出力へ。
- 拡張性
  - エラー型の導入、JSON/Prometheus向けのメトリクスエクスポート。

## Observability (Logging, Metrics, Tracing)

- ログ
  - `tracing`で構造化イベントを発行（files_indexed, files_failed, symbols_found, elapsed）。
  - エラーはseverity（warn/error）に応じたレベルで記録。機密情報をマスクするフィルタ機構を用意。
- メトリクス
  - counters/gauges: files_indexed, files_failed, symbols_found。
  - histogram/timer: elapsedを記録、処理時間分布を観測。
- トレーシング
  - インデックス処理全体にSpanを張り、**IndexStats**の開始・終了で入出時イベントを発行。
- 出力制御
  - 環境変数や設定でログレベル・出力先（stdout, file, syslog）を切り替え可能に。

## Risks & Unknowns

- 利用箇所
  - このモジュールがどこから・どう呼ばれるかは不明（このチャンクには現れない）。
- 並行性要件
  - 複数スレッドが同時に統計を更新する設計かは不明。必要に応じて同期化が必要。
- エラー保存ポリシー
  - 「最初の100件」か「最後の100件」か、要件の確定が不明。
- 出力要件
  - 標準出力で十分か、ログ/メトリクス/GUI連携が必要か不明。
- 行番号
  - 重要主張の行番号は、このチャンクに行番号が提示されていないため不明。