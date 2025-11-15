# symbol\mod.rs Review

## TL;DR

- 目的: ソースコード中の**シンボル**（関数・型・変数など）のメタデータを表現し、検索・インデックス向けに**CompactSymbol**へ変換/復元するためのコアロジック。
- 主な公開API: **Symbol**, **CompactSymbol**, **StringTable**, **Visibility**, **ScopeContext**, そして**Symbol::to_compact**, **CompactSymbol::to_symbol**, **StringTable::intern/get**。
- 複雑箇所: シンボル種別の**u8への直列化/復元**（固定テーブルによるmatch）、**StringTable**のC風ヌル終端管理、**#[repr(C, align(32))]**な固定サイズ設計。
- 重大リスク: 
  - Unicodeの**ドキュメント文字列切り詰め**での境界不一致によるパニック（fmt::Display）。
  - **FileIdのu16ダウンキャスト**によるIDの切り捨て/データ破損。
  - **StringTableのオフセットu32**のオーバーフロー（巨大データ時）。
  - **Option返しのサイレントNone**（詳細エラー不在）で復元失敗原因が不透明。
- 並行性: StringTableの**読み取りはSync**だが、**internは&mut self**が必要。共有時は**Mutex/RwLock**等の保護が必要。
- セキュリティ: 外部入力の直接実行はないが、**ログ出力**や**経路文字列の扱い**に配慮が必要（情報洩れ防止）。

## Overview & Purpose

このモジュールは、言語横断で抽出されたソースコード上のシンボルを表現するための基盤です。以下の目的を持ちます。

- **Symbol**: 豊富なメタデータ（名前、種別、ファイル位置、可視性、モジュールパス、ドキュメント、言語ID、スコープコンテキスト）を保持。
- **CompactSymbol**: インデックスやストレージ向けに**固定サイズ（32バイト）**でシリアライズ可能な軽量構造体。
- **StringTable**: シンボル名などを**ヌル終端バイト列**としてプールし、**オフセット（u32）**で参照することで重複を削減。
- 変換API（Symbol ⇄ CompactSymbol）により、**検索用の軽量表現**と**詳細メタデータ**の間を行き来可能。

この設計は、言語固有のロジックなしで共通インデックスを構築し、高速な検索/復元を実現するための核です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Module | context | pub | スコープ/コンテキスト関連の補助（このチャンクには現れない） | 不明 |
| Enum | Visibility | pub | シンボル可視性の表現（Public/Crate/Module/Private） | Low |
| Enum | ScopeContext | pub | 定義スコープの表現（Local/Parameter/ClassMember/Module/Package/Global） | Med |
| Struct | Symbol | pub | 豊富なシンボルメタデータの保持とビルダーAPI | Med |
| Struct | CompactSymbol | pub | 32バイトの固定レイアウトでの軽量表現 | Med |
| Struct | StringTable | pub | ヌル終端文字列プールとオフセット管理 | Med |
| Trait impl | fmt::Display for Symbol | pub | シンボルの人間向け表示（整形） | Low |

### Dependencies & Interactions

- 内部依存:
  - Symbol → CompactSymbol: `Symbol::to_compact`で`StringTable::intern`を使用し名前を登録後、CompactSymbolを生成。
  - CompactSymbol → Symbol: `CompactSymbol::to_symbol`で`StringTable::get`により名前を復元、`SymbolKind`へ`kind: u8`を**match**でマッピング。
  - CompactSymbol → StringTable: `from_symbol`は`string_table.offsets`を参照して既登録名のオフセット取得（未登録ならNone）。
  - Display実装: Symbolのフィールドを整形出力（doc文字列の先頭100バイトを切り出し）。

