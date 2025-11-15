# display\help.rs Review

## TL;DR

- 目的: CLIヘルプの見出しやコマンド説明を一貫したスタイルで整形するための軽量ユーティリティモジュール
- 主要公開API: **format_help_section**, **create_help_text**, **format_command_description**（すべて pub）
- 複雑箇所: 行ごとのインデント挙動とカラー有無の分岐（format_help_section: L9–13, L15–23）
- 重大リスク: 色付き表示時の幅指定（{:16}）がANSIコードを含むと揃え不整合の可能性、インデント判定が「先頭4スペース」固定でタブや他幅を想定していない
- Rust安全性: unsafe未使用、所有権・借用は最小限で安全、同期なし・データ競合なし
- エラー設計: 例外やResultなし、純粋に文字列整形のみ。パニックポイントは事実上なし（format!の正常使用）

## Overview & Purpose

このファイルは、CLIのヘルプ表示を一貫したスタイル（色、太字、インデント）で整形するためのフォーマッタを提供します。カラー出力の有効/無効は外部の**Theme**（crate::display::theme::Theme）に委ね、見出しと本文、コマンド名と説明文の表示を簡潔に構築します。  
利用側はこのモジュールの公開APIを呼び出すことで、環境（カラー可否）に応じて適切な整形済み文字列を受け取れます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | format_help_section | pub | 見出しの色/太字化と本文の行ごとのインデント整形 | Low |
| Function | create_help_text | pub | 「QUICK START」「LEARN MORE」からなる標準ヘルプ文の生成 | Low |
| Function | format_command_description | pub | コマンド名を幅揃えし、説明文を連結して表示 | Low |

### Dependencies & Interactions

- 内部依存
  - create_help_text → format_help_section（L34, L38）
- 外部依存（このチャンク内で使用しているもの）
  | 依存 | 使用箇所 | 目的 | 備考 |
  |------|----------|------|------|
  | crate::display::theme::Theme | L9, L43 | カラー有無の判定 | 実装はこのチャンクには現れない（不明） |
  | console::style | L12, L46 | 色付け/太字スタイルを付与 | ANSIスタイルコード出力。幅指定時の揃えに影響の可能性 |

- 被依存推定
  - CLIのヘルプ表示やサブコマンド一覧を出すUIレイヤからの利用が想定されますが、具体的な呼び出し箇所はこのチャンクには現れない（不明）。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| format_help_section | pub fn format_help_section(title: &str, content: &str, indent: bool) -> String | 見出しと本文をスタイル適用し整形 | O(|title| + |content|) | O(|title| + |content|) |
| create_help_text | pub fn create_help_text() -> String | 標準のヘルプ文を生成 | O(1)（定数長） | O(1)（定数長） |
| format_command_description | pub fn format_command_description(name: &str, description: &str) -> String | コマンド名と説明を1行に整形（幅揃え） | O(|name| + |description|) | O(|name| + |description|) |

### format_help_section

1) 目的と責務  
- 見出しにカラー/太字スタイル（環境により無効化）を適用し、本文を行単位でインデントしつつ整形する（L6–25）。  
- 空行は空行として維持（L16–17）。

2) アルゴリズム（主要ステップ）  
- カラー無効判定（Theme::should_disable_colors: L9）で見出しを通常orスタイル付きで追加  
- content.lines()で各行を走査（L15）  
  - 行が空白のみなら改行追加（L16–17）  
  - indentがtrueかつ先頭4スペースでない場合は"    "を付加（L18–20）  
  - それ以外はそのまま行追加（L20–21）  
- 最終的な文字列を返す（L24）

3) 引数
| 引数名 | 型 | 必須 | 説明 |
|-------|----|------|------|
| title | &str | はい | セクション見出し |
| content | &str | はい | セクション本文（複数行を想定） |
| indent | bool | はい | 本文各行に4スペースのインデントを付けるかどうか |

4) 戻り値
| 型 | 説明 |
|----|------|
| String | 整形済みヘルプセクションの文字列 |

