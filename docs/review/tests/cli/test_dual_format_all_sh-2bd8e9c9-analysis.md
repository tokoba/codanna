# cli/test_dual_format_all.sh Review

## TL;DR

- このスクリプトの目的は、外部CLIバイナリである**codanna**の**retrieve**サブコマンドに対し、従来のフラグ形式と**key:value形式**の両方が動作するかを包括的に検証すること。
- 主要な「API」に相当するのはスクリプト内関数**test_command**で、各コマンドの2形式を試験し、成功、未検出（**exit code 3**）、失敗を判定してレポートする。
- 複雑な箇所は「検索パラメータの混在（フラグとkey:valueの併用）」「優先度（フラグがkey:valueを上書きするか）」「エラーメッセージの検証」「パフォーマンス計測の扱い」。
- 重大リスクは、外部依存（./target/release/codanna、jq、grep、time）が未インストール／非互換の場合に試験が誤判定する点と、**エラーメッセージ文字列**への依存による脆さ。
- セキュリティ観点では、引数展開の未引用による**コマンドインジェクション**可能性は低いものの、将来的に外部入力化すると危険。現状は内部固定値のみでリスク低。
- **Rustのメモリ安全性/エラー/並行性**評価はこのチャンクでは不明（外部バイナリの実装詳細はこのファイルに現れない）。

## Overview & Purpose

このbashスクリプトは、codannaバイナリの**retrieve**系コマンド群（symbol, calls, callers, implementations, describe, search）について、2種類の入力形式（従来のフラグ形式と**key:value形式**）を横断的にテストします。さらに、下記を検証します。

- エラーハンドリング（必須引数欠如時のエラーメッセージ）
- フラグの優先度（例：--limit と limit: の競合時）
- パフォーマンス（検索3件の処理時間が300ms未満かの目視確認）

このスクリプト自体は**統合テスト**であり、codannaのCLI出力（JSON）や**終了コード（0=成功、3=未検出、1=引数不備想定）**という契約に依存してテスト判定を行います。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Variable | BINARY | script-global | テスト対象バイナリのパス（./target/release/codanna） | Low |
| Variable | GREEN/RED/NC | script-global | カラー出力用エスケープシーケンス | Low |
| Function | test_command | script-global | 指定コマンド名について従来形式とkey:value形式を実行し、結果表示 | Low |
| Script Block | SYMBOL/CALLS/CALLERS/IMPLEMENTATIONS/DESCRIBE 試験 | script-global | 各retrieveサブコマンドを2形式で検証 | Low |
| Script Block | SEARCH 試験（複数パラメータ/混在） | script-global | 従来/key:value/混在の3バリエーションを試験 | Low |
| Script Block | ERROR HANDLING 試験 | script-global | 必須引数欠如時のエラー文言チェック | Low |
| Script Block | PRECEDENCE 試験 | script-global | --limit が limit: を上書きするかの検証 | Med |
| Script Block | PERFORMANCE 試験 | script-global | 実行時間の目視確認（<300ms想定） | Low |

### Dependencies & Interactions

- 内部依存
  - test_command は BINARY 変数とカラーコードに依存し、標準出力/標準エラーのリダイレクトにより成功・失敗を判定します。
  - PRECEDENCE 試験は `jq` に依存してJSONから `.count` を抽出します。
  - ERROR HANDLING 試験は `grep -q` に依存してエラーメッセージの有無を確認します。

- 外部依存（このスクリプトが使う外部コマンド/ツール）
  | 依存名 | 種別 | 用途 | 必須性 | 備考 |
  |--------|------|------|--------|------|
  | ./target/release/codanna | 外部バイナリ | テスト対象CLI | 必須 | パスが固定。存在しない場合は全テスト失敗（exit 127 等） |
  | bash | シェル | スクリプト実行 | 必須 | shebangは#!/bin/bash |
  | echo/printf | 組み込みコマンド | 出力整形 | 必須 | echo -e を使用 |
  | grep | コマンド | エラーメッセージ確認 | 任意/推奨 | ERROR HANDLING 試験に必要 |
  | jq | コマンド | JSON値抽出 | 任意/推奨 | PRECEDENCE 試験に必要 |
  | time | 組み込み/外部 | 実行時間計測 | 任意 | bashのtimeはstderr出力。リダイレクト挙動に注意 |