- 外部依存（表）:
  | 依存 | 用途 | 備考 |
  |------|------|------|
  | crate::types::{CompactString, FileId, Range, SymbolId, SymbolKind, compact_string} | 基本型とコンバータ | `FileId::new`, `SymbolId::new`はOption返却 |
  | crate::parsing::registry::LanguageId | 言語種別ID | Symbolのオプションフィールド |
  | serde::{Serialize, Deserialize} | シリアライズ | Visibility, ScopeContext（Symbolもderiveあり） |
  | std::fmt | Display実装 | 整形出力 |
  | std::collections::HashMap | StringTableのオフセット管理 | String→u32 |

- 被依存推定:
  - インデックス構築・検索エンジン（シンボル名検索、種類フィルタリング）。
  - 言語パーサ・抽出器（Symbol生成）。
  - ストレージ層（CompactSymbolのバイナリ保存/メモリマップ）。
  - UI/CLI（Symbolの表示やドキュメントプレビュー）。
  - これらは推定であり、このチャンクには具体的な呼び出し元は現れない。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| Visibility | enum Visibility { Public, Crate, Module, Private } | 可視性の表現 | O(1) | O(1) |
| ScopeContext | enum ScopeContext { Local{hoisted, parent_name, parent_kind}, Parameter, ClassMember, Module, Package, Global } | 定義スコープの表現 | O(1) | O(1) |
| Symbol::new | `pub fn new(id: SymbolId, name: impl Into<CompactString>, kind: SymbolKind, file_id: FileId, range: Range) -> Self` | 基本フィールドの初期化 | O(|name|) | O(|name|) |
| Symbol::new_with_scope | `pub fn new_with_scope(..., scope: ScopeContext) -> Self` | スコープ付き初期化 | O(|name|) | O(|name|) |
| Symbol::with_file_path | `pub fn with_file_path(self, file_path: impl Into<Box<str>>) -> Self` | ファイルパス設定 | O(|path|) | O(|path|) |
| Symbol::with_signature | `pub fn with_signature(self, signature: impl Into<Box<str>>) -> Self` | シグネチャ設定 | O(|sig|) | O(|sig|) |
| Symbol::with_doc | `pub fn with_doc(self, doc: impl Into<Box<str>>) -> Self` | ドキュメント設定 | O(|doc|) | O(|doc|) |
| Symbol::with_module_path | `pub fn with_module_path(self, path: impl Into<Box<str>>) -> Self` | モジュールパス設定 | O(|path|) | O(|path|) |
| Symbol::with_visibility | `pub fn with_visibility(self, visibility: Visibility) -> Self` | 可視性設定 | O(1) | O(1) |
| Symbol::with_scope | `pub fn with_scope(self, scope: ScopeContext) -> Self` | スコープ設定 | O(1) | O(1) |
| Symbol::with_language_id | `pub fn with_language_id(self, language_id: LanguageId) -> Self` | 言語ID設定 | O(1) | O(1) |
| Symbol::as_name | `pub fn as_name(&self) -> &str` | 名前参照 | O(1) | O(1) |
| Symbol::into_name | `pub fn into_name(self) -> CompactString` | 所有権移動で名前抽出 | O(1) | O(1) |
| Symbol::as_signature | `pub fn as_signature(&self) -> Option<&str>` | シグネチャ参照 | O(1) | O(1) |
| Symbol::as_doc_comment | `pub fn as_doc_comment(&self) -> Option<&str>` | ドキュメント参照 | O(1) | O(1) |
| Symbol::as_module_path | `pub fn as_module_path(&self) -> Option<&str>` | モジュールパス参照 | O(1) | O(1) |
| Symbol::to_compact | `pub fn to_compact(&self, string_table: &mut StringTable) -> CompactSymbol` | CompactSymbolへ変換（名前をintern） | 平均O(|name|) | 文字列長分増加 |
| StringTable::new | `pub fn new() -> Self` | 文字列テーブル初期化 | O(1) | O(1) |
| StringTable::intern | `pub fn intern(&mut self, s: &str) -> u32` | 文字列のプール化＋オフセット取得 | 平均O(|s|) | +(|s|+1) |
| StringTable::get | `pub fn get(&self, offset: u32) -> Option<&str>` | オフセットから文字列復元 | O(len_at_offset) | O(1) |
| CompactSymbol::from_symbol | `pub fn from_symbol(symbol: &Symbol, string_table: &StringTable) -> Option<Self>` | 既登録名からCompactSymbol生成 | 平均O(|name|) | O(1) |
| CompactSymbol::to_symbol | `pub fn to_symbol(&self, string_table: &StringTable) -> Option<Symbol>` | 文字列テーブルからSymbol復元 | O(len(name)) | O(|name|) |
| fmt::Display for Symbol | `impl fmt::Display for Symbol` | 人間向け整形出力 | O(1)〜O(|doc|) | O(1) |

