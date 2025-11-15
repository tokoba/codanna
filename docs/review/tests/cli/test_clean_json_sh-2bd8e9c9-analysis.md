# test_clean_json.sh Review

## TL;DR

- このスクリプトは、Rust製バイナリ「codanna」の各「retrieve」系コマンドが、標準出力に**クリーンなJSONのみ**を出し、デバッグ等は**標準エラー**に限定されているかを検証するための**統合テスト**。
- 中核ロジックは関数`test_json`で、1) 出力のJSON妥当性チェック、2) 標準エラーの有無を判定する2段構え。FDリダイレクト順序で「stderrのみ」を捕捉する点が重要。
- パイプテストでは、`retrieve symbol main`の出力からシンボル名を抽出して`retrieve callers`に渡すチェーンを検証。実運用に近い「パイプライン耐性」を確認。
- 重大リスクは、外部依存（`jq`、`xargs`、`codanna`バイナリ）未存在時の失敗検知がないこと、巨大出力を変数に格納することによるメモリ使用量増、特殊文字を含むシンボル名の引数伝播の不安定性。
- Rust側の安全性（メモリ安全、並行性、権限、秘密情報）はこのチャンクには現れない。テストが検証するのは「出力のチャネル分離とJSON妥当性」まで。
- 1コマンドを2回実行して両チャネルを確認する設計は分かりやすいが、パフォーマンス的に冗長。単回実行でstdout/stderrを別ファイルに分離する方式にリファクタ可能。

## Overview & Purpose

このBashスクリプトの目的は、`./target/release/codanna`の各取得系（retrieve）サブコマンドが、JSONモード（`--json`）で以下を満たすかの検証です。

- 標準出力（stdout）に**妥当なJSON**のみを出力すること
- デバッグやエラー等の非JSONは標準エラー（stderr）に出力されること
- コマンド同士のパイプラインで**機械処理可能**な出力が得られること

CIやローカル検証で「ログ混入によるJSON破損」を早期に検出するための**回帰テストスクリプト**です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Script | test_clean_json.sh | 内部（単体ファイル） | codanna出力のJSON妥当性とstderr混入検査、パイプライン検証 | Low |
| Function | test_json(cmd, desc) | 内部 | 指定コマンドのstdout+stderr取り回し、JSON妥当性とstderr有無チェック | Low |
| Variable | GREEN, RED, NC | 内部 | コンソール出力の色付け | Low |
| External | ./target/release/codanna | 外部依存 | Rustバイナリ。各retrieve操作を実行 | High（外部の振る舞いに依存） |
| External | jq | 外部依存 | JSONパース（妥当性検証、値抽出） | Medium |
| External | xargs | 外部依存 | パイプ値を次コマンド引数として注入 | Medium |

### Dependencies & Interactions

- 内部依存
  - メインフロー -> `test_json`を複数コール（7ケース）
  - パイプテストで`jq`出力を`xargs`に渡し、再度`codanna`を実行
- 外部依存（表）
  | 依存 | 用途 | 必須/任意 |
  |------|------|-----------|
  | ./target/release/codanna | テスト対象のCLI本体 | 必須 |
  | jq | JSON妥当性検査・抽出 | 必須 |
  | xargs | パイプラインで引数置換 | 任意（代替可） |
  | bash | シェル実行環境 | 必須 |
- 被依存推定
  - CI/CDパイプラインの「統合テスト」ステップ
  - ローカル開発環境での「退行検出」用途

## API Surface (Public/Exported) and Data Contracts

このファイルは公開APIを持たないテストスクリプトですが、内部関数のI/Fとデータ契約を整理します。

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| test_json | test_json "<cmd>" "<desc>" | 指定retrieveコマンドがクリーンJSONか検証 | O(size(output)) | O(size(output)) |
| メイン | N/A（スクリプト起動時） | 複数ケースの一括検証＋パイプライン検査 | 合計でO(∑output) | O(max(output)) |

詳細:

1) 目的と責務
- test_json
  - JSON妥当性を検査（stdout+stderr混合で破損検出）
  - stderrの有無を個別に検査
  - 合格/不合格の短いレポートを出力
- メイン
  - 検査ケースの定義と実行
  - パイプチェーン（symbol -> callers）の動作確認