5) 使用例
```rust
let title = "USAGE";
let content = "codanna [OPTIONS]\nRun the server";
// 本文にインデントを付ける
let section = format_help_section(title, content, true);
println!("{}", section);
```

6) エッジケース
- contentが空文字列の場合も見出しのみが出力される
- 行がすでに4スペースで始まっていれば追加インデントは付加されない
- 行頭がタブや2スペースなど「4スペース以外」のインデントは認識されず、さらに4スペースが付加され得る
- タイトル/本文に非ASCII文字が含まれていてもそのまま出力される

7) 根拠（分岐の存在）  
- カラー分岐（L9–13）  
- 行の分岐（空行/インデント付与/そのまま: L16–21）

### create_help_text

1) 目的と責務  
- 標準の「QUICK START」「LEARN MORE」セクションからなるヘルプ全文を組み立てる（L27–40）。

2) アルゴリズム  
- 定数文字列quick_startとlearn_moreを定義（L30–33, L37）  
- format_help_sectionを2回呼び出してセクションを連結（L34, L38）  
- 間に改行を挿入（L35）

3) 引数
| 引数名 | 型 | 必須 | 説明 |
|-------|----|------|------|
| なし | - | - | - |

4) 戻り値
| 型 | 説明 |
|----|------|
| String | 標準ヘルプ文 |

5) 使用例
```rust
let help_text = create_help_text();
println!("{}", help_text);
```

6) エッジケース
- 内容は固定のため外部入力によるエッジケースは基本的にない
- カラーはThemeに依存（このチャンクには実装が現れない）

7) 根拠  
- format_help_section呼び出し（L34, L38）

### format_command_description

1) 目的と責務  
- コマンド名を16幅で揃え、説明文を続けて1行の説明を整形（カラー無効時はプレーン、カラー有効時はnameを緑色に）（L42–48）。

2) アルゴリズム  
- カラー無効なら`format!("{name:16} {description}")`（L43–45）  
- 有効なら`format!("{:16} {}", style(name).green(), description)`（L45–47）

3) 引数
| 引数名 | 型 | 必須 | 説明 |
|-------|----|------|------|
| name | &str | はい | コマンド名 |
| description | &str | はい | コマンドの説明 |

4) 戻り値
| 型 | 説明 |
|----|------|
| String | 整形済みコマンド説明行 |

5) 使用例
```rust
let line = format_command_description("serve", "Start the HTTP/HTTPS server");
println!("{}", line);
```

6) エッジケース
- nameが16文字を超える場合、幅指定により折り返しは行われず、行全体の整形が崩れる可能性あり
- 色付き出力時、ANSIコードの長さが幅計算に影響し、見た目の揃えがずれる可能性（詳細は不明）

7) 根拠  
- カラー分岐（L43–47）

## Walkthrough & Data Flow

- 入力はすべて&str、出力は新規に構築されたString。  
- **format_help_section**のフロー:
  - Themeでカラー可否判定 → 見出しの追加
  - contentを行分割 → 各行について空行/インデント付与/そのままを選択 → 追記
- **create_help_text**は固定文字列をformat_help_sectionに渡して連結するのみ。
- **format_command_description**はカラー有無でフォーマット文字列を切り替え、nameを16幅で整形した後descriptionを続ける。

```mermaid
flowchart TD
  A[format_help_section(title, content, indent)] --> B{Theme::should_disable_colors?}
  B -->|true| C[push_str plain title + '\n']
  B -->|false| D[push_str styled title (cyan, bold) + '\n']
  C --> E[for line in content.lines()]
  D --> E
  E --> F{line.trim().is_empty()?}
  F -->|true| G[push '\n']
  F -->|false| H{indent && !line.starts_with("    ")?}
  H -->|true| I[push_str "    " + line + '\n']
  H -->|false| J[push_str line + '\n']
  G --> K[次の行]
  I --> K
  J --> K
  K -->|全行処理後| L[return output]
```

上記の図は`format_help_section`関数（L6–25）の主要分岐を示す。

## Complexity & Performance