以下、主要APIの詳細。

### Symbol::to_compact

1) 目的と責務
- Symbolを**コンパクトな固定レイアウト**に変換し、インデックス/保存に適した形へする。
- 同時に`StringTable::intern`で名前を登録（重複排除）。

2) アルゴリズム（ステップ）
- 名前を`string_table.intern(&self.name)`で登録し**オフセット**を得る。
- `SymbolKind`を`u8`へキャスト。
- `FileId.value()`を`u16`へダウンキャスト。
- `Range`各フィールドを対応する整数へ格納。
- `SymbolId.value()`を`u32`で格納。

3) 引数
| 名称 | 型 | 意味 |
|------|----|------|
| self | &Symbol | 変換元のシンボル |
| string_table | &mut StringTable | 名前をプールするテーブル |

4) 戻り値
| 型 | 説明 |
|----|------|
| CompactSymbol | 32バイトの軽量表現 |

5) 使用例
```rust
let mut table = StringTable::new();
let sym = Symbol::new(
    SymbolId::new(42).unwrap(),
    "foo",
    SymbolKind::Function,
    FileId::new(1).unwrap(),
    Range::new(1, 0, 1, 10),
);
let compact = sym.to_compact(&mut table);
```

6) エッジケース
- FileIdの値が`u16::MAX`を超えると**切り捨て**（データ破損）。
- `SymbolKind`の列挙順が変更されると復元時に**不整合**。
- StringTableが極端に巨大だと**offsetのu32オーバーフロー**。

### CompactSymbol::to_symbol

1) 目的と責務
- CompactSymbolを**高レベルなSymbol**へ復元する。

2) アルゴリズム
- `string_table.get(self.name_offset)`で名前を取得（ヌル終端まで）。
- `self.kind`（u8）を`match`で`SymbolKind`へマッピング。
- `SymbolId::new(self.symbol_id)?`、`FileId::new(self.file_id as u32)?`でIDを構築。
- `Range::new(...)`で範囲を生成。
- `visibility`はPrivate、`scope_context`/`language_id`はNoneに初期化（CompactSymbol非保持）。

3) 引数
| 名称 | 型 | 意味 |
|------|----|------|
| self | &CompactSymbol | 変換元 |
| string_table | &StringTable | 名前復元に使用 |

4) 戻り値
| 型 | 説明 |
|----|------|
| Option<Symbol> | 復元成功ならSome、失敗（offset/kind/ID不正）でNone |

5) 使用例
```rust
let restored = compact.to_symbol(&table).expect("restore symbol");
assert_eq!(restored.name.as_ref(), "foo");
```

6) エッジケース
- `name_offset`が不正/範囲外なら**None**。
- `kind`が未知値（14以上）なら**None**。
- `SymbolId::new`や`FileId::new`が**None**を返すケース（仕様依存）。

### CompactSymbol::from_symbol

1) 目的
- 既に`StringTable`へ登録済みの名前を前提に、**非破壊的に**CompactSymbol生成。