2) アルゴリズム（ステップ分解）
- test_json
  - コマンドを「stdout+stderr結合」で実行し、`jq '.'`で妥当性検査
  - 同コマンドを「stderrのみ」捕捉（`2>&1 1>/dev/null`）して非JSONの混入を検査
  - 結果に応じて色付きで合否表示
- メイン
  - 7ケースのretrieve検査を順次実施
  - パイプチェーン：symbol名抽出 -> callers数抽出 -> 合否表示

3) 引数（test_json）
| 引数 | 型 | 必須 | 説明 |
|-----|----|------|------|
| cmd | string | 必須 | codannaに渡すサブコマンド列（例: `retrieve symbol main`） |
| desc | string | 必須 | 人間可読な説明（例: `symbol (exists)`） |

4) 戻り値
| 関数 | 戻り値 | 説明 |
|------|--------|------|
| test_json | なし（echo出力） | 合否メッセージをコンソール出力 |
| メイン | スクリプト終了コード | 現状は常に0（失敗時も続行）。改善余地あり |

5) 使用例
```bash
# 単体での呼び出し（内部から）
test_json "retrieve symbol main" "symbol (exists)"
```

6) エッジケース
- codannaが存在しない/実行不可
- jqが存在しない
- 出力が巨大でシェル変数を圧迫
- JSONは妥当だがstderrに警告がある
- シンボル名に空白・特殊文字が含まれる

## Walkthrough & Data Flow

- 起動時ヘッダ表示
- `test_json`を下記7パターンで呼び出し
  - `retrieve symbol main`（存在）
  - `retrieve symbol nonexistent`（非存在）
  - `retrieve callers new`
  - `retrieve calls main`
  - `retrieve describe OutputManager`
  - `retrieve implementations Parser`
  - `retrieve search parse`
- パイプテスト
  - `retrieve symbol main --json` -> `jq -r '.data.items[0].symbol.name'`
  -> `xargs -I {} ./codanna retrieve callers {} --json` -> `jq '.data.count'`

重要箇所（抜粋、FDリダイレクトの要点）:

```bash
test_json() {
    local cmd="$1"
    local desc="$2"
    echo -n "Testing $desc... "

    # 1回目：stdout+stderrを結合して捕捉（JSON妥当性チェック用）
    OUTPUT=$(./target/release/codanna $cmd --json 2>&1)

    # 2回目：stderrのみを捕捉（非JSON混入の検出）
    # ポイント：`2>&1 1>/dev/null` の順序により、FD2（stderr）を元のFD1（stdout）の行き先（= コマンド置換パイプ）へ、
    # その後FD1を/dev/nullへ変更。結果として「stderrだけ」が$(...)に流れる。
    STDERR=$(./target/release/codanna $cmd --json 2>&1 1>/dev/null)

    if echo "$OUTPUT" | jq '.' > /dev/null 2>&1; then
        if [ -z "$STDERR" ]; then
            echo -e "${GREEN}✓${NC} Clean JSON, no debug output"
        else
            echo -e "${RED}✗${NC} Has stderr output: $STDERR"
        fi
    else
        echo -e "${RED}✗${NC} Invalid JSON or has debug output"
        echo "  Output: $OUTPUT"
    fi
}
```

- データフロー
  - 入力：各retrieveコマンド文字列
  - 処理：codanna実行 -> 出力のチャネル分離 -> jqによるパース検証
  - 出力：合否メッセージ＋（必要に応じ）出力ダンプ

パイプチェーンは以下（直線的で分岐少・Mermaid要件に満たないため図示は省略）:

```bash
PIPE_RESULT=$(./target/release/codanna retrieve symbol main --json 2>/dev/null | \
    jq -r '.data.items[0].symbol.name' 2>/dev/null | \
    xargs -I {} ./target/release/codanna retrieve callers {} --json 2>/dev/null | \
    jq '.data.count' 2>/dev/null)
```

- 標準エラーは都度`2>/dev/null`で捨て、パイプを純粋にJSON処理に限定
- `xargs -I {}`でシンボル名を次コマンド引数へ注入（特殊文字の扱い注意点あり）

