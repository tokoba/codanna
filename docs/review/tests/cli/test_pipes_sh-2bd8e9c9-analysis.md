# test_pipes.sh Review

## TL;DR

- 目的: Codanna CLIのスラッシュコマンド系サブコマンドがパイプで連結可能かを検証し、JSON妥当性・エラーコード・性能を確認する
- 公開API: スクリプト自体の公開APIはなし。内部関数は1つ（test_json_output）。外部APIとしてCodanna CLIのサブコマンド群（retrieve symbol/callers/calls/describe/search, mcp semantic_search_docs）を使用
- 複雑箇所: jqとxargs・sh -cを絡めた多段パイプ（文字列置換とシェル解釈の相互作用）が**入力の特殊文字で壊れる可能性**
- 重大リスク: xargs -I {} と sh -c の併用による**コマンドインジェクション/誤解釈リスク**、計測の**perl依存**による移植性課題、**JSONスキーマ変化**に対する脆弱性
- エラー/並行性: スクリプトは**逐次処理**で並行性なし。エラーは明示的にチェックするが、**set -euo pipefail未使用**で一部取りこぼし可能
- パフォーマンス: 応答時間をミリ秒で測定（単発<300ms、チェーン<1sが目標）。I/Oとプロセス起動がボトルネック

## Overview & Purpose

このbashスクリプトは、Codanna CLI（Rust製バイナリと推測）のスラッシュコマンド群が、UNIXパイプライン（jq, xargsなど）で**機械的に扱えるJSON**を出力すること、**チェーン可能**であること、**適切なエラーコード（0=success, 3=not_found）**を返すこと、そして**性能目標（単発<300ms、チェーン<1s）**を満たすことを検証します。対象となるCLIサブコマンドのデータ契約（JSONフィールド）に依存したテストも含みます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Script | test_pipes.sh | private (ファイル内) | 全体のテストオーケストレーション（存在確認、JSON妥当性、パイプ動作、エラー、性能） | Med |
| Function | test_json_output | private (ファイル内) | 与えたCodannaサブコマンドのJSON妥当性チェック | Low |
| Variable | CODANNA | private | バイナリパス指定（./target/release/codanna） | Low |
| Variable | GREEN/RED/YELLOW/NC | private | カラー表示用ANSIコード | Low |

### Dependencies & Interactions

- 内部依存
  - メインフローが**test_json_output**関数を複数回呼出し
  - 変数CODANNAを各コマンド呼び出しで使用
  - 文字装飾変数をecho出力に使用

- 外部依存（コマンド/ツール）

| ツール | 用途 | 備考 |
|-------|------|------|
| codanna (Rust CLI) | サブコマンド実行とJSON出力 | ./target/release/codanna の存在必須 |
| jq | JSON整形/抽出/妥当性検査 | `command -v jq` で存在確認 |
| xargs | 値を引数に展開して連鎖実行 | `-I {}` 置換を使用 |
| sh -c | 複合コマンドの実行 | xargsと併用（置換後にシェル解釈） |
| perl (Time::HiRes) | ミリ秒タイマ取得 | 可搬性課題あり |
| head | 先頭2件抽出 | callsの追跡で使用 |
| sed | 出力整形（インデント） | 表示用 |
| echo/printf/command | 標準的シェルユーティリティ | |

- 被依存推定
  - 開発者のローカル検証
  - CIステップでの非回帰テスト（CLIのJSON契約/性能）
  - ドキュメント・サンプルとしての利用

## API Surface (Public/Exported) and Data Contracts

このスクリプト自体の公開APIはありません（引数やエクスポート関数なし）。内部関数と外部CLIの使用APIを整理します。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| test_json_output | test_json_output "<cmd>" "<desc>" | 指定コマンドのJSON妥当性チェック | O(1)（外部プロセス1回） | O(1) |
| codanna retrieve symbol | codanna retrieve symbol <name> --json | シンボル取得。items[0].symbol.name参照 | O(1)（DB/索引依存） | O(1) |
| codanna retrieve callers | codanna retrieve callers <name> --json | 呼び出し元計数。data.count参照 | O(1) | O(1) |
| codanna retrieve calls | codanna retrieve calls <name> --json | 呼び出し先列挙。items[].symbol.name参照 | O(1) | O(1) |
| codanna retrieve describe | codanna retrieve describe <symbol> --json | シンボル説明 | O(1) | O(1) |
| codanna retrieve search | codanna retrieve search <term> --json | 検索 | O(1) | O(1) |
| codanna mcp semantic_search_docs | codanna mcp semantic_search_docs query:<q> limit:<n> --json | 文章のセマンティック検索 | O(1) | O(1) |