2) アルゴリズム
- `string_table.offsets.get(symbol.name.as_ref())`でオフセット探索。
- 見つからなければ**None**返却。
- `Symbol::to_compact`同様に各フィールドを詰める。

3) 引数
| 名称 | 型 | 意味 |
|------|----|------|
| symbol | &Symbol | 変換元 |
| string_table | &StringTable | オフセット参照 |

4) 戻り値
| 型 | 説明 |
|----|------|
| Option<CompactSymbol> | 名前未登録ならNone |

5) 使用例
```rust
let mut table = StringTable::new();
let _ = table.intern("foo"); // 先に登録
let cs = CompactSymbol::from_symbol(&sym, &table).unwrap();
```

6) エッジケース
- internが未実行だと**None**。
- 上記の`to_compact`と同様のID/範囲の前提条件。

### StringTable::intern / get

1) 目的
- intern: 文字列の**重複を排除**し、**オフセット**を返す。
- get: オフセットから**ヌル終端まで**を文字列として返す。

2) アルゴリズム
- intern: `offsets`に存在すれば再利用、なければ`data`末尾に`bytes + 0`を追加し`offsets`へ登録。
- get: `start = offset`から`0`（ヌル）を探索し、その範囲をUTF-8として返す。

3) 引数/戻り値
- intern: `(&mut self, s: &str) -> u32`
- get: `(&self, offset: u32) -> Option<&str>`

4) 使用例
```rust
let mut table = StringTable::new();
let off = table.intern("hello"); // 1
assert_eq!(table.get(off), Some("hello"));
```

5) エッジケース
- `offset >= data.len()`でNone。
- ヌルが見つからない（理論上不整合）場合はNone。
- 極端なサイズで`offset`が`u32::MAX`超過のリスク。

### Data Contracts（主な構造体の不変条件）

- Symbol
  - `id`, `file_id`は有効なID（`SymbolId::new`, `FileId::new`で生成される前提）。
  - `range`は「開始 ≤ 終了」が期待されるが、このチャンクでは検証なし（不明）。
  - `file_path`, `signature`, `doc_comment`, `module_path`は任意。
  - `scope_context`, `language_id`は現在Optional（移行期間中）であり、CompactSymbolへは未出力。

- CompactSymbol
  - 32バイト固定（`#[repr(C, align(32))]`）。size/alignはテストで検証済み。
  - `file_id`は`u16`で格納（`FileId`の`u32`値をダウンキャストする設計上の制約）。
  - `kind`は`u8`で、0〜13の範囲に限定（このチャンクのmatch準拠）。

- StringTable
  - `data`は先頭に0を持ち、各文字列は`bytes`＋終端`0`。
  - `offsets`は文字列→先頭オフセットの対応。

## Walkthrough & Data Flow

以下はSymbolの生成からCompactSymbolでの保存、復元までのフローです。

- 作成フェーズ
  1. Parserが`Symbol::new`や`new_with_scope`でSymbolを構築。
  2. 任意で`with_*`ビルダーで追加メタ情報を設定。

- コンパクト化
  3. `Symbol::to_compact(&mut string_table)`が呼ばれ、`string_table.intern(name)`で名前を登録し、`CompactSymbol`を生成。

- 復元
  4. ストレージから`CompactSymbol`を読み出し、`CompactSymbol::to_symbol(&string_table)`で`name_offset`から名前を取得。
  5. `kind: u8`を`SymbolKind`へmatchで復元、`id/file_id/range`も再構築。

Mermaid図（条件分岐が多いkind復元ロジックの主要分岐）:

```mermaid
flowchart TD
  A[kind: u8] -->|0| K0[SymbolKind::Function]
  A -->|1| K1[SymbolKind::Method]
  A -->|2| K2[SymbolKind::Struct]
  A -->|3| K3[SymbolKind::Enum]
  A -->|4| K4[SymbolKind::Trait]
  A -->|5| K5[SymbolKind::Interface]
  A -->|6| K6[SymbolKind::Class]
  A -->|7| K7[SymbolKind::Module]
  A -->|8| K8[SymbolKind::Variable]
  A -->|9| K9[SymbolKind::Constant]
  A -->|10| K10[SymbolKind::Field]
  A -->|11| K11[SymbolKind::Parameter]
  A -->|12| K12[SymbolKind::TypeAlias]
  A -->|13| K13[SymbolKind::Macro]
  A -->|その他| E[None (失敗)]
```

上記の図は`CompactSymbol::to_symbol`関数のkind復元分岐を示す（このチャンクに相当）。

## Complexity & Performance

- StringTable::intern
  - 時間: 平均O(|s|)（ハッシュ計算＋コピー）／最悪時HashMap再配置あり。
  - 空間: `|s| + 1`バイト（ヌル終端）＋HashMapエントリ。

- StringTable::get
  - 時間: O(文字列長)（ヌル終端探索）＋UTF-8検証。
  - 空間: O(1)。

- Symbol::to_compact
  - 時間: O(|name|)（intern）＋各フィールド詰めはO(1)。
  - 空間: O(1)（CompactSymbolの生成）＋StringTable側で増加。

- CompactSymbol::to_symbol
  - 時間: O(文字列長)（get）＋kindマッチはO(1)。
  - 空間: O(|name|)（CompactStringへの取り込み）＋小固定サイズのSymbol生成。

ボトルネック・スケール限界:
- 大規模コードベースでは**StringTable**が巨大化しu32オフセットが飽和する可能性。
- `get`のヌル探索は**線形**であり、ホットパスで多用されると費用が増す。
- `file_id`の`u16`制約により、**65536ファイル超**のプロジェクトで情報が壊れる。

実運用負荷要因:
- I/O/ストレージ: CompactSymbolは32バイト固定で**キャッシュ効率**は良いが、文字列は別領域で**ランダムアクセス**。
- ネットワーク: 文字列テーブルのサイズに依存（転送コスト）。
- CPU: ハッシュ計算とUTF-8検証はデータ量に比例。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| Unicode境界でのdoc切り詰め | `doc = "😀..."`（100バイト未満/以上） | 文字境界で安全に切り詰め | `doc[..100]`でバイト境界スライス | バグ（UTF-8境界でpanicの可能性） |
| name未internのfrom_symbol | `CompactSymbol::from_symbol(&sym, &table)`（未登録） | エラー通知 | `None`返却 | 改善余地（Resultで詳細） |
| file_idダウンキャスト | `FileId.value() > 65535` | エラー/安全な拒否 | `as u16`で切り捨て | バグ（サイレント破損） |
| StringTableオフセット範囲外 | `get(999)`（data未満） | None | `None`返却 | OK |
| kind不明値 | `kind = 255` | エラー通知 | `None`返却 | OKだが詳細不足 |
| SymbolId/FileId不正 | `SymbolId::new(0)`等 | エラー通知 | `None`伝播 | OKだが詳細不足 |
| Range不整合 | `start > end` | 検証・拒否 | 検証なし | 不明（Range::new側の仕様次第） |
| 秘密情報のログ漏洩 | `doc_comment/signature`に秘密含む | マスク/省略 | Displayで全文/先頭部分を出力 | リスク（マスキングなし） |
| 並行アクセス | 複数スレッドからintern | Mutexなどで保護 | &mut selfが必要 | 設計で保護必要 |

セキュリティチェックリスト:
- メモリ安全性: unsafe未使用（このチャンク）。ただし`doc[..100]`はUTF-8境界問題で**panic**リスク。
- インジェクション: SQL/Command/Path traversalなし。`file_path`は表示用途のみだが、ログ/表示で**情報漏洩**に注意。
- 認証・認可: 該当なし。
- 秘密情報: ハードコード秘密なし。Displayが**doc/signature**をそのまま出力するため、**ログ漏洩**に留意。
- 並行性: `StringTable::intern`は**排他**が必要。`get`は読み取りで安全（Sync）。共有時は**Mutex/RwLock**推奨。