このチャンクでは行番号は提供されていないため、根拠の参照は関数名ベース。

## Complexity & Performance

- 時間計算量
  - 各テストケース：O(N)（N=出力サイズ。`jq`のパースコスト）
  - 全体：O(∑N)（7ケース＋パイプチェーン）
- 空間計算量
  - `OUTPUT`/`STDERR`に全文を格納するためO(N)（最大出力サイズ分）
- ボトルネック
  - 大きなJSONをシェル変数に保持することでメモリ負荷・速度低下
  - 同一コマンドを2回呼ぶ設計（JSON妥当性とstderr有無）による余分なI/O
- スケール限界
  - 巨大レスポンス（>数MB）ではシェル変数・`jq`のパース時間が増大
  - 外部依存が多く、環境差（OSのxargs仕様差など）で不安定になりうる
- 実運用負荷要因
  - codannaの内部処理（I/O/DB/ネットワーク）には非依存に見えるが、このチャンクには現れないため詳細は不明

## Edge Cases, Bugs, and Security

セキュリティチェックリストの観点で評価（Rust側の内部安全性は不明）:

- メモリ安全性（Bash側）
  - 巨大出力を変数保持する設計により、シェルのメモリ使用量が増える可能性
- インジェクション
  - コマンドインジェクション：`xargs -I {}`は基本的に「シェル解釈」を介さず引数として渡すが、実装差や特殊文字（改行、NULL文字）で不安定化する可能性
  - 推奨：`read -r`で安全に読み取り、直接クォートして関数呼び出しに渡す
- 認証・認可
  - このチャンクには現れない（不明）
- 秘密情報
  - ハードコード秘密情報はなし
  - 失敗時の`echo "Output: $OUTPUT"`はデータをそのまま表示するため、機密情報が出力される可能性がある運用環境では注意
- 並行性
  - スクリプトは逐次実行。並列なし
  - Rustバイナリの並行性安全性は不明

詳細なエッジケース表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| codanna未ビルド | バイナリ不存在 | エラーを検知して明示的に失敗 | 検知なし（`command not found`になる） | 改善要 |
| jq未インストール | `jq`未導入 | 前提不足を明示してスキップ/失敗 | 検知なし | 改善要 |
| 無効JSON | デバッグ行混入 | `jq`失敗で「Invalid JSON」を報告 | 対応あり（`jq '.'`） | 良好 |
| stderr出力あり | 警告のみstderr | 「Has stderr output: ...」を報告 | 対応あり | 良好 |
| 大出力 | 数MB〜 | 成果判定は可能だがパフォーマンス低下 | 変数保持のため負荷増 | 注意 |
| 特殊文字シンボル | `main v2 (β)` | callers取得に失敗しないこと | `xargs -I {}`で渡すが環境差あり | 注意 |
| 空結果 | `items[0]`なし | パイプテストが空で失敗 | 現状は「Pipe chain failed」 | 良好 |
| 標準出力→複数JSON | 複数行JSON | `jq '.'`成功（複数JSON可）だが契約次第 | おおむねOKだが契約要確認 | 不明 |
| エラーコード管理 | 一部失敗 | 非0で終了 | 現状はまとめて成功/失敗表示のみ | 改善要 |

## Design & Architecture Suggestions

- 事前チェックの追加
  - `command -v jq >/dev/null || { echo "jq required"; exit 1; }`
  - `test -x ./target/release/codanna || { echo "codanna missing"; exit 1; }`
- シェル安全性
  - `set -o pipefail`を有効化し、パイプ内の失敗を検知
  - 必要に応じて`set -u`（未定義変数エラー）を採用。ただしテスト継続性に配慮
- 1回実行で両チャネル捕捉（パフォーマンス改善）
  - 単回のcodanna起動でstdout/stderrを別ファイルに分離
  ```bash
  run_once() {
      local cmd="$1"
      local out_file err_file
      out_file=$(mktemp) && err_file=$(mktemp)
      ./target/release/codanna $cmd --json >"$out_file" 2>"$err_file"
      if jq '.' <"$out_file" >/dev/null 2>&1; then
          if [ ! -s "$err_file" ]; then
              echo -e "${GREEN}✓${NC} Clean JSON, no debug output"
          else
              echo -e "${RED}✗${NC} Has stderr output:"
              cat "$err_file"
          fi
      else
          echo -e "${RED}✗${NC} Invalid JSON or has debug output"
          echo "  Output:"
          cat "$out_file"
          [ -s "$err_file" ] && echo "  Stderr:" && cat "$err_file"
      fi
      rm -f "$out_file" "$err_file"
  }
  ```
