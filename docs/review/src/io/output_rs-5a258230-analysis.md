# io/output.rs Review

## TL;DR

- **目的**: CLIの出力を統一化し、**Text/JSON**の両フォーマットを安全に扱う。特に**BrokenPipe**（EPIPE）を正常系として無視する方針。
- **主要公開API**: **OutputManager**のメソッド群（success, item, not_found, collection, error, progress, info, symbol_contexts, unified）で、結果/コレクション/エラー/進捗/統合構造の出力を提供。
- **複雑箇所**: **unified**の多岐分岐（JSON直列化 vs TextでのEmpty/Items空/Display出力＋guidance）と**BrokenPipe**無視の一貫適用。
- **重大リスク**: `serde_json::to_string_pretty(...) ?` を**io::Result**の関数で直接`?`しており、**serde_json::Error → io::Error**変換が未定義だとコンパイル不可。要エラー変換。
- **Rust安全性**: unsafeなし。**Box<dyn Write>**を都度`&mut dyn Write`で借用し、**BrokenPipe**以外のI/Oエラーは伝播。所有権/借用は妥当。
- **並行性**: 非同期なし、スレッド安全保証なし（Send/Sync未明）。単一スレッド前提の設計。
- **テスト**: BrokenPipeの扱い、JSON/Text両モードの成功/未検出の振る舞い、SymbolContextの出力が網羅。追加で「非BrokenPipeエラー伝播」や`unified`のguidance出力などのテスト強化が望ましい。

## Overview & Purpose

本ファイルはCLIコマンドの出力管理を行う**OutputManager**の実装であり、**Text**と**JSON**の2種類のフォーマットを統一インターフェースで扱います。主な目的は次の通りです。

- 結果・コレクション・エラー・進捗の出力を統一化。
- **BrokenPipe（EPIPE）**を正常系として無視し、パイプ（head/grepなど）での利用時に**ExitCode**を操作の成功/失敗にのみ基づかせる。
- JSONモードでは構造化レスポンス（JsonResponse、UnifiedOutput）でメタ情報・ガイダンスを含めて序列化する。
- Textモードでは`Display`に基づく人間可読な表示、`stderr`への進捗/ガイダンスの出力を行う。

根拠（関数名:行番号不明）：`OutputManager::success`, `OutputManager::collection`, `OutputManager::error`, `OutputManager::progress`, `OutputManager::info`, `OutputManager::unified`

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | OutputManager | pub | 出力フォーマットに応じたstdout/stderrへの整形出力 | Med |
| Fn | new | pub | 実環境のstdout/stderrを束縛したOutputManager生成 | Low |
| Fn | new_with_writers | pub (doc hidden) | テスト用に任意の`Write`を注入 | Low |
| Fn | write_ignoring_broken_pipe | private | BrokenPipeを無視する安全な書き込みユーティリティ | Low |
| Fn | success | pub | 成功結果出力（Text/JSON） | Low |
| Fn | item | pub | Option<T>を成功/未検出に分岐出力 | Low |
| Fn | not_found | pub | 未検出出力（ExitCode::NotFound） | Low |
| Fn | collection | pub | コレクションを整形出力（空はNotFound） | Med |
| Fn | error | pub | エラー＋リカバリ提案を出力、ExitCode変換 | Low |
| Fn | progress | pub | 進捗メッセージ（Textのみ、stderr） | Low |
| Fn | info | pub | 情報メッセージ（Textのみ、stdout） | Low |
| Fn | symbol_contexts | pub | SymbolContext専用の整形出力 | Med |
| Fn | unified | pub | UnifiedOutputをJSON直列化/多分岐Text表示 | Med |
| Module | tests | private | BrokenPipeや出力パスの単体テスト | Med |

### Dependencies & Interactions

- 内部依存
  - `item` → `success` / `not_found`
  - `collection` → `not_found`, `write_ignoring_broken_pipe`
  - `error`, `success`, `not_found`, `progress`, `info`, `symbol_contexts`, `unified` → `write_ignoring_broken_pipe`
  - `unified` → `OutputData`の分岐（Empty/Items空/その他）