- 被依存推定（このモジュールを使用する可能性のある箇所）
  - CIパイプラインの統合テストステップ
  - 開発者のローカル検証
  - リリース前のサニティチェックスクリプト

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| test_command | test_command cmd_name traditional keyvalue | 2形式（従来/kv）でretrieveを実行し結果表示 | O(1)（外部コマンド時間に依存） | O(1) |
| retrieve symbol（外部） | codanna retrieve symbol <name> --json | シンボル取得（従来形式） | 不明（外部） | 不明 |
| retrieve symbol（外部, kv） | codanna retrieve symbol name:<name> --json | シンボル取得（key:value形式） | 不明（外部） | 不明 |
| retrieve calls（外部） | codanna retrieve calls <function> --json | 呼び出し先取得（従来形式） | 不明（外部） | 不明 |
| retrieve calls（外部, kv） | codanna retrieve calls function:<name> --json | 呼び出し先取得（kv） | 不明（外部） | 不明 |
| retrieve callers（外部） | codanna retrieve callers <function> --json | 呼び出し元取得（従来形式） | 不明（外部） | 不明 |
| retrieve callers（外部, kv） | codanna retrieve callers function:<name> --json | 呼び出し元取得（kv） | 不明（外部） | 不明 |
| retrieve implementations（外部） | codanna retrieve implementations <trait> --json | 実装取得（従来形式） | 不明（外部） | 不明 |
| retrieve implementations（外部, kv） | codanna retrieve implementations trait:<name> --json | 実装取得（kv） | 不明（外部） | 不明 |
| retrieve describe（外部） | codanna retrieve describe <symbol> --json | シンボル説明（従来形式） | 不明（外部） | 不明 |
| retrieve describe（外部, kv） | codanna retrieve describe symbol:<name> --json | シンボル説明（kv） | 不明（外部） | 不明 |
| retrieve search（外部） | codanna retrieve search "<query>" --limit N --kind K --json | 検索（従来形式） | 不明（外部） | 不明 |
| retrieve search（外部, kv） | codanna retrieve search query:<q> limit:<n> kind:<k> --json | 検索（kv形式） | 不明（外部） | 不明 |
| retrieve search（外部, 混在） | codanna retrieve search "<query>" limit:<n> kind:<k> --json | 検索（混在） | 不明（外部） | 不明 |

詳細（このファイル内API: test_command）

1) 目的と責務
- 指定されたコマンド名に対し、従来形式と**key:value形式**の両方を実行し、その成功可否をわかりやすく表示する。
- **exit code 3**を「未検出だが想定内」として扱い、成功扱いの表示を行う。

2) アルゴリズム（ステップ分解）
- traditional 形式で `$BINARY retrieve $traditional` を実行し、成功なら緑チェック。
- 失敗時は `$?` を取得し、3なら緑チェック（未検出扱いの成功）、それ以外なら赤バツ。
- key:value 形式でも同様の判定を行う。
- 各形式の結果を行ごとに出力し、最後に空行を挿入。

3) 引数
| 引数名 | 型 | 必須 | 説明 |
|--------|----|------|------|
| cmd_name | string | 必須 | 表示用のコマンド名（"Symbol"等） |
| traditional | string | 必須 | 従来形式の引数列（例: "symbol main --json"） |
| keyvalue | string | 必須 | key:value形式の引数列（例: "symbol name:main --json"） |

4) 戻り値
| 戻り値 | 型 | 説明 |
|--------|----|------|
| なし | N/A | 標準出力へ結果を表示するのみ |

5) 使用例
```bash
# 従来形式とkv形式を比較
test_command "Symbol" "symbol main --json" "symbol name:main --json"
```

6) エッジケース
- BINARYが存在しない（exit 127等）と常に失敗する
- jqやgrepが未インストールだと後続試験が不安定
- 引数にスペースやシェル特殊文字が含まれると未引用展開が問題化

データ契約（外部CLI, このスクリプトから推定）
- 成功時は**exit code 0**で、--json指定時はJSONを返す。
- 未検出時は**exit code 3**を返し、これを「想定内」扱いする（スクリプト内の表示仕様）。
- 必須引数欠如時は**エラーメッセージ**を標準エラーへ出し、**exit code 1**が期待される（コメントに「should exit with code 1」と記載、ただしこのスクリプトは文字列でのみ検証）。
- searchの結果はJSONに**count**フィールドが存在することを前提とし、優先度検証で `.count == 1` を期待。