- format_help_section:  
  - 時間計算量: O(|title| + Σ|line_i|) ≒ O(|title| + |content|)  
  - 空間計算量: O(|title| + |content|)（出力Stringのサイズ）  
  - ボトルネック: contentが非常に長い場合の文字列連結。必要に応じてString::with_capacityで事前確保可能。
- create_help_text:  
  - 時間/空間: 定数（固定文言のみ）
- format_command_description:  
  - 時間: O(|name| + |description|)  
  - 空間: O(|name| + |description|)

実運用負荷要因: I/O/ネットワーク/DBは関与せず、標準出力への表示時にのみ影響。多量のヘルプ生成は通常想定されず、パフォーマンス問題は軽微。

## Edge Cases, Bugs, and Security

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列本文 | content="" | 見出しのみ出力、本文なし | format_help_section（L6–25） | OK |
| 既に4スペース | "    line" + indent=true | 追加インデント不要でそのまま出力 | starts_with("    ")判定（L18） | OK |
| タブや2スペースでのインデント | "\tline" or "  line" + indent=true | 二重インデントを避けたいが現実は4スペース追加 | 4スペース固定判定のみ（L18） | 注意（改善余地） |
| 空行の保持 | "line1\n\nline2" | 空行がそのまま保持される | trim().is_empty()（L16–17） | OK |
| name長過ぎによる幅揃え | nameが32文字以上 | 幅指定で揃えが崩れる可能性 | {:16}使用（L43–47） | 注意 |
| 色付き幅揃えの不整合 | Themeがカラー有効 | 見た目の列揃えが崩れる可能性 | ANSIコードと幅指定の相互作用（詳細不明） | 不明（リスクあり） |

- セキュリティチェックリスト
  - メモリ安全性:  
    - Buffer overflow: なし（Rust安全なString操作のみ）  
    - Use-after-free: なし（所有権/借用はスコープ内完結）  
    - Integer overflow: なし（長さ計算のみ）
  - インジェクション:  
    - SQL/Command/Path: なし（外部コマンド未実行、単なる文字列整形）  
  - 認証・認可: 該当なし  
  - 秘密情報: ハードコード秘密情報なし（GitHubリンクのみ）／ログ漏洩なし  
  - 並行性: Race/Deadlockなし（同期なし）

- Rust特有の観点
  - 所有権: 戻り値は新規String（move）。引数は&str借用のみで安全（L6, L42）。  
  - 借用: 可変借用はoutput変数に限定され、関数スコープ内で完結。  
  - ライフタイム: 明示的なライフタイム不要（&str借用は呼び出し元管理）。  
  - unsafe境界: unsafe未使用（全体）。  
  - 並行性/非同期: Send/Syncに関与しない（シングルスレッド前提の整形）。awaitなし、キャンセルなし。  
  - エラー設計: Result/Option不使用、unwrap/expectなし、panic可能性は低い（format!に不正なフォーマット指定はなし）。

## Design & Architecture Suggestions

- インデント判定の汎用化  
  - 現在は「先頭4スペース」固定（L18）。タブや任意幅を考慮するため、IndentPolicy（タブ/スペース幅）をパラメータ化するとよい。
- 幅揃えとANSIコード  
  - 色付き（style）時の幅指定は揃え崩れのリスク。**可視幅**（display width）に基づきパディングするユーティリティを導入する（例: ANSIコードを除去した長さで揃える）。
- String事前確保  
  - format_help_sectionのoutputに対して、`String::with_capacity(title.len() + content.len() + 16)`などで軽微なパフォーマンス改善。
- ローカライズ対応  
  - create_help_textの固定文言を静的定数か外部リソースに分離し、将来的な翻訳対応を容易に。
- テーマとスタイルの抽象化  
  - Themeに「見出しスタイル」「コマンド名スタイル」を委譲して、色や装飾方針を統一管理する。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト方針
  - format_help_section
    - インデント有無の挙動（先頭4スペース行は追加インデントしない）
    - 空行保持
    - カラー無効時の見出しが素の文字列になること
  - format_command_description
    - カラー無効時の幅揃え（name 16幅）
    - 長いnameで揃えが崩れることの確認（期待仕様を決める）
  - create_help_text
    - 固定文言が含まれていること（"QUICK START", "LEARN MORE"）

