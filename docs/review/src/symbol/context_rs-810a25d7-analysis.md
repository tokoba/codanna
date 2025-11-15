## symbol\context.rs Review

## TL;DR

- 目的: **SymbolContext**は、単一のシンボルに関するメタデータ（ファイル位置、可視性、署名、ドキュメント、各種リレーション）を集約し、読みやすいテキスト形式で出力するためのユーティリティ。
- 主要公開API: **format_location**、**format_location_with_type**、**format_full**、および**Display**実装（fmt）。さらに、関係容器の**SymbolRelationships**とコンテキスト制御用**ContextIncludes**（bitflags）。
- コアロジック: 関係の整形は**append_relationships**が担当し、Option/Vecの状態に応じた分岐とメタデータ（行番号・コンテキスト）の優先使用を行う。
- Rust安全性: 全て安全なRustで記述、**unsafeなし**。行番号の加算に**saturating_add**を利用し、オーバーフロー防止。
- エラー設計: 例外は使わず、全APIはString生成に終始（Result不使用）。失敗しない代わりに出力の正確性は入力に依存。
- 重大リスク: ドキュメントやファイルパスをそのまま出力するため、**ログ/端末エスケープ未対策**による表示汚染リスク。大量関係時の**巨大文字列生成**。
- 改善提案: **ContextIncludesフラグの適用**による出力制御、**共通ロジックの抽出**（calls/called_by）、**Writerベース**のストリーミング出力、**サニタイズ処理**の導入。

## Overview & Purpose

このファイルは、1つのシンボル（関数、型、トレイト等）について、関連するメタデータを集約し、使いやすい1つのテキスト出力にまとめるためのヘルパー群を提供します。主な目的は以下です。

- シンボルの**位置情報**（ファイルパス＋行レンジ）を一貫した形式で表示。
- **署名**や**ドキュメントコメント**の要約を含め、読みやすい形式に整形。
- シンボル間の**関係**（実装関係、定義、呼び出し関係）をカテゴリ別に表示。
- 出力はCLIやレポート生成など、多用途な表示に活用可能。

このモジュールは表示と整形に特化し、実際の関係解決やシンボル解析は上位レイヤ（crate::Symbol, crate::relationship）に依存しています。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | SymbolContext | pub | シンボル1件の総合文脈（メタデータ＋関係）保持と整形 | Med |
| Struct | SymbolRelationships | pub | 関係カテゴリ（implements/implemented_by/defines/calls/called_by）のコンテナ | Low |
| bitflags | ContextIncludes | pub | 出力に含める関係種別のフラグ管理（未使用のため拡張余地） | Low |
| Impl | fmt::Display for SymbolContext | pub | format_fullを用いたユーザ向け整形の標準表示（末尾改行調整） | Low |
| Method | SymbolContext::format_location | pub | 名称＋位置の簡易表示 | Low |
| Method | SymbolContext::format_location_with_type | pub | 種別＋名称＋位置＋IDの簡易表示 | Low |
| Method | SymbolContext::format_full | pub | ヘッダ、メタデータ、関係の包括整形 | Med |
| Method | SymbolContext::symbol_location | pub(crate) | ファイルパス＋行レンジを一貫形式に整形 | Low |
| Method | SymbolContext::append_header | private | format_fullヘッダ整形 | Low |
| Method | SymbolContext::append_metadata | private | モジュールパス、署名、可視性、ドキュメントの整形 | Med |
| Method | SymbolContext::append_relationships | private | 関係（5カテゴリ）の整形、メタデータ行優先の位置出力 | High |
| Method | SymbolContext::write_multiline | private | 複数行文字列のインデント付き出力 | Low |

### Dependencies & Interactions

- 内部依存
  - format_full → append_header / append_metadata / append_relationships
  - append_metadata → symbol.as_module_path(), as_signature(), as_doc_comment()
  - append_relationships → SymbolContext::symbol_location(), RelationshipMetadata（line, context）
  - Display(fmt) → format_full