注: 上記契約はこのファイルの出現箇所からの推定であり、外部バイナリの実装はこのチャンクには現れないため厳密な保証は不明。

## Walkthrough & Data Flow

- 冒頭でテストスイートのタイトルを表示。
- BINARY変数で対象バイナリ（./target/release/codanna）を指定。
- カラーコード（GREEN/RED/NC）を定義。
- 関数**test_command**を定義し、各コマンドを従来形式とkey:value形式でテスト。
  - 各実行は標準出力/標準エラーを`> /dev/null 2>&1`で抑制。
  - 失敗時には`$?`で**exit code**を取得し、**3**なら「未検出の成功」、他は失敗として表示。

- 個別試験
  - SYMBOL/CALLS/CALLERS/IMPLEMENTATIONS/DESCRIBE：test_commandで2形式を検証。
  - SEARCH（複数パラメータ）：従来形式、kv形式、混在形式の3通りをテスト。
  - ERROR HANDLING：必須引数欠如時の出力を**grep -q**で文字列一致確認。
  - PRECEDENCE：`$BINARY retrieve search "output" limit:10 --limit 1 --json` の結果JSONから `jq -r '.count'` を抽出し、**1**であることを期待。
  - PERFORMANCE：`time $BINARY retrieve search "unified output" limit:3 --json` を実行し、300ms未満の完了を目視確認（閾値強制はしていない）。

データフローの要点
- コマンド実行結果の「成功/失敗」は**exit code**で判定。
- エラーメッセージ検証は標準エラーをgrepで文字列一致（ローカライズや文言変更に弱い）。
- 優先度検証はJSON出力から**count**フィールドを抽出（**jq**に依存）。

### Mermaid（分岐フロー）

```mermaid
flowchart TD
  A[call test_command(cmd_name, traditional, keyvalue)]
  A --> B{Run traditional form}
  B -->|exit 0| C[Print: Traditional ✓]
  B -->|exit != 0| D{Exit code == 3?}
  D -->|Yes| E[Print: Traditional ✓ (not found - exit 3)]
  D -->|No| F[Print: Traditional ✗ (exit X)]

  F --> G{Run key:value form}
  E --> G
  C --> G

  G -->|exit 0| H[Print: Key:value ✓]
  G -->|exit != 0| I{Exit code == 3?}
  I -->|Yes| J[Print: Key:value ✓ (not found - exit 3)]
  I -->|No| K[Print: Key:value ✗ (exit Y)]
```

上記の図は`test_command`関数（L16-L49）の主要分岐を示す。

参考抜粋（関数の要部）
```bash
test_command() {
    local cmd_name=$1
    local traditional=$2
    local keyvalue=$3
    
    echo "Testing $cmd_name:"
    
    # Traditional
    if $BINARY retrieve $traditional > /dev/null 2>&1; then
        echo -e "  Traditional: ${GREEN}✓${NC}"
    else
        EXIT_CODE=$?
        if [ $EXIT_CODE -eq 3 ]; then
            echo -e "  Traditional: ${GREEN}✓${NC} (not found - exit 3)"
        else
            echo -e "  Traditional: ${RED}✗${NC} (exit $EXIT_CODE)"
        fi
    fi

    # Key:value
    if $BINARY retrieve $keyvalue > /dev/null 2>&1; then
        echo -e "  Key:value:   ${GREEN}✓${NC}"
    else
        EXIT_CODE=$?
        if [ $EXIT_CODE -eq 3 ]; then
            echo -e "  Key:value:   ${GREEN}✓${NC} (not found - exit 3)"
        else
            echo -e "  Key:value:   ${RED}✗${NC} (exit $EXIT_CODE)"
        fi
    fi
    echo ""
    # /* ... 省略 ... */
}
```

## Complexity & Performance

- 時間計算量
  - スクリプト自身はテストケース数に線形（O(T)）。このファイルでは概ね18回程度の外部コマンド呼び出し。
  - 実際の時間は各**codanna retrieve**呼び出しの処理時間に支配。スクリプトオーバーヘッドは軽微。
- 空間計算量
  - O(1)。一時変数（COUNT, EXIT_CODE）程度。
- ボトルネック
  - 外部バイナリの処理（I/O、解析、インデックス検索等）。
  - `jq`の起動コスト（微小）。
- スケール限界
  - 大量のテストケース追加時は外部バイナリ起動回数が増加し、CI時間増。並列化は現状実装なし。