- 外部依存（表）

  | 依存 | 用途 | 備考 |
  |------|------|------|
  | crate::error::IndexError | エラー型 | `recovery_suggestions()`使用 |
  | crate::io::exit_code::ExitCode | 終了コード | `Success/NotFound/from_error` |
  | crate::io::format::{JsonResponse, OutputFormat} | フォーマット判定/JSONラッパ | JSON成功/未検出レスポンス |
  | crate::io::schema::{OutputData, UnifiedOutput} | 統一出力構造 | `unified`で使用 |
  | serde::Serialize, std::fmt::Display | 型境界 | ジェネリクス制約 |
  | std::io::{self, Write} | 出力I/O | stdout/stderr、エラー型 |
  | serde_json | JSON整形 | `to_string_pretty`使用（エラー変換課題あり） |

- 被依存推定
  - CLIコマンド実装（retrieve系など）からの利用。
  - JSON出力を必要とするAPI層/上位CLI。
  - テスト用に`new_with_writers`で`Vec<u8>`やテストダブルを注入。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | `pub fn new(format: OutputFormat) -> Self` | 実stdout/stderrでマネージャ生成 | O(1) | O(1) |
| new_with_writers | `pub fn new_with_writers(format: OutputFormat, stdout: Box<dyn Write>, stderr: Box<dyn Write>) -> Self` | テスト用DI | O(1) | O(1) |
| success | `pub fn success<T: Serialize + Display>(&mut self, data: T) -> io::Result<ExitCode>` | 成功結果のText/JSON出力 | O(n) | O(n) |
| item | `pub fn item<T: Serialize + Display>(&mut self, item: Option<T>, entity: &str, name: &str) -> io::Result<ExitCode>` | Option<T>を成功/未検出で出力 | O(n) | O(n) |
| not_found | `pub fn not_found(&mut self, entity: &str, name: &str) -> io::Result<ExitCode>` | 未検出の標準表現を出力 | O(1) | O(1) |
| collection | `pub fn collection<T: Serialize + Display, I: IntoIterator<Item = T>>(&mut self, items: I, entity_name: &str) -> io::Result<ExitCode>` | コレクション出力（空はNotFound） | O(n) | O(n) |
| error | `pub fn error(&mut self, error: &IndexError) -> io::Result<ExitCode>` | エラー＋提案の出力 | O(k + n) | O(n) |
| progress | `pub fn progress(&mut self, message: &str) -> io::Result<()>` | 進捗（Textのみ、stderr） | O(m) | O(1) |
| info | `pub fn info(&mut self, message: &str) -> io::Result<()>` | 情報（Textのみ、stdout） | O(m) | O(1) |
| symbol_contexts | `pub fn symbol_contexts(&mut self, contexts: impl IntoIterator<Item = crate::symbol::context::SymbolContext>, entity_name: &str) -> io::Result<ExitCode>` | SymbolContext専用出力 | O(n) | O(n) |
| unified | `pub fn unified<T: Serialize + Display>(&mut self, output: UnifiedOutput<'_, T>) -> io::Result<ExitCode>` | UnifiedOutputの統合的出力 | O(n) | O(n) |

注:
- n: 出力対象要素数またはシリアライズ文字列長に比例。
- k: `error.recovery_suggestions()`の件数。
- m: メッセージ長。

以下、各APIの詳細。

### OutputManager::new

1. 目的と責務
   - 指定の**OutputFormat**で、実際の`stdout`/`stderr`を用いる**OutputManager**を生成。

2. アルゴリズム
   - `io::stdout()`/`io::stderr()`を`Box<dyn Write>`に格納。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| format | OutputFormat | 出力フォーマット（Text/Json） |

4. 戻り値

| 値 | 型 | 説明 |
|----|----|------|
| manager | OutputManager | 構成済み出力マネージャ |

5. 使用例

```rust
use crate::io::format::OutputFormat;
use crate::io::output::OutputManager;

let mut out = OutputManager::new(OutputFormat::Text);
```