詳細（このチャンクの観測に基づき記述。行番号はこのチャンクには現れないため不明）:

1) test_json_output
- 目的と責務
  - **指定したサブコマンド**が**有効なJSON**を出力するか検証し、✓/✗を表示する
- アルゴリズム
  - codannaを実行し標準エラーを捨てる
  - 出力をjq '.'へパイプし検証
  - 成功なら✓、失敗なら✗
- 引数

| 名称 | 型 | 必須 | 説明 |
|-----|----|-----|------|
| cmd | string | 必須 | 実行するcodannaのサブコマンド文字列 |
| desc | string | 必須 | 表示用説明文 |

- 戻り値

| 型 | 説明 |
|----|------|
| 0 | JSON妥当性OK |
| 1 | JSON妥当性NG |

- 使用例
```bash
test_json_output "retrieve symbol main" "retrieve symbol"
```

- エッジケース
  - codannaが存在しない場合（事前チェックあり）
  - jqが存在しない場合（事前チェックあり）
  - コマンドが非JSONを出力する場合（✗）

2) codanna retrieve symbol <name> --json
- 目的と責務
  - **任意シンボル**のメタデータ取得
- アルゴリズム
  - Rust CLI内部のインデックス/DBから検索（詳細不明）
- 引数

| 名称 | 型 | 必須 | 説明 |
|-----|----|-----|------|
| name | string | 必須 | シンボル名（例：main） |
| --json | flag | 必須 | JSONフォーマットでの出力 |

- 戻り値（期待されるデータ契約）
  - .data.items[0].symbol.name（シンボル名）
  - .status（not_foundなどを返す可能性）
  - これらフィールドはスクリプトが参照（jq -r '.data.items[0].symbol.name'）
- 使用例
```bash
$CODANNA retrieve symbol main --json | jq -r '.data.items[0].symbol.name'
```
- エッジケース
  - itemsが空、またはフィールド不在 → jq抽出が空文字になる

3) codanna retrieve callers <name> --json
- 責務
  - **呼び出し元**の集計
- 戻り値契約
  - .data.count（整数）
- 使用例
```bash
xargs -I {} $CODANNA retrieve callers {} --json | jq '.data.count'
```

4) codanna retrieve calls <name> --json
- 責務
  - **呼び出し先**の列挙
- 戻り値契約
  - .data.items[].symbol.name
- 使用例
```bash
$CODANNA retrieve calls main --json | jq -r '.data.items[:2].symbol.name // empty'
```

5) codanna retrieve describe <symbol> --json
- 責務
  - **シンボルの説明**取得
- 戻り値契約
  - 不明（このチャンクには現れない詳細）

6) codanna retrieve search <term> --json
- 責務
  - **テキスト検索**による一致シンボル/要素取得
- 戻り値契約
  - 不明（このチャンクには現れない詳細）

7) codanna mcp semantic_search_docs query:<q> limit:<n> --json
- 責務
  - **セマンティック検索**（Claude向けMCP連携を示唆）
- 戻り値契約
  - 不明（このチャンクには現れない詳細）

## Walkthrough & Data Flow

全体の流れ（高レベル）:
- 前提チェック
  - codannaバイナリ存在確認
  - jqの存在確認
- JSON妥当性テスト（test_json_outputを複数呼出）
- パイプラインテスト
  - symbol → name抽出 → callers → count
  - calls → 上位2件 → 各々のcalls数
- エラー検証
  - 存在しないシンボルのstatus
  - exitコード（0=success, 3=not_found）
- 性能計測
  - 単発コマンドのms
  - チェーンのms
- サマリー表示