- サンプルユニットテスト（カラーの有無に依存しないよう、ANSIコードを簡易的に除去するヘルパを使う）
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn strip_ansi(s: &str) -> String {
        // 最低限のANSIエスケープ除去（厳密ではない）
        let mut out = String::with_capacity(s.len());
        let mut bytes = s.bytes().peekable();
        while let Some(b) = bytes.next() {
            if b == 0x1b { // ESC
                // CSIシーケンスをざっくりスキップ: \x1b[ ... m
                if let Some(b2) = bytes.peek() {
                    if *b2 == b'[' {
                        // 'm'が来るまで飛ばす
                        while let Some(x) = bytes.next() {
                            if x == b'm' { break; }
                        }
                        continue;
                    }
                }
            }
            out.push(b as char);
        }
        out
    }

    #[test]
    fn help_section_indent_and_blank_lines() {
        let title = "TITLE";
        let content = "line1\n\n  line2\n    line3";
        let s = format_help_section(title, content, true);
        let plain = strip_ansi(&s);

        // 見出しは末尾に改行
        assert!(plain.starts_with("TITLE\n"));

        // 空行保持
        assert!(plain.contains("line1\n\n"));

        // "  line2" は4スペースではないので追加インデントされる
        assert!(plain.contains("      line2\n"));

        // "    line3" は既に4スペースなのでそのまま
        assert!(plain.contains("    line3\n"));
    }

    #[test]
    fn command_description_alignment_plain() {
        // カラー有効/無効に依らず、可視文字だけでチェック
        let s = format_command_description("serve", "Start server");
        let plain = strip_ansi(&s);

        // "serve" は左詰めで16幅、その後にスペースと説明
        assert!(plain.starts_with("serve           Start server"));
    }

    #[test]
    fn create_help_text_contains_sections() {
        let s = create_help_text();
        let plain = strip_ansi(&s);

        assert!(plain.contains("QUICK START"));
        assert!(plain.contains("LEARN MORE"));
        assert!(plain.contains("GitHub: https://github.com/bartolli/codanna"));
    }
}
```

- インテグレーションテスト案
  - 実行環境のカラー可否（Theme::should_disable_colors）に応じた出力差を、ANSI除去後のテキストで正規化して検証。

## Refactoring Plan & Best Practices

- インデントの柔軟化
  - `indent: bool`を`Indent::None | Indent::Spaces(usize) | Indent::Tabs(usize)`等に拡張し、`starts_with`ではなく「既存インデント量」を解析して二重インデントを回避。
- 可視幅でのパディング
  - `format_command_description`の16幅指定を「可視文字幅」に基づく関数に委譲し、ANSIの影響を排除。
- 事前容量確保
  - `format_help_section`で`String::with_capacity`を導入して小改善。
- 共通スタイルポリシー
  - タイトルの色（cyan/bold）やコマンド名の色（green）をThemeに集約して、将来の配色変更を容易に。
- ドキュメント強化
  - インデントのルール（4スペース固定）と幅指定の前提をRustdocに明記。

## Observability (Logging, Metrics, Tracing)

- 本モジュールは表示文字列を返すのみで、ログやメトリクス、トレースは不要。  
- もし将来的にCLIの表示品質計測（例: ヘルプ表示回数）を行うなら、呼び出し側でカウントするのが適切。モジュール自体は無状態を保つのが望ましい。

## Risks & Unknowns

- Theme::should_disable_colorsの判定ロジックはこのチャンクには現れないため、CI環境や非TTYでの振る舞いは不明。
- ANSIコードとフォーマッタの幅指定の相互作用により、色付き時の揃えが乱れる可能性（実際の端末/環境依存、consoleクレートの実装詳細は不明）。
- インデント検出が「先頭4スペース」に限られているため、タブ/異なるスペース幅の既存インデントとの相互作用は未定義。  
- 国際化/ローカライズの対応可否は未定義。