6. エッジケース
- 特になし。

根拠（関数名:行番号不明）

### OutputManager::new_with_writers

1. 目的と責務
   - テスト用DI。任意の`Write`実装を注入。

2. アルゴリズム
   - フィールドにそのままセット。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| format | OutputFormat | フォーマット |
| stdout | Box<dyn Write> | 出力先stdout |
| stderr | Box<dyn Write> | 出力先stderr |

4. 戻り値

| 値 | 型 | 説明 |
|----|----|------|
| manager | OutputManager | 指定ライター入り |

5. 使用例

```rust
let mut out = OutputManager::new_with_writers(OutputFormat::Json, Box::new(Vec::new()), Box::new(Vec::new()));
```

6. エッジケース
- ライターのエラー挙動は呼び出し側の責務。

根拠（関数名:行番号不明）

### OutputManager::success

1. 目的と責務
   - 正常系のデータをText/JSONで出力し、**ExitCode::Success**を返す。**BrokenPipe**を無視。

2. アルゴリズム
   - `format`を分岐。
     - JSON: `JsonResponse::success(&data)`を`serde_json::to_string_pretty`で文字列化してstdoutへ。
     - Text: `format!("{data}")`で文字列化してstdoutへ。
   - すべて`write_ignoring_broken_pipe`で書き込み。
   - `ExitCode::Success`を返す。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| data | T: Serialize + Display | 出力対象 |

4. 戻り値

| 値 | 型 | 説明 |
|----|----|------|
| code | io::Result<ExitCode> | 成功時Success、I/Oエラー時Err（BrokenPipeはOk扱い） |

5. 使用例

```rust
let code = out.success("OK")?;
assert_eq!(code, ExitCode::Success);
```

6. エッジケース
- BrokenPipe時はOk返却。
- JSONシリアライズに失敗すると本来Errだが、現実装は`io::Result`への変換が未定義のため要対応（後述）。

根拠（関数名:行番号不明）

### OutputManager::item

1. 目的と責務
   - `Option<T>`を**Some**なら`success`、**None**なら`not_found`として出力。

2. アルゴリズム
   - `match item { Some(d) => success(d), None => not_found(entity, name) }`

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| item | Option<T: Serialize + Display> | 要素 |
| entity | &str | 対象エンティティ名 |
| name | &str | 検索キー名 |

4. 戻り値

| 値 | 型 | 説明 |
|----|----|------|
| code | io::Result<ExitCode> | Success/NotFound |

5. 使用例

```rust
let code = out.item(Some(42), "symbol", "main")?;
```

6. エッジケース
- NoneでNotFound（Textはstderr、JSONはstdoutへ構造化）。

根拠（関数名:行番号不明）

### OutputManager::not_found

1. 目的と責務
   - 未検出結果の出力。**ExitCode::NotFound**。

2. アルゴリズム
   - JSON: `JsonResponse::not_found(entity, name)`を整形してstdoutへ。
   - Text: `"entity 'name' not found"`をstderrへ。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| entity | &str | 対象エンティティ |
| name | &str | キー |

4. 戻り値

| 値 | 型 | 説明 |
|----|----|------|
| code | io::Result<ExitCode> | NotFound（3） |

5. 使用例

```rust
let code = out.not_found("Symbol", "foo")?;
assert_eq!(code, ExitCode::NotFound);
```

6. エッジケース
- BrokenPipeでもNotFoundを返す。

根拠（関数名:行番号不明）

### OutputManager::collection

1. 目的と責務
   - コレクションの一括出力。空なら`not_found`、非空ならSuccess。

2. アルゴリズム
   - `items.into_iter().collect::<Vec<_>>()`で一度収集（空判定と件数取得のため）。
   - JSON: `JsonResponse::success(&items)`→整形→stdout。
   - Text: 件数ヘッダ→罫線→各要素`Display`→stdout。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| items | I: IntoIterator<Item = T> | 要素列 |
| entity_name | &str | エンティティ表示名 |