Rust特有の観点:
- 所有権: `Symbol::into_name`で`self`を消費し`name`の所有権を移動（関数: into_name）。
- 借用: `to_compact`は`&self`と`&mut StringTable`の同時借用。Rustの借用規則で**データ競合防止**。
- ライフタイム: 明示ライフタイム不要（全て所有型/Copy）。
- unsafe境界: なし。
- Send/Sync: `StringTable`は読み取り時`Sync`。変更時は`&mut`が必要で、**多スレッドでは明示的な同期**が必要。
- await境界/非同期: 該当なし。
- エラー設計: Optionで失敗を返す設計。**原因特定が難しい**ため、`Result<_, Error>`への改善が望ましい。
- panic箇所: `fmt::Display`の`doc[..100]`で**境界不一致panic**可能。
- エラー変換: `SymbolId::new`/`FileId::new`の`Option`に依存。`TryFrom`や`thiserror`等の導入で改善可能。

## Design & Architecture Suggestions

- CompactSymbol拡張:
  - **file_idをu32**へ拡張（または2フィールド構成）し、ダウンキャストを廃止。
  - `flags`を活用して**visibility/scope/language_id**のビット保持を検討（現状常に0）。
  - `kind`の復元は`num_enum::TryFromPrimitive`などで安定性を高め、列挙順変更に**強くする**。

- 文字列テーブル改善:
  - オフセットを**u64**へ拡張可能にし、巨大プロジェクトの安全性向上。
  - `get`のヌル探索を最適化（例えば別途長さテーブルを持つ、または**LFH**配置）。

- エラー設計:
  - `CompactSymbol::{from_symbol,to_symbol}`を`Result<_, SymbolError>`にする。原因（offset不正/kind不明/ID不正）を明示。
  - `Symbol::to_compact`で**FileIdのオーバーフロー検知**を追加（`u16::try_from`を使い`Result`化）。

- Displayの安全化:
  - ドキュメント切り詰めは**char境界**で行う（`doc.chars().take(n).collect::<String>()`）。
  - 機密情報を含む可能性のある`signature/doc`は**ログ側でマスク**の設定を可能に。

- API一貫性:
  - Builderの`with_*`群は`&mut self`にする設計も検討（メソッドチェーンは現行で可だが所有権移動コストに注意）。

## Testing Strategy (Unit/Integration) with Examples

追加テスト案（ユニット）:

- Unicodeドキュメント境界
```rust
#[test]
fn test_display_doc_truncation_unicode_safe() {
    // 想定修正後：charsベースでtruncate
    let doc = "😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀😀"; // 20 chars, > 100 bytes
    let s = Symbol::new(SymbolId::new(1).unwrap(), "x", SymbolKind::Function, FileId::new(1).unwrap(), Range::new(1,0,1,1))
        .with_doc(doc);
    let out = format!("{s}");
    assert!(out.contains("Doc:")); // panicしないこと
}
```

- file_idオーバーフロー検知
```rust
#[test]
fn test_file_id_overflow_detect() {
    let mut table = StringTable::new();
    // 想定修正後：to_compactがResultを返す
    let sym = Symbol::new(SymbolId::new(1).unwrap(), "x", SymbolKind::Function, FileId::new(70000).unwrap(), Range::new(1,0,1,1));
    // assert!(matches!(sym.to_compact(&mut table), Err(_)));
}
```

- from_symbolで未intern
```rust
#[test]
fn test_from_symbol_without_intern() {
    let table = StringTable::new();
    let sym = Symbol::new(SymbolId::new(1).unwrap(),"x",SymbolKind::Function,FileId::new(1).unwrap(),Range::new(1,0,1,1));
    assert!(CompactSymbol::from_symbol(&sym, &table).is_none());
}
```