- 実運用負荷要因
  - ファイル数/コードベース規模が大きい場合の**search**のレスポンス。
  - ディスクI/O、CPU（パース/索引）、キャッシュの有無。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト
- メモリ安全性: 不明（このチャンクには現れない。対象は外部Rustバイナリ）
- インジェクション:
  - Command Injection: 引数展開が未引用（例: `$BINARY retrieve $traditional`）。現状、固定引数のみでリスク低だが、将来外部入力化で危険。配列で安全に渡すのが望ましい。
  - Path Traversal/SQL Injection: 該当なし（このスクリプト内に該当処理なし）
- 認証・認可: 該当なし（外部バイナリの仕様は不明）
- 秘密情報: ハードコードされた秘密情報なし。ログ漏えいも最小（/dev/nullにリダイレクト）
- 並行性: なし（逐次実行）。Race/Deadlockの懸念なし。

潜在的な不具合・注意点
- 依存チェック不足: `codanna`, `jq`, `grep`, `time` の存在確認がない。
- エラーメッセージ依存: 文字列一致（英語固定）に依存。文言変更やローカライズで誤判定。
- 優先度検証の脆弱性: `.count` フィールド存在を仮定。JSON構造が変わると誤判定。
- `time`のリダイレクト挙動: bashの`time`は通常stderrへ出力。`> /dev/null 2>&1`で抑制できるが、シェル/環境差異に注意。厳密なしきい値評価はしていない。
- 変数のスコープ: `EXIT_CODE`が`local`でなくグローバル。副作用は小さいがベストプラクティスではない。
- 引数未引用: 将来的にスペース/特殊文字を含む入力で問題化する可能性。
- カラー出力の可読性: 非TTY環境やログ収集でエスケープシーケンスがノイズになる可能性。

エッジケース一覧

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| BINARYが存在しない | BINARY="./target/release/codanna" が未ビルド | 明確な失敗表示（exit 127等） | 失敗時に赤バツ表示 | 対応あり（ただし事前チェックなし） |
| jq未インストール | `command -v jq`失敗 | PRECEDENCE試験をスキップ/警告 | 現状COUNTが空/失敗判定の可能性 | 未対策 |
| grep未インストール | `command -v grep`失敗 | ERROR HANDLING試験をスキップ/警告 | 文字列検証が常に失敗 | 未対策 |
| JSONにcountがない | searchのJSONに`.count`なし | 優先度検証を不合格扱い/詳細表示 | COUNTが空で赤バツ表示 | 部分対応 |
| エラー文言が変更 | "Error: ..."が変更/ローカライズ | ERROR HANDLING試験の柔軟化（exit codeで判定） | 文字列固定grep | 未対策 |
| time出力が抑制されない | シェル差異 | 正しく計測/記録 | `> /dev/null 2>&1`だが保証弱 | 未対策 |
| 引数に空白/特殊文字 | traditional/keyvalueに空白・シェル記号 | 安全に引数渡し | 未引用で分割/展開の危険 | 未対策 |
| カラーコードが不要 | CIログで可読性低下 | 色なし/TTY検出 | 常に色出力 | 未対策 |

## Design & Architecture Suggestions

- 依存チェックの導入
  - スクリプト冒頭で `command -v jq grep` と `test -x "$BINARY"` を検証し、欠如時は明確な警告とスキップ/失敗を設定。
- 引数の安全な取り扱い
  - 文字列連結ではなく**配列**でコマンド引数を保持し、未引用展開を避ける。
- 終了コード中心の判定
  - ERROR HANDLINGは**exit code**で判定し、メッセージ検証は補助的扱いに。
- 優先度検証の強化
  - JSONスキーマ（例：results配列の長さ）からカウントを導出し、`.count`依存を緩和。`jq`がない場合のフォールバック（--jsonなしやテキスト出力での件数推定）は非推奨だが、明示エラーにする。
- 可観測性
  - `TIMEFORMAT`や`/usr/bin/time -f`で安定した計測。閾値判定（<300ms）も自動化。
- 信号/エラー管理
  - `set -euo pipefail` と `trap '...' ERR` で早期失敗と後始末を行う。

## Testing Strategy (Unit/Integration) with Examples

- スクリプト自体の単体検証
  - ダミーの`codanna`バイナリを作り、期待する終了コードとJSONを返すようにして動作確認。
- 統合検証
  - 実際のcodannaに対して、CIでこのスクリプトを実行し、失敗時はログ/JSONを保存。