4. 戻り値

| 値 | 型 | 説明 |
|----|----|------|
| code | io::Result<ExitCode> | Success/NotFound |

5. 使用例

```rust
let v = vec![1,2,3];
let code = out.collection(v, "numbers")?;
```

6. エッジケース
- 空コレクションでNotFound。
- 要素の`Display`が高コスト/長い場合、出力コスト増。

根拠（関数名:行番号不明）

### OutputManager::error

1. 目的と責務
   - エラー内容と**回復提案**を出力。対応する**ExitCode**へ変換。

2. アルゴリズム
   - JSON: `JsonResponse::from_error(error)`→整形→stderr。
   - Text: `"Error: {error}"`→stderr、各`Suggestion`を行ごとに出力。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| error | &IndexError | エラー |

4. 戻り値

| 値 | 型 | 説明 |
|----|----|------|
| code | io::Result<ExitCode> | `ExitCode::from_error(error)` |

5. 使用例

```rust
let code = out.error(&err)?;
```

6. エッジケース
- 提案件数が多いとテキスト量が増える。

根拠（関数名:行番号不明）

### OutputManager::progress

1. 目的と責務
   - **Textのみ**で進捗を`stderr`へ出力。JSONでは抑止。

2. アルゴリズム
   - `matches!(self.format, OutputFormat::Text)`で分岐し、`stderr`へ書込。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| message | &str | 進捗メッセージ |

4. 戻り値

| 値 | 型 | 説明 |
|----|----|------|
| result | io::Result<()> | 書き込み結果 |

5. 使用例

```rust
out.progress("Processing...")?;
```

6. エッジケース
- JSONモードでは何もしない。

根拠（関数名:行番号不明）

### OutputManager::info

1. 目的と責務
   - **Textのみ**で情報メッセージを`stdout`へ出力。

2. アルゴリズム
   - Textモードのみ`stdout`へ書込。

3. 引数/戻り値
- `progress`と同様。

5. 使用例

```rust
out.info("Done.")?;
```

6. エッジケース
- JSONモードでは抑止。

根拠（関数名:行番号不明）

### OutputManager::symbol_contexts

1. 目的と責務
   - `SymbolContext`のコレクションを統一フォーマットで出力。

2. アルゴリズム
   - Vecに収集→空なら`not_found`。
   - JSON: `JsonResponse::success(&contexts)`→整形→stdout。
   - Text: ヘッダ/罫線/各要素表示。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| contexts | impl IntoIterator<Item = SymbolContext> | コンテキスト列 |
| entity_name | &str | エンティティ名 |

4. 戻り値

| 値 | 型 | 説明 |
|----|----|------|
| code | io::Result<ExitCode> | Success/NotFound |

5. 使用例

```rust
let code = out.symbol_contexts(contexts, "functions")?;
```

6. エッジケース
- コレクションが大きい場合、収集のメモリコスト増。

根拠（関数名:行番号不明）

### OutputManager::unified

1. 目的と責務
   - `UnifiedOutput<'_, T>`を**JSON**ではそのまま構造化で直列化、**Text**では`OutputData`の内容に応じて特別扱い（Empty/Items空）または`Display`で出力。`guidance`があれば`stderr`にも出力。

2. アルゴリズム（主な分岐）
   - `format`分岐：
     - JSON: `serde_json::to_string_pretty(&output)`→stdout。
     - Text: `match output.data`でさらに分岐
       - `OutputData::Empty`→entity_typeのDebugをlowercaseして「not found」をstderr。
       - `OutputData::Items{items}`が空→同上。
       - その他→`Display`実装をstdoutへ。
     - `guidance`が`Some`なら空行＋ガイダンスをstderrへ追記。
   - `Ok(output.exit_code)`で終了。

3. 引数

| 引数 | 型 | 説明 |
|------|----|------|
| output | UnifiedOutput<'_, T: Serialize + Display> | 統一出力構造 |

4. 戻り値

| 値 | 型 | 説明 |
|----|----|------|
| code | io::Result<ExitCode> | `output.exit_code`を返却 |