- 外部依存（クレート・モジュール）
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | crate::Symbol | シンボルメタデータ（name, kind, id, file_path, range, visibility, 各種メソッド） | 詳細はこのチャンクに現れない |
  | crate::Visibility | 可視性の列挙 | Private判定に使用 |
  | crate::relationship::RelationshipMetadata | 呼び出しサイトの行番号・コンテキスト表示 | フィールド仕様の詳細不明（line, context使用のみ確認） |
  | bitflags | ContextIncludesのフラグ定義 | 出力制御フラグ |
  | serde::Serialize | 構造体のシリアライズ | フロントエンド/API出力向け |
  | std::fmt | Display実装とFormatter | 標準表示 |

- 被依存推定（このモジュールを使用し得る箇所）
  - CLI/TTY向けのシンボル情報ビュー
  - ドキュメント生成ツール（静的サイト/HTMLビュー）
  - IDE/エディタ拡張でのシンボル情報ポップアップ
  - サーバ/API層でのJSON出力（Serialize済み構造体）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| SymbolContext::format_location | fn format_location(&self) -> String | 名称＋位置（ファイル:行 or 行範囲）の簡易表示 | O(1) | O(L) |
| SymbolContext::format_location_with_type | fn format_location_with_type(&self) -> String | 種別＋名称＋位置＋symbol_idの簡易表示 | O(1) | O(L) |
| SymbolContext::format_full | fn format_full(&self, indent: &str) -> String | ヘッダ、メタデータ、関係の包括的な人間向け表示 | O(N) | O(S) |
| fmt::Display for SymbolContext | fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result | format_fullの表示。末尾改行を除去 | O(N) | O(1)（Formatterへの書込み） |
| ContextIncludes | bitflags! { pub struct ContextIncludes: u8 { ... } } | 出力に含める関係のフラグ（拡張用） | O(1) | O(1) |
| SymbolRelationships | pub struct SymbolRelationships { ... } | 関係データ用のコンテナ（Option<Vec<...>>） | 生成/利用に依存 | 保持データ量に依存 |

注: Nは関係リスト内の要素総数、Lは文字列長、Sは出力文字列長合計。  
symbol_locationはpub(crate)のため外部公開APIではありません。

### SymbolContext::format_location

1) 目的と責務
- シンボル名と位置の最小限情報を提供し、一覧表示やリンク生成に適する簡潔な文字列を返します。

2) アルゴリズム（ステップ分解）
- 自シンボル名を取得
- symbol_location(&self.symbol)を呼び出し、"path:start"または"path:start-end"の文字列を取得
- "name at location"形式の文字列を返す

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| self | &SymbolContext | 対象コンテキスト |

4) 戻り値

| 型 | 説明 |
|----|------|
| String | "name at path:line"形式の表示文字列 |

5) 使用例

```rust
// 既存のcontextが与えられている前提（このチャンクにSymbolの構築方法は現れない）
let s = context.format_location();
println!("{s}");
```

6) エッジケース
- symbol.rangeのstart_line == end_line → 単一行の位置表示
- symbol.file_pathが空/不正 → そのまま文字列化される（サニタイズなし）

短い関数の引用（行番号不明）:
```rust
pub fn format_location(&self) -> String {
    format!(
        "{} at {}",
        self.symbol.name,
        Self::symbol_location(&self.symbol)
    )
}
```

### SymbolContext::format_location_with_type

1) 目的と責務
- 種別（kind）、名称、位置、symbol_idを含む詳細な1行表現を生成。

2) アルゴリズム
- self.symbol.kindをDebug表示
- symbol_locationを利用して位置文字列を取得
- id.value()を取得して識別子表示
- 全てを1行にフォーマット

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| self | &SymbolContext | 対象コンテキスト |

4) 戻り値

| 型 | 説明 |
|----|------|
| String | "{:?} name at path:line [symbol_id:X]"形式 |

5) 使用例

```rust
let s = context.format_location_with_type();
println!("{s}");
```

6) エッジケース
- kindのDebug表示が長い/詳細すぎる場合もそのまま出力
- id.value()が大きい値でもそのまま出力

短い関数の引用（行番号不明）:
```rust
pub fn format_location_with_type(&self) -> String {
    format!(
        "{:?} {} at {} [symbol_id:{}]",
        self.symbol.kind,
        self.symbol.name,
        Self::symbol_location(&self.symbol),
        self.symbol.id.value()
    )
}
```

### SymbolContext::format_full

1) 目的と責務
- ヘッダ（名称・種別・位置・ID）、メタデータ（モジュールパス、署名、可視性、ドキュメント）、関係（実装・定義・呼び出し・呼び出され）を統合した包括表示を返す。