Mermaid（パイプチェーンの流れ）:
```mermaid
flowchart LR
  A[retrieve symbol main --json] --> B{jq -r .data.items[0].symbol.name}
  B -->|name| C[xargs -I {} codanna retrieve callers {} --json]
  C --> D{jq .data.count}
  D --> E[count表示]

  subgraph Legend
    direction LR
    A[Codanna CLI]:::cli
    B[jq抽出]:::jq
    C[xargsでコマンド展開]:::xargs
    D[jq抽出]:::jq
  end

classDef cli fill:#cde,stroke:#000;
classDef jq fill:#efe,stroke:#000;
classDef xargs fill:#fec,stroke:#000;
```
上記の図は`「チェーンパイプ（symbol -> callers）」`の主要フローを示す（行番号不明。このチャンクでは行番号情報なし）。

Mermaid（マルチレベルトレースの流れ）:
```mermaid
flowchart TD
  A[retrieve calls main --json] --> B{jq -r .data.items[:2].symbol.name // empty}
  B --> C[head -2]
  C --> D{xargs -I {} sh -c "echo '{}:' && codanna retrieve calls {} --json | jq '.data.count // 0'"}
  D --> E[名前とcalls数のペア出力]
```
上記の図は`「マルチレベルトレース（calls -> calls）」`の主要分岐を示す（行番号不明）。

## Complexity & Performance

- 計算量
  - 各テストは外部プロセス呼び出しの固定回数で構成 → **時間: O(1)、空間: O(1)**（入力サイズに依存しない）
- ボトルネック
  - **プロセス起動オーバーヘッド**（codanna, jq, xargs, shなど）
  - codanna内部（Rust側）のI/O・インデックス照会
- スケール限界
  - itemsの数やcalls深さが増えると、パイプライン内の**繰り返し呼び出し**で遅延増
- 実運用負荷要因
  - ディスク/ネットワーク/DB（Rust CLI内部の実装次第。詳細はこのチャンクには現れない）
- 計測手法
  - perl Time::HiResで**ミリ秒**測定。移植性課題あり（perl未インストール環境）

## Edge Cases, Bugs, and Security

セキュリティチェックリストの観点で評価:

- メモリ安全性: bash側は該当なし。Rust側の安全性は不明（このチャンクには現れない）
- インジェクション:
  - xargs -I {} と sh -c の併用箇所で、**特殊文字/引用符**を含むシンボル名が**シェルで解釈**される可能性
    - 該当コード例:
    ```bash
    xargs -I {} sh -c "echo '{}:' && $CODANNA retrieve calls {} --json | jq '.data.count // 0'"
    ```
    - 改善案: 置換値を**シェルに渡さない**（sh -cを避ける）、もしくは**printf %q**や**null区切り（-0）**で厳密に引数化
  - xargs単体の置換（retrieve callers）では`-I {}`により1行単位で置換されるが、**シェルメタ文字**はCLIに安全に渡るとは限らないため**クォート厳格化**が望ましい
- 認証・認可: 該当なし（CLI側の権限設計は不明）
- 秘密情報: ハードコードされた秘密はなし。ログ漏洩の危険性は低いが、**trace出力に機密名**が混入する可能性には注意
- 並行性: スクリプトは逐次的。**レース/デッドロック**の懸念なし

詳細なエッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| codanna不在 | CODANNAのパスが存在しない | エラーメッセージを出してexit 1 | ファイル存在チェックあり | 対応済み |
| jq不在 | jqがPATHにない | エラーメッセージを出してexit 1 | command -vで検出 | 対応済み |
| 非JSON出力 | CLIが誤ってテキスト出力 | ✗を表示しreturn 1 | jq '.'妥当性チェック | 対応済み |
| 空items | .data.itemsが空 | name抽出が空→警告/✗ | nameが空だと"Found:"なし | 部分対応 |
| 特殊文字シンボル | シンボル名に`$()&;|`など | シェル解釈で誤動作/インジェクション | xargs+sh -cで危険 | 要修正 |
| exitコード不一致 | not_foundが3以外 | ✗で異常表示 | 0と3を想定 | 要検証（CLI側変更時） |
| perl不在 | Time::HiRes未導入 | 計測失敗/エラー | perl依存 | 要代替手段 |

## Design & Architecture Suggestions