5. 使用例

```rust
use crate::io::schema::{UnifiedOutput, OutputData};
let u = UnifiedOutput {
    entity_type: /* 省略 */,
    data: OutputData::Empty,
    guidance: Some("Try a different filter"),
    exit_code: ExitCode::NotFound,
};
let code = out.unified(u)?;
```

6. エッジケース
- `OutputData::Empty`や`Items`空で「not found」をstderrに出す設計。
- `entity_type`のDebug表示をlowercaseするため、期待文字列が想定と異なる可能性あり。

根拠（関数名:行番号不明）

## Walkthrough & Data Flow

- 入力（成功/失敗/コレクション/統合構造）に応じて、**OutputFormat**（Text/Json）で出力経路が決定されます。
- 実出力は全て`write_ignoring_broken_pipe(&mut dyn Write, &str)`経由で行われ、**BrokenPipe**のみを黙って無視し、その他のI/OエラーはErrで返します。
- Textモードでは、
  - 成功/コレクションは`stdout`へ、
  - `not_found`や`progress`、`unified`の`guidance`は`stderr`へ。
- JSONモードでは、
  - 成功/未検出/統合構造は構造化JSONを**整形**で出力し、**stdout**が基本。
  - 進捗メッセージは抑止し、構造化出力の汚染を避けます。

### Mermaid Flowchart（unified関数の主要分岐）

```mermaid
flowchart TD
    A[unified(output)] --> B{format == Json?}
    B -- Yes --> C[serde_json::to_string_pretty(output)]
    C --> D[stdoutへwrite_ignoring_broken_pipe]
    B -- No (Text) --> E{match output.data}
    E -- Empty --> F[stderrへ "<entity_type> not found"]
    E -- Items empty --> F
    E -- Other --> G[stdoutへ Display(output)]
    G --> H{guidance is Some?}
    F --> H
    D --> I[return output.exit_code]
    H -- Yes --> J[stderrへ空行+guidance]
    H -- No --> I
    J --> I
```

上記の図は`unified`関数の主要分岐を示す（行番号はこのチャンクでは不明）。

## Complexity & Performance

- success
  - 時間: Text/JSONともに出力文字列長に比例（O(n)）
  - 空間: 中間文字列生成に O(n)
  - メモ: `format!`で一旦文字列を作ってから書くため、直接`writeln!`するより一時割り当てが発生
- item
  - 時間/空間: 成功/未検出分岐のみ、O(1)〜O(n)
- not_found
  - 時間/空間: O(1)〜O(m)（文字列長）
- collection / symbol_contexts
  - 時間: 収集＋出力でO(n)
  - 空間: `Vec`収集でO(n)（空判定と件数表示のため）
- error
  - 時間: O(k + n)（提案数＋文字列長）
  - 空間: O(n)
- progress / info
  - 時間: O(m)（メッセージ長）
  - 空間: O(1)

ボトルネック/スケール限界:
- 大量要素の**収集**（Vec化）がメモリと時間を消費。
- **JSON整形**（pretty）は出力が巨大になるとコスト増。
- Textモードの`format!`による一時文字列生成が多い。

実運用負荷要因:
- I/O速度（stdout/stderrへの書き込み）。
- パイプ先の早期終了（BrokenPipe）の頻度が高い場面では多数の書き込みが「無視される」ため、処理時間は減るが出力は欠落。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| BrokenPipe（stdout） | head/grepで早期終了 | エラーを無視しExitCodeを返す | write_ignoring_broken_pipe | 対応済 |
| BrokenPipe（stderr） | 同上 | エラーを無視 | write_ignoring_broken_pipe | 対応済 |
| 非BrokenPipe I/Oエラー | PermissionDenied等 | Errで伝播 | write_ignoring_broken_pipeでErr返却 | 対応済 |
| JSON整形のエラー伝播 | 不正なシリアライズ | Errで伝播（型整合） | `to_string_pretty`で`?`（io::Result内） | 問題あり（型不整合） |
| 空コレクション | `[]` | ExitCode::NotFound | collection / symbol_contexts | 対応済 |
| unifiedのEmpty | OutputData::Empty | 「not found」をstderr、exit_code返却 | unified | 対応済 |
| unified Items空 | OutputData::Items{items: []} | 同上 | unified | 対応済 |
| JSONモードでprogress/info | 文字列 | 出力抑止（汚染防止） | progress/infoの分岐 | 対応済 |