2) アルゴリズム
- String出力バッファを作成
- append_headerを呼び出し
- append_metadataを呼び出し
- append_relationshipsを呼び出し
- 完成した文字列を返す

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| self | &SymbolContext | 対象コンテキスト |
| indent | &str | 先頭インデント（スペースやタブ） |

4) 戻り値

| 型 | 説明 |
|----|------|
| String | 複数行の整形済み文字列 |

5) 使用例

```rust
let report = context.format_full("  "); // 2スペースインデント
println!("{report}");
```

6) エッジケース
- relationshipsが全てNone/空 → 関係セクションは出力されない
- as_signatureが複数行 → write_multilineでインデントを揃えて行ごと出力
- ドキュメントコメントが長い → 先頭2行のみプレビューし末尾に"..."を付加

短い関数の引用（行番号不明）:
```rust
pub fn format_full(&self, indent: &str) -> String {
    let mut output = String::new();
    self.append_header(&mut output, indent);
    self.append_metadata(&mut output, indent);
    self.append_relationships(&mut output, indent);
    output
}
```

### fmt::Display for SymbolContext

1) 目的と責務
- format_fullの出力を標準表示に統合し、末尾の改行を1つ削除して使いやすい表示を提供。

2) アルゴリズム
- format_full("")で出力を取得
- 末尾が'\n'なら1文字分を除去
- Formatterに書き込む

3) 引数

| 名前 | 型 | 説明 |
|------|----|------|
| self | &SymbolContext | 対象 |
| f | &mut fmt::Formatter<'_> | フォーマッタ |

4) 戻り値

| 型 | 説明 |
|----|------|
| fmt::Result | 表示の成否 |

5) 使用例

```rust
println!("{}", context); // 末尾改行なしで1ブロック表示
```

6) エッジケース
- format_fullが空文字列の場合も安全に動作
- 改行がない出力の場合、無変更

引用（行番号不明）:
```rust
impl fmt::Display for SymbolContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let formatted = self.format_full("");

        if formatted.ends_with('\n') {
            write!(f, "{}", &formatted[..formatted.len() - 1])
        } else {
            write!(f, "{formatted}")
        }
    }
}
```

### ContextIncludes（bitflags）

- 目的: 出力に含める関係種別（IMPLEMENTATIONS, DEFINITIONS, CALLS, CALLERS, ALL）を表現。
- このファイル内では**使用箇所なし**。今後の拡張でformat_fullやappend_*に適用可能。

引用（行番号不明）:
```rust
bitflags! {
    pub struct ContextIncludes: u8 {
        const IMPLEMENTATIONS = 0b00000001;
        const DEFINITIONS    = 0b00000010;
        const CALLS         = 0b00000100;
        const CALLERS       = 0b00001000;
        const ALL           = 0b00001111;
    }
}
```

### SymbolRelationships

- 目的: 関係カテゴリをOption<Vec<...>>で保持し、未設定時に出力を抑制。
- フィールド:
  - implements: Option<Vec<Symbol>>
  - implemented_by: Option<Vec<Symbol>>
  - defines: Option<Vec<Symbol>>
  - calls: Option<Vec<(Symbol, Option<RelationshipMetadata>)>>
  - called_by: Option<Vec<(Symbol, Option<RelationshipMetadata>)>>

## Walkthrough & Data Flow

全体のフロー:
- ヘッダ行出力: name, kind, file位置, symbol_id
- メタデータ出力:
  - モジュールパス（as_module_pathがSomeの場合）
  - シグネチャ（as_signatureがSomeの場合、複数行をインデント付きで整形）
  - 可視性（Private以外のみ）
  - ドキュメントプレビュー（先頭2行＋...）
- 関係出力（存在するカテゴリのみ）
  - implements: 「Implements:」配下に各シンボル
  - implemented_by: 件数と各シンボル
  - defines: 件数と各シンボル＋署名（存在する場合のみ）
  - calls / called_by: 件数と各シンボル。位置はmetadata.lineがSomeなら呼び出しサイトの行を使用、なければ定義位置
  - metadata.contextがSomeかつ非空なら角括弧で補足表示

関係整形の主要分岐図（条件数が多いためMermaidで表現）:

```mermaid
flowchart TD
    A[append_relationships start] --> B{implements is Some & !empty?}
    B -- yes --> B1[Print 'Implements' and list symbols]
    B -- no --> C{implemented_by is Some & !empty?}
    C -- yes --> C1[Print 'Implemented by N symbol(s):' and list]
    C -- no --> D{defines is Some & !empty?}
    D -- yes --> D1[Print 'Defines N symbol(s):' and list\nInclude signature if available]
    D -- no --> E{calls is Some & !empty?}
    E -- yes --> E1[Print 'Calls N function(s):'\nFor each (called, metadata):\nif metadata.line Some -> use callsite path:line\nelse -> use symbol_location(called)\nif metadata.context non-empty -> append [context]]
    E -- no --> F{called_by is Some & !empty?}
    F -- yes --> F1[Print 'Called by N function(s):'\nSame logic as Calls]
    F -- no --> G[append_relationships end]
```

上記の図は`append_relationships`関数（行番号不明）の主要分岐を示す。

位置文字列生成（symbol_locationのデータフロー）:
- range.start_lineとrange.end_lineを1ベース化（saturating_add(1)）
- start == endなら "path:start"
- 異なる場合は "path:start-end"

引用（行番号不明）:
```rust
pub(crate) fn symbol_location(symbol: &Symbol) -> String {
    let start = symbol.range.start_line.saturating_add(1);
    let end = symbol.range.end_line.saturating_add(1);
    if start == end {
        format!("{}:{start}", symbol.file_path)
    } else {
        format!("{}:{start}-{end}", symbol.file_path)
    }
}
```

複数行テキスト整形（write_multiline）:
- 指定インデント＋extra_spaces分の空白を前置して各行を出力

引用（行番号不明）:
```rust
fn write_multiline(output: &mut String, text: &str, indent: &str, extra_spaces: usize) {
    let padding = format!("{indent}{:width$}", "", width = extra_spaces);
    for line in text.lines() {
        output.push_str(&padding);
        output.push_str(line);
        output.push('\n');
    }
}
```

## Complexity & Performance

- 時間計算量
  - format_location/format_location_with_type: O(1)
  - format_full: O(N + M + K)
    - N: 各関係ベクタの要素総数（implements, implemented_by, defines, calls, called_byの合計）
    - M: 署名の行数（write_multiline）
    - K: ドキュメントプレビューの行カウント（最大2行＋判定）
- 空間計算量
  - 出力文字列の長さに比例 O(S)。関係や署名が多いほど増加
- ボトルネック
  - 大量の関係や長大な署名を持つシンボルでの文字列結合コスト
  - calls/called_byでの繰り返しformat!呼び出し
- スケール限界
  - 数万件の関係を持つ場合、単一Stringへの連結はメモリ・CPU負荷大
- 実運用負荷要因
  - I/Oやネットワーク、DBは本ファイルの責務外
  - 端末出力やログ出力側のレンダリング時間・文字数制限に影響

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:

- メモリ安全性
  - Buffer overflow: 文字列操作のみでunsafeなし。Rust標準のString/format!使用で安全。
  - Use-after-free: 所有権/借用は&self中心で問題なし。
  - Integer overflow: 行番号調整にsaturating_add(1)採用（symbol_location、calls/called_byでmetadata.line使用時もsaturating_add(1)）（append_relationships内の式にて確認、行番号不明）。
- インジェクション
  - SQL/Command/Path traversal: 該当なし（表示のみ）。
  - ログ/端末エスケープ: ドキュメントコメントやファイルパスをそのまま表示するため、ANSIエスケープや特殊文字による表示汚染のリスクあり。サニタイズやエスケープが望ましい。
- 認証・認可
  - 該当なし（表示のみ）。
- 秘密情報
  - Hard-coded secrets: なし。
  - Log leakage: ドキュメントコメントに機密が含まれている場合、プレビュー表示で漏えいの可能性。制御オプションで無効化を推奨。
- 並行性
  - Race condition / Deadlock: 該当なし（状態共有やスレッド操作なし）。

Rust特有の観点:

- 所有権: 全メソッドは&selfを取り、フィールドの参照・表示のみ。値の移動は発生しない（関数名:行番号不明）。
- 借用: 文字列出力用に&mut Stringを局所的に使用し、他の共有状態なし。可変借用の期間は関数スコープ内のみで安全。
- ライフタイム: 明示的ライフタイムなし。必要性もなし。
- unsafe境界: 使用なし。
- 並行性・非同期: 非同期APIやSend/Sync境界に関する記述なし。この構造体のSend/SyncはSymbol/RelationshipMetadataの実装に依存（このチャンクには現れない）。
- await境界/キャンセル: 該当なし。
- エラー設計: 全メソッドはString生成で、ResultやOptionを返さない。表示専用のため妥当。panicを誘発するunwrap/expectは使用なし。エラー変換（From/Into）なし。

エッジケース詳細表:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空の関係 | relationshipsの各フィールドがNoneまたは空Vec | 関係セクションは出力しない | append_relationshipsでis_empty/Option判定 | 実装済 |
| 単一行範囲 | range.start_line == range.end_line | "path:start"形式 | symbol_location | 実装済 |
| 複数行範囲 | start_line != end_line | "path:start-end"形式 | symbol_location | 実装済 |
| 行番号が0起点 | metadata.line = Some(0) | "path:1"として表示 | saturating_add(1) | 実装済 |
| 行番号未提供 | metadata.line = None | 定義位置を使用 | append_relationships内分岐 | 実装済 |
| ドキュメントが3行以上 | as_doc_comment()が3行以上 | 先頭2行＋"..."でプレビュー | append_metadata | 実装済 |
| 可視性がPrivate | Visibility::Private | 非表示（行を出力しない） | append_metadata | 実装済 |
| 長大署名 | 100+行の署名 | 各行にインデント＋改行出力 | write_multiline | 実装済 |
| ファイルパスに特殊文字 | "file\npath.rs"等 | そのまま出力（表示乱れ） | サニタイズなし | リスクあり |
| コンテキスト文字列に制御文字 | metadata.contextにANSI ESC | そのまま出力（端末汚染） | サニタイズなし | リスクあり |

## Design & Architecture Suggestions

- ContextIncludesの適用: 現状未使用のため、format_full/append_relationshipsにフラグ引数を追加し、ユーザが関係カテゴリの出力可否を制御できるようにする。例: format_full_with_flags(&self, indent, includes: ContextIncludes).
- 共通ロジック抽出: callsとcalled_byはほぼ同一ロジック。共通ヘルパー（例: append_call_list(label, pairs, indent)）にまとめて重複削減。
- Writerベースの出力: 大量データ時のパフォーマンス向上のため、String連結ではなくfmt::Writeやstd::io::Writeに直接書き出すAPIを追加。必要ならば両方提供（StringとWriter）。
- サニタイズ/エスケープ: ファイルパス・ドキュメント・contextの表示前にエスケープ（ASCIIのみ、ANSIコード除去、制御文字除去）を導入。
- 表示モードの分離: CLI向けテキスト、Markdown向け、JSON向けなど出力モードを戦略パターン化して拡張容易に。
- インデント管理の改善: write_multilineのextra_spacesとappend_*内の「"  - "」固定を抽象化し、テーマ/スタイル適用を容易に。

## Testing Strategy (Unit/Integration) with Examples

ユニットテスト観点:
- symbol_locationの単一行/複数行レンジ表示
- Displayの末尾改行除去
- append_metadataの可視性判定、ドキュメントプレビュー（2行＋...）
- write_multilineのインデント適用と行ごと改行
- calls/called_byのmetadata.line優先とcontext表示、None時の定義位置使用
- 空の関係でセクション非表示

注意: このチャンクにはcrate::Symbol/RelationshipMetadataの構築手段が現れないため、テストでは既存のテストヘルパーやモックを使用すること。

例（擬似テストコード、行番号不明）:

```rust
#[test]
fn symbol_location_formats_single_line() {
    // arrange
    let symbol = make_symbol("foo", "/path/file.rs", 9, 9); // 0-based lines
    let ctx = SymbolContext {
        symbol,
        file_path: "/path/file.rs".to_string(),
        relationships: SymbolRelationships::default(),
    };
    // act
    let s = SymbolContext::symbol_location(&ctx.symbol);
    // assert
    assert_eq!(s, "/path/file.rs:10"); // 0-based -> 1-based
}

#[test]
fn symbol_location_formats_range() {
    let symbol = make_symbol("bar", "/path/file.rs", 9, 19);
    let s = SymbolContext::symbol_location(&symbol);
    assert_eq!(s, "/path/file.rs:10-20");
}

#[test]
fn display_trims_trailing_newline() {
    let ctx = make_context_with_signature("fn x() {}", None);
    let out = format!("{}", ctx);
    assert!(!out.ends_with('\n'));
}

#[test]
fn metadata_shows_doc_preview_two_lines_with_ellipsis() {
    let doc = "Line1\nLine2\nLine3\nLine4";
    let ctx = make_context_with_doc(doc);
    let out = ctx.format_full("");
    assert!(out.contains("Doc: Line1 Line2..."));
}

#[test]
fn calls_use_callsite_line_when_available() {
    let (caller_ctx, callee_sym) = make_call_relation(Some(0), None); // call site at line 0
    let out = caller_ctx.format_full("");
    assert!(out.contains(&format!("{}:1", callee_sym.file_path))); // 1-based
}

#[test]
fn relations_sections_omitted_when_empty() {
    let ctx = SymbolContext {
        symbol: make_symbol("foo", "/path/file.rs", 0, 0),
        file_path: "/path/file.rs".into(),
        relationships: SymbolRelationships::default(), // all None
    };
    let out = ctx.format_full("");
    assert!(!out.contains("Implements:"));
    assert!(!out.contains("Implemented by"));
    assert!(!out.contains("Defines"));
    assert!(!out.contains("Calls"));
    assert!(!out.contains("Called by"));
}

// make_symbol/make_context_*はプロジェクト側のテストヘルパーを想定（このチャンクには現れない）
```

🧪 追加テスト提案:
- write_multilineのextra_spacesが想定どおりに効いているか
- Visibility::PrivateのときVisibility行が出ないこと
- metadata.contextが空文字列のとき、[]を出力しないこと

## Refactoring Plan & Best Practices

- 重複削減: calls/called_byのブロックをヘルパー関数へ抽出し、関数ポインタ/ラベルで切り替え。
- 出力の一貫性: writeln!マクロ利用に統一して可読性向上と末尾改行管理を簡略化。
- フラグ適用: ContextIncludesをformat_fullに適用したバリアントを追加し、柔軟な出力制御を提供。
- 文字列結合の効率化: String::with_capacityで概算容量を予約（関係件数や文字数を推定）。
- サニタイズ: エスケープ関数をユーティリティとして導入（ASCII化、制御コード除去、ANSIコードストリップ）。
- API拡張: format_full_to<W: fmt::Write>(&self, indent: &str, w: &mut W)を追加して巨大出力のコピーを削減。

ベストプラクティス:
- データと表示の分離（DTOとViewModelの概念導入）
- 出力をロケール/言語に依存しない定型文にし、上位レイヤでローカライズ
- テストでGolden File（期待出力）を用いて差分検出

## Observability (Logging, Metrics, Tracing)

- ロギング: 出力生成前後に情報ログを追加可能（件数、所要時間）。ただし本ファイルは純粋整形層のため、上位で計測する設計が望ましい。
- メトリクス: 
  - relationships総数、カテゴリ別件数（implements/implemented_by/defines/calls/called_by）
  - 署名行数、ドキュメント文字数
- トレーシング:
  - format_full呼び出し時のspanを作り、append_*各段の所要時間を計測
- サニタイズの観測:
  - エスケープ適用件数や除去された制御文字の統計（導入後）

## Risks & Unknowns

- Unknowns（このチャンクには現れない）
  - crate::Symbolの完全な仕様（フィールド型・メソッド契約）
  - RelationshipMetadataの全フィールド仕様（line/context以外）
  - Visibilityの列挙内容（Public, Crate, Protectedなどの具体的表示）
  - ID（symbol.id.value()）の型や意味体系
- リスク
  - 大量データ時のメモリ・CPU消費（単一String構築）
  - 表示汚染（制御文字、ANSIエスケープ）によるUX低下
  - ContextIncludes未使用による柔軟性不足
  - 他レイヤからの非同期並行利用時の期待（Send/Sync）は型実装に依存し未確認

以上により、本ファイルは「安全なRustによる表示整形」に特化しており、実運用に耐えるためには拡張（出力制御・サニタイズ・性能最適化）が有効です。