- パイプテストの堅牢化
  - `read -r`＋クォートで安全な引数受け渡し
  ```bash
  name=$(./target/release/codanna retrieve symbol main --json 2>/dev/null | jq -r '.data.items[0].symbol.name')
  if [ -n "$name" ]; then
      count=$(./target/release/codanna retrieve callers "$name" --json 2>/dev/null | jq '.data.count' 2>/dev/null)
      # ...
  fi
  ```
- 出力契約の明文化
  - 「stdoutは単一JSONドキュメント」「stderrはテキストのみ」の契約をREADME/仕様に明記
- 失敗時の終了コード／集計
  - 失敗数をカウントし、最後に非0で終了できるモード（CI用）を追加

## Testing Strategy (Unit/Integration) with Examples

- 現状は統合テストのみ（CLIの実出力検証）
- 追加テスト例（このファイル内で拡張可能）:
  - コマンド存在・依存確認
  ```bash
  command -v jq >/dev/null || { echo -e "${RED}✗${NC} jq not found"; exit 1; }
  test -x ./target/release/codanna || { echo -e "${RED}✗${NC} codanna binary missing"; exit 1; }
  ```
  - エラーコマンド（未知サブコマンド）の扱い
  ```bash
  test_json "retrieve unknown_command foo" "unknown subcommand"
  ```
  - 複数アイテム出力の妥当性（配列）
  ```bash
  ./target/release/codanna retrieve search parse --json | jq -e '.data.items | type=="array"' >/dev/null \
    && echo -e "${GREEN}✓${NC} items is array" || echo -e "${RED}✗${NC} items not array"
  ```
  - 大出力確認（性能・妥当性）
    - 実際のコマンドは不明だが、フィルタ無し検索やリポジトリ全域スキャンで応答サイズが増えるケースを想定し、時間計測やサイズ閾値検査を導入

## Refactoring Plan & Best Practices

- 構造化
  - テストケースを配列に定義し、ループで処理
  - 合否カウンタと最後のサマリ表示
- 安全設定
  - `set -o pipefail`の導入
  - 必要時のみ`set -e`モードをサブ関数で有効化（全体停止を避ける）
- 外部依存検知
  - 起動前チェックとわかりやすいメッセージ
- 出力取り扱い
  - 単回実行でstdout/stderr分離（前述の`run_once`）
  - 巨大出力は変数保持せず、一時ファイル/ストリームでパース
- 引数の安全化
  - `xargs`依存を減らし、`read -r`＋クォート渡しへ
- CIフレンドリ
  - JSONレポート生成（合否カウント、ケース名、失敗原因）

## Observability (Logging, Metrics, Tracing)

- 現状
  - 人間向けの色付きログ（✓/✗）
  - 合否の簡単な説明
- 改善提案
  - 総テスト数／成功数／失敗数を最後に表示
  - `--ci`モードで機械可読なJSONレポートを出力
  ```bash
  # 例: JSONレポート
  echo '{"tests":[{"name":"symbol (exists)","status":"pass"},{"name":"callers","status":"fail","reason":"stderr not empty"}]}'
  ```
  - タイミング（経過時間）を計測して性能観測

## Risks & Unknowns

- このチャンクにはRust側の内部実装が現れないため、メモリ安全性、並行性、安全なログ分離の保証は**不明**。本スクリプトは症状（JSON破損、stderr混在）を検出するのみ。
- `codanna --json`の出力契約仕様（単一JSON/複数JSON、null許容、エラー時のJSON形状）が**不明**。テストの期待値調整が必要な可能性。
- `xargs`の実装差・ロケール差による引数扱いの差異は**リスク**。より堅牢な方法（`read -r`＋クォート）に置換を推奨。
- 巨大出力時のメモリ／時間使用は**環境依存**。CIのリソース制限に注意。