- **シェル厳格化**: `set -euo pipefail` を冒頭に追加し、未定義変数やパイプ失敗を検出
- **クォート/安全な引数渡し**:
  - `xargs -0` + `printf '%s\0'`で**null区切り**を利用
  - `sh -c`を避け、**直接exec**で配列引数渡し（bash関数化やwhile read -rで安全に引数化）
- **可搬性改善**:
  - perl依存の時刻計測を代替（例: `python -c 'import time; print(int(time.time()*1000))'`）
- **構造化テスト**:
  - BATSなどの**シェルテストフレームワーク**を採用し、アサーションとレポートを明確化
- **引数化**:
  - CODANNAパスや性能閾値（300ms/1000ms）を**環境変数/引数**で上書き可能に
- **JSON契約バリデーション**:
  - 期待スキーマの**明示**（必須フィールド、有無チェック）と**詳細なエラーメッセージ**

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト（関数レベル）
  - test_json_outputの挙動（成功/失敗パス）を**モック化**したjq出力で検証（codanna呼び出しを差し替え）
- 統合テスト（CLI連携）
  - 実CLIでの**正例/誤例**（存在しないシンボル、特殊文字を含むシンボル）を網羅
- パフォーマンステスト
  - 反復実行し**平均/95%ile**を記録、閾値に対するパス/フェイル判定

BATS例（提案）:
```bash
#!/usr/bin/env bats

setup() {
  CODANNA="${CODANNA:-./target/release/codanna}"
  command -v jq >/dev/null
}

@test "retrieve symbol returns JSON" {
  run bash -c "$CODANNA retrieve symbol main --json | jq '.' >/dev/null"
  [ "$status" -eq 0 ]
}

@test "nonexistent symbol returns not_found status and exit 3" {
  run "$CODANNA" retrieve symbol nonexistent_xyz --json
  [ "$status" -eq 3 ]
  echo "$output" | jq -r '.status' | grep -qx 'not_found'
}
```

特殊文字安全テスト（改善後の例）:
```bash
# 改善例: while read -r で1行安全入力
$CODANNA retrieve calls main --json | jq -r '.data.items[:2].symbol.name // empty' | \
  head -2 | while IFS= read -r name; do
    printf '%s:\n' "$name"
    "$CODANNA" retrieve calls "$name" --json | jq '.data.count // 0'
  done
```

## Refactoring Plan & Best Practices

- **Strict mode**: `set -euo pipefail` 導入
- **関数分割**:
  - `check_prereqs`, `measure_ms <cmd>`, `pipe_symbol_to_callers`, `trace_calls_depth2` といった関数化で再利用性を向上
- **安全な引数取り扱い**:
  - `while read -r` と**ダブルクォート**で全引数を保護
- **ロギング整備**:
  - `printf`使用（`echo -e`より移植性が高く安全）
- **ツール依存の整理**:
  - perlの代替（python, date）と**存在チェック**追加
- **静的解析**:
  - `shellcheck`の導入で**未定義変数/未クォート**検出

## Observability (Logging, Metrics, Tracing)

- ログ
  - 現状カラー記号と短文。**失敗時の詳細原因（実コマンド, stderr要約）**を併記すると診断迅速化
- メトリクス
  - 計測値（ms）を**CSV/JSON**で保存して履歴比較できるようにする
- トレーシング
  - 各ステップの開始/終了時刻を**タグ付き**で記録し、どのプロセスが遅いかを特定可能に

## Risks & Unknowns

- JSONデータ契約の詳細は**CLI実装依存**（このチャンクには現れない）
  - `.data.items[0].symbol.name`, `.data.count`, `.status`の**存在と型**が変更されるとテスト崩壊
- CLIのexitコード仕様（3=not_found）が**将来変更**される可能性
- perlの可用性やバージョン差による**移植性**問題
- `mcp semantic_search_docs` の**外部依存/ネットワーク**有無が不明（このチャンクには現れない）
- 並行性/スレッド安全性（Rust側）は**不明**。このスクリプトは逐次実行で同時性を検証しない

以上を踏まえ、**入力の安全性強化（クォート/シェル解釈排除）**と**計測・ログの可搬性向上**を優先することで、堅牢で再現性の高い検証スクリプトになります。