- StringTable巨大化の健全性
```rust
#[test]
fn test_string_table_large_offsets() {
    let mut table = StringTable::new();
    for i in 0..100_000 {
        let s = format!("name_{i}");
        let _ = table.intern(&s);
    }
    // u32範囲内でgetが成功すること
    let off = table.intern("final");
    assert_eq!(table.get(off), Some("final"));
}
```

- kind変換の健全性（既存テストを補完）
```rust
#[test]
fn test_invalid_kind_fails() {
    let mut table = StringTable::new();
    let sym = Symbol::new(SymbolId::new(1).unwrap(), "x", SymbolKind::Function, FileId::new(1).unwrap(), Range::new(1,0,1,1));
    let mut cs = sym.to_compact(&mut table);
    cs.kind = 255;
    assert!(cs.to_symbol(&table).is_none());
}
```

インテグレーションテスト案:
- 大規模なファイル群でのインデックス（StringTable肥大時の**性能と正確性**）。
- 異言語混在（`language_id`フィルタリングの挙動：このチャンクではロジック不明のため「不明」）。

## Refactoring Plan & Best Practices

- 変換APIのResult化:
  - `Symbol::to_compact(&mut StringTable) -> Result<CompactSymbol, SymbolError>`
  - `CompactSymbol::to_symbol(&StringTable) -> Result<Symbol, SymbolError>`
  - 具体的な`SymbolError`（KindUnknown, OffsetOutOfRange, FileIdOverflow, IdInvalid など）を定義。

- kindマッピングの安定化:
  - `#[repr(u8)]`を`SymbolKind`に付与の上、`TryFrom<u8>`を実装して**安全に**復元。
  - あるいは`num_enum::TryFromPrimitive`採用。

- file_idの拡張/検証:
  - `u16::try_from(file_id.value())?`で明示的エラー化、もしくはCompactSymbolのフィールドを`u32`へ拡張。

- Displayの安全/設定可能性:
  - Unicode安全な切り詰め関数を導入。
  - 出力行数/長さの制限、機密フィールドのマスクオプション。

- StringTable最適化:
  - 文字列長のキャッシュ（開始オフセット→長さのマップ）で`get`を**O(1)**化。
  - オフセット型の拡張（u64）とフェイルファスト検出。

- BuilderAPIの整合性:
  - `with_*`の連鎖時に過剰なムーブを避けるため`&mut self`版も提供。

## Observability (Logging, Metrics, Tracing)

- ロギング:
  - CompactSymbol復元失敗（kind未知、offset不正、ID不正）で**警告ログ**。
  - StringTableの**サイズ/文字列数**の定期ログ出力（上限超過検知）。

- メトリクス:
  - `string_table.intern`呼び出し回数、重複率（再利用比率）、総バイト数。
  - 復元失敗件数、kind未知の発生率。

- トレーシング:
  - バルク変換（Symbol→CompactSymbol）処理でスパンを張り、**ホットスポット特定**。

## Risks & Unknowns

- Unknowns:
  - `crate::types::SymbolKind`の**定義順**（変動すると復元に影響）。
  - `SymbolId::new`, `FileId::new`の**不正条件**（このチャンクでは不明）。
  - `Range::new`の**検証仕様**（開始/終了の正当性チェックの有無は不明）。
  - `context`モジュールの詳細（このチャンクには現れない）。

- Risks:
  - Unicode切り詰めの**panic**（Display）。
  - `file_id`ダウンキャストによる**データ破損**。
  - StringTableの**u32オフセット限界**到達。
  - Option返却による**サイレント失敗**でデバッグ困難。
  - `SymbolKind as u8`の**将来互換性**問題。

以上の点を踏まえ、APIのエラー報告の明確化、可搬性/安全性の高い直列化設計への移行、文字列処理のUnicode安全化が優先改善項目です。