セキュリティチェックリスト:
- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: 該当なし（安全なRust、unsafeなし）
- インジェクション
  - SQL/Command/Path traversal: 該当なし（出力のみ）
- 認証・認可
  - 権限チェック漏れ/セッション固定: 該当なし
- 秘密情報
  - Hard-coded secrets: 該当なし
  - Log leakage: `error`と`suggestions`をそのまま出力するため、環境次第で詳細情報が露出し得るが一般的なCLI出力の範疇
- 並行性
  - Race condition / Deadlock: 該当なし（単一スレッド前提）

重要なバグ詳細（根拠: 関数名:行番号不明）:
- `serde_json::to_string_pretty`は`Result<String, serde_json::Error>`を返すが、各関数の戻り型は`io::Result<ExitCode>`。`?`を使うには`From<serde_json::Error> for io::Error`が必要だが標準では未提供。現状このままでは**コンパイル不可**の可能性が高い。対処案は後述。

## Design & Architecture Suggestions

- **エラー型の統一**:
  - `Result<ExitCode, OutputError>`のような専用エラー型を設け、`From<serde_json::Error>`や`From<io::Error>`を実装。
  - もしくは`serde_json::to_string_pretty(...).map_err(|e| io::Error::new(io::ErrorKind::Other, e.to_string()))?`のように明示変換する。
- **文字列一時生成の削減**:
  - Textモードで`format!("{data}")`せず、`writeln!(stdout, "{}", data)`を直接使用して割り当てを抑制。
- **出力ポリシーの明確化**:
  - JSONモードでの`not_found`をstdoutに出す現行設計は妥当だが、Textモードの`not_found`はstderr。仕様書に明記して利用側に周知。
- **Entity名の扱い改善**:
  - `unified`で`entity_type`のDebug→lowercaseは不安定。明示的な`Display`/`AsRef<str>`を用意し、人間可読な正規化済み文字列を使用。
- **Large collections対応**:
  - `collection`/`symbol_contexts`の`collect()`はメモリコスト増。必要なら**ストリーミング出力**（件数を事前に計測可能なら2パス、不可ならヘッダに「count unknown」を採用）を検討。

## Testing Strategy (Unit/Integration) with Examples

- 既存テストはBrokenPipeに対して十分。追加すべきテスト：
  - 非BrokenPipeエラーの伝播確認
    ```rust
    struct FailingWriter;
    impl std::io::Write for FailingWriter {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::PermissionDenied, "nope"))
        }
        fn flush(&mut self) -> io::Result<()> { Ok(()) }
    }

    #[test]
    fn test_non_broken_pipe_error_propagates() {
        let mut out = OutputManager::new_with_writers(
            OutputFormat::Text, Box::new(FailingWriter), Box::new(Vec::new())
        );
        let err = out.success("data").err().unwrap();
        assert_eq!(err.kind(), io::ErrorKind::PermissionDenied);
    }
    ```
  - `unified`のguidance出力（stderr＋空行）確認
    ```rust
    use crate::io::schema::{UnifiedOutput, OutputData};
    // 簡易のVec<u8>をstderrに差し込んで内容に空行＋guidanceが含まれることを確認
    ```
  - JSON整形エラーの型変換（修正後）確認
    ```rust
    // serde_json::Errorをio::Errorに変換するラッパが正しく機能するか
    ```
  - Textモードの`not_found`がstderr、JSONモードがstdoutであることの確認
  - `collection`/`symbol_contexts`の空/非空パスをTextでも検証