ダミーcodannaの例（簡易スタブ）
```bash
#!/usr/bin/env bash
# ./target/release/codanna の代替として使用
if [ "$1" = "retrieve" ]; then
  cmd="$2"
  shift 2
  case "$cmd" in
    symbol)
      if [ $# -eq 0 ]; then
        echo "Error: symbol requires a name" >&2
        exit 1
      fi
      if [[ "$*" == *"name:missing"* || "$*" == *" missing "* ]]; then
        exit 3
      fi
      echo '{"name":"main","kind":"function"}'
      exit 0
      ;;
    search)
      # 例: --limit / limit: から最終的なlimitを判定（--limit優先）
      limit=10
      for arg in "$@"; do
        case "$arg" in
          --limit)
            shift
            limit="$1"
            ;;
          --limit=*)
            limit="${arg#--limit=}"
            ;;
          limit:*)
            kv="${arg#limit:}"
            # 既に--limitが存在する場合は上書きしない（優先度テスト用）
            ;;
        esac
      done
      # 優先度テスト観点でcount=limitとする
      echo "{\"count\":${limit},\"results\":[]}"
      exit 0
      ;;
    *)
      exit 0
      ;;
  esac
else
  echo "codanna stub"
fi
```

## Refactoring Plan & Best Practices

- ベースラインの堅牢化
  - 冒頭に `set -euo pipefail` を追加。
  - 依存チェックと親切なメッセージ。
- 関数分割
  - `run_retrieve`（外部呼び出しを安全に行う）、`print_result`（出力整形）、`check_precedence`（JSON解析）などに分離。
- 変数スコープの明確化
  - 関数内変数は`local`指定。色コードは`readonly`。
- 引数の配列化
  - コマンドと引数を配列で渡し、未引用展開を避ける。

改善例（test_commandの安全版）
```bash
#!/usr/bin/env bash
set -euo pipefail

readonly GREEN='\033[0;32m'
readonly RED='\033[0;31m'
readonly NC='\033[0m'

run_cmd() {
  local -a args=("$@")
  "${args[@]}" > /dev/null 2>&1
}

test_command_safe() {
  local cmd_name="$1"
  shift
  local -a traditional=("$BINARY" retrieve "$2")
  local -a keyvalue=("$BINARY" retrieve "$3")

  echo "Testing ${cmd_name}:"

  if run_cmd "${traditional[@]}"; then
    echo -e "  Traditional: ${GREEN}✓${NC}"
  else
    local exit_code=$?
    if [ "$exit_code" -eq 3 ]; then
      echo -e "  Traditional: ${GREEN}✓${NC} (not found - exit 3)"
    else
      echo -e "  Traditional: ${RED}✗${NC} (exit ${exit_code})"
    fi
  fi

  if run_cmd "${keyvalue[@]}"; then
    echo -e "  Key:value:   ${GREEN}✓${NC}"
  else
    local exit_code=$?
    if [ "$exit_code" -eq 3 ]; then
      echo -e "  Key:value:   ${GREEN}✓${NC} (not found - exit 3)"
    else
      echo -e "  Key:value:   ${RED}✗${NC} (exit ${exit_code})"
    fi
  fi
  echo ""
}
```

## Observability (Logging, Metrics, Tracing)

- ロギング
  - 失敗時には対象コマンドと引数をログ出力（色なしオプションも用意）。
- メトリクス
  - `TIMEFORMAT`で各コマンドの実行時間を収集。閾値（例：300ms）を自動評価。
  - 例:
```bash
TIMEFORMAT='real=%3R user=%3U sys=%3S'
time "$BINARY" retrieve search "unified output" limit:3 --json >/dev/null 2>&1
```
- トレーシング
  - `set -x`（デバッグ時限定）でコマンド展開を追跡。

## Risks & Unknowns

- 外部バイナリの**契約（exit code/JSON構造）**はこのチャンクでは不明。ここでの推定が将来変更される可能性。
- **Rustのメモリ安全性/エラー/並行性**については、このスクリプトからは評価不能（不明）。
- `time`の出力抑制と計測の安定性はシェル実装に依存。厳密なしきい値判定が未実装。
- エラーメッセージ文字列依存により、ローカライズや文言変更でテストが壊れるリスク。
- CI環境差異（PATH, 権限, 端末特性）によりカラー表示や外部コマンドの可用性が変動する可能性。