- 使用例（成功と未検出）
```rust
let mut out = OutputManager::new(OutputFormat::Text);
out.info("Starting")?;
let code = out.item::<i32>(None, "Symbol", "main")?;
assert_eq!(code, ExitCode::NotFound);
let code = out.success(123)?;
assert_eq!(code, ExitCode::Success);
```

## Refactoring Plan & Best Practices

- **ステップ1**: JSONエラー変換の導入
  - 小規模: `map_err`で都度変換。
  - 大規模: `OutputError`を導入し、`Result<ExitCode, OutputError>`統一。
- **ステップ2**: `writeln!`による直接出力に移行
  - `format!("{data}")`や`format!("{item}")`を削減。
- **ステップ3**: `unified`のエンティティ名処理を型安全に
  - Debug依存を排し、`entity_type`に人間可読な`Display`を要求。
- **ステップ4**: 出力ポリシーのコメント強化
  - Text/JSONでのstdout/stderrの使い分けをドキュメント化。
- **ステップ5**: 大量データの扱い改善（必要に応じて）
  - ストリーミング出力、ヘッダ出力の工夫。

ベストプラクティス:
- **BrokenPipeは正常**という設計意図を明示し、整合性のあるExitCodeを維持。
- JSONモードの補助メッセージ（progress/info/guidance）は**stderr**へ分離し、**stdout**は構造化専用に。
- 汎用ユーティリティ`write_ignoring_broken_pipe`の再利用で方針の一貫性を担保。

## Observability (Logging, Metrics, Tracing)

- 現状、独自のロギング/メトリクス/トレースはなし。
- 提案：
  - **メトリクス**: 出力行数、BrokenPipe発生回数、NotFound件数、JSON/Textの割合。
  - **トレース**: `tracing`クレートで「開始/終了」「ExitCode」「フォーマット」「要素数」などのspanを付与（Textモードではstderr汚染に注意）。
  - **ログ**: 開発環境ではdebugログを有効化し、運用では抑制。

## Risks & Unknowns

- **不明**: `OutputFormat`が**Copy**かどうか（`match self.format`の使用に照らすとCopy前提に見えるが、このチャンクでは未定）。非Copyなら所有権移動による使用制限が生じうる。
- **不明**: `JsonResponse`/`UnifiedOutput`/`OutputData`の詳細構造と`Display`実装の仕様。
- **不明**: `ExitCode::from_error`の具体的マッピング。
- **リスク**: JSON整形エラーの型変換未定義（コンパイル不可の可能性）。
- **リスク**: 大量データ時の`collect()`によるメモリ使用増。
- **リスク**: `entity_type`のDebug→lowercaseが意図しない文言になる可能性。

## Complexity & Performance

- Big-O（時間/空間）を各APIに明示済み。
- 主なボトルネック：`collect()`によるベクタ化、JSON整形、文字列割り当て。
- スケール限界：大量要素、巨大JSON。
- 実運用負荷要因：I/O速度、パイプ先の早期終了頻度、`stderr`運用ポリシー。

## Edge Cases, Bugs, and Security

- 詳細は上表とセキュリティチェックリスト参照。
- 重点バグ: **serde_json::Error → io::Error**変換不足。`?`が使えないため、**コンパイルに失敗する可能性**。修正が最優先。

## Design & Architecture Suggestions

- 上述の**エラー統合/直接writeln/Entity表記改善**を推奨。
- API利用側が`stdout`/`stderr`の方針を理解できるよう、ドキュメントを強化。

## Testing Strategy (Unit/Integration) with Examples

- 既存テストに加え、**非BrokenPipeエラー伝播**、**unifiedガイダンス**、**JSON整形エラー変換**のテストを追加。

## Refactoring Plan & Best Practices

- 段階的リファクタリング案（エラー型→直接書き込み→表示改善→ドキュメント→大規模データ対応）を提示。

## Observability (Logging, Metrics, Tracing)

- メトリクスとトレースの導入提案。

## Risks & Unknowns

- `OutputFormat`のCopy特性や`UnifiedOutput`の詳細がこのチャンクには現れない。エラー変換が最優先のリスク。