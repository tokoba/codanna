# storage/symbol_cache.rs Review

## TL;DR

- 目的: メモリマップを用いたハッシュバケット形式のシンボル高速ルックアップキャッシュ。Tantivyを介さない名前検索のゼロオーバヘッド化。
- 主要公開API: SymbolHashCache::{new, open, path, symbol_count, lookup_by_name, lookup_candidates, build_from_symbols} と ConcurrentSymbolCache::{new, lookup_by_name, lookup_candidates}。
- 複雑箇所: キャッシュファイルのバイナリレイアウト（ヘッダ＋バケットオフセット＋バケットデータ）と安全な読み取り境界チェック、Windowsファイルロック回避ロジック。
- 重大リスク: バケット数0での除算パニック、バケットオフセットがファイルサイズ外の場合のパニック、Windowsロック検出の文字列依存、破損ファイルに対する検証不足。
- Rust安全性: unsafeブロック（memmap2::Mmapのmap呼び出し）あり。読み取りは境界チェックありだがオフセットの妥当性検証不足。
- 並行性: 読み取りはparking_lot::RwLockで保護。書き込み（build）は別途実行であり、同時更新時の整合性は外部で担保する必要あり。
- パフォーマンス: FNV-1aハッシュ＋256バケットで平均O(1)探索。ただし最悪ケースは1バケットへ集中しO(n)。

## Overview & Purpose

このモジュールは、シンボル名に対する高速検索を可能にする、メモリマップを用いたハッシュベースのキャッシュを提供します。目的は、フルテキストインデックス（例：Tantivy）を通さずに単純な「名前→ID」問い合わせを極小の遅延で処理することです。キャッシュファイルは以下の構造を持ち、メモリマップされたバイト列から直接読み取り、ヒープ割り当てを避けます。

- ヘッダ（32バイト）
- バケットオフセット配列（バケット数×8バイト）
- 各バケットのデータ（件数＋エントリ配列）

検索は、FNV-1aで名前をハッシュ化し、バケットを決定して線形走査することで実現します。衝突は「候補列挙（lookup_candidates）」で対応し、呼び出し側で最終的な照合を行えます。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Const | MAGIC_BYTES | private | ファイル識別用マジック | Low |
| Const | VERSION | private | フォーマットバージョン | Low |
| Const | DEFAULT_BUCKET_COUNT | private | デフォルトのバケット数（2の冪） | Low |
| Const | HEADER_SIZE | private | ヘッダサイズ固定値（32） | Low |
| Const | MAX_BUCKET_SIZE | private | バケット最大推奨サイズ | Low |
| Func | fnv1a_hash | private | FNV-1aハッシュ計算 | Low |
| Struct | CacheEntry | private | キャッシュファイル内エントリ（24バイト固定） | Med |
| Struct | SymbolHashCache | pub | キャッシュの読み取り・作成（メモリマップ管理） | Med |
| Struct | ConcurrentSymbolCache | pub | RwLockによるスレッドセーフラッパー | Low |

### Dependencies & Interactions

- 内部依存:
  - SymbolHashCache::lookup_by_name → fnv1a_hash, self.mmap, self.bucket_offsets
  - SymbolHashCache::lookup_candidates → fnv1a_hash, self.mmap, self.bucket_offsets
  - SymbolHashCache::build_from_symbols → CacheEntry::from_symbol, fnv1a_hash, ファイルI/O
  - CacheEntry::from_symbol → crate::Symbol, SymbolId, range, kind
- 外部依存（クレート/モジュール）:

| 依存 | 用途 |
|------|------|
| memmap2::{Mmap, MmapOptions} | メモリマップの生成（unsafe） |
| parking_lot::RwLock | 低オーバヘッドの読取重視ロック |
| std::fs::{File, OpenOptions} | ファイル読み書き |
| std::io::{self, Write} | I/Oエラー型、書き込み |
| std::path::{Path, PathBuf} | パス管理 |
| std::sync::Arc | 共有所有権 |
| crate::{Symbol} | キャッシュ構築入力 |
| crate::types::SymbolId | API戻り値（Optionで検証） |

- 被依存推定:
  - インデクサ（ビルド時に build_from_symbols を呼び出す）
  - 検索API層（名前から SymbolId を高速解決し、詳細は別ストアに照会）
  - プロジェクト内の補助キャッシュ管理コンポーネント（ConcurrentSymbolCache）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| SymbolHashCache::new | fn new(path: impl AsRef<Path>) -> io::Result<Self> | キャッシュオブジェクトの作成（未マップ） | O(1) | O(1) |
| SymbolHashCache::open | fn open(path: impl AsRef<Path>) -> io::Result<Self> | 既存キャッシュのメモリマップとメタ読み込み | O(buckets) | O(buckets) |
| SymbolHashCache::path | fn path(&self) -> &Path | キャッシュファイルへの参照取得 | O(1) | O(1) |
| SymbolHashCache::symbol_count | fn symbol_count(&self) -> usize | シンボル総数の取得 | O(1) | O(1) |
| SymbolHashCache::lookup_by_name | fn lookup_by_name(&self, name: &str) -> Option<SymbolId> | 名前一致の高速検索（最初の一致を返す） | O(k) | O(1) |
| SymbolHashCache::lookup_candidates | fn lookup_candidates(&self, name: &str, max_candidates: usize) -> Vec<SymbolId> | 名前ハッシュ一致候補を最大数まで列挙 | O(min(k, max_candidates) + prefix) | O(max_candidates) |
| SymbolHashCache::build_from_symbols | fn build_from_symbols<'a>(path: impl AsRef<Path>, symbols: impl Iterator<Item = &'a Symbol>) -> io::Result<()> | シンボル列からキャッシュファイル生成 | O(n + buckets) | O(n + buckets) |
| ConcurrentSymbolCache::new | fn new(cache: SymbolHashCache) -> Self | スレッドセーフラッパーの作成 | O(1) | O(1) |
| ConcurrentSymbolCache::lookup_by_name | fn lookup_by_name(&self, name: &str) -> Option<SymbolId> | 共有ロック下での高速検索 | O(k) | O(1) |
| ConcurrentSymbolCache::lookup_candidates | fn lookup_candidates(&self, name: &str, max_candidates: usize) -> Vec<SymbolId> | 共有ロック下での候補列挙 | O(min(k, max_candidates)) | O(max_candidates) |

用語補足:
- k: 対象バケット内のエントリ数
- n: 全シンボル数
- buckets: バケット数（デフォルト256）
- prefix: バケットの先頭で件数（4バイト）を読む固定コスト

各API詳細:

1) SymbolHashCache::new
- 目的と責務: パスを受け取り、未マップ状態のキャッシュオブジェクトを初期化します。（行番号: 不明）
- アルゴリズム:
  - PathBufに変換
  - mmap=None、bucket_count=DEFAULT_BUCKET_COUNT、symbol_count=0、bucket_offsets=空で初期化
- 引数:

| 名前 | 型 | 説明 |
|------|----|------|
| path | impl AsRef<Path> | キャッシュファイルのパス |

- 戻り値:

| 型 | 説明 |
|----|------|
| io::Result<Self> | 成功時に初期化済みインスタンス |

- 使用例:
```rust
use std::path::PathBuf;
let cache = SymbolHashCache::new(PathBuf::from("sym.cache"))?;
```
- エッジケース:
  - パスが不正: PathBuf生成は失敗しません。I/Oは行いません。

2) SymbolHashCache::open
- 目的と責務: 既存ファイルを開き、メモリマップし、ヘッダとバケットオフセットを読み込みます。（行番号: 不明）
- アルゴリズム（主要ステップ）:
  1. File::open
  2. unsafe { MmapOptions::new().map(&file)? }
  3. ヘッダ長チェック（HEADER_SIZE以上）
  4. マジック一致検証（"SYMC"）
  5. バージョン一致検証（VERSION）
  6. bucket_count, symbol_count読み取り
  7. bucket_offsets配列（bucket_count×u64）読み取り
  8. 構造体にセット
- 引数:

| 名前 | 型 | 説明 |
|------|----|------|
| path | impl AsRef<Path> | 既存キャッシュファイルのパス |

- 戻り値:

| 型 | 説明 |
|----|------|
| io::Result<Self> | 成功時にメモリマップ済みインスタンス |

- 使用例:
```rust
let cache = SymbolHashCache::open("sym.cache")?;
let id = cache.lookup_by_name("foo");
```
- エッジケース:
  - ヘッダ不足、マジック不一致、バージョン不一致 → InvalidDataエラーで早期終了
  - bucket_count=0やオフセット範囲外 → 現実装では検証不足。下記「Edge Cases」を参照。

3) SymbolHashCache::path
- 目的と責務: 設定済みのパス参照を返す。（行番号: 不明）
- 引数/戻り値は表通り。
- 使用例:
```rust
let p: &std::path::Path = cache.path();
```
- エッジケース: なし。

4) SymbolHashCache::symbol_count
- 目的と責務: ヘッダから得たシンボル総数を返す。（行番号: 不明）
- 使用例:
```rust
println!("symbols: {}", cache.symbol_count());
```

5) SymbolHashCache::lookup_by_name
- 目的と責務: 名前ハッシュで該当バケットを走査し最初に一致したSymbolIdを返す。（行番号: 不明）
- アルゴリズム:
  1. self.mmapがSomeでなければNone
  2. FNV-1aでname_hash計算
  3. bucket_idx = (name_hash as usize) % self.bucket_count
  4. bucket_start = bucket_offsets[bucket_idx]
  5. bucket_end = 次のバケットオフセット or mmap.len()
  6. エントリ数（先頭u32）を読む（境界チェックあり）
  7. entry_count回、各エントリのname_hash（u64）を比較、一致したらsymbol_id（u32）を読みSymbolId::newで返す
- 引数:

| 名前 | 型 | 説明 |
|------|----|------|
| name | &str | シンボル名 |

- 戻り値:

| 型 | 説明 |
|----|------|
| Option<SymbolId> | 先頭一致が見つかればSome、なければNone |

- 使用例:
```rust
if let Some(id) = cache.lookup_by_name("Foo") {
    // 追加情報は別ストアで照合
}
```
- エッジケース:
  - self.bucket_count==0で %0 パニック
  - bucket_offsetsが不正でbucket_end > mmap.len()かつbucket_start < bucket_end → その後のmmap[pos]インデックスでパニックの可能性

6) SymbolHashCache::lookup_candidates
- 目的と責務: 名前ハッシュ一致の候補SymbolIdを最大max_candidates個返す。（行番号: 不明）
- アルゴリズムはlookup_by_nameと同様だが、複数収集し、SymbolId::newのNoneはスキップ、最大件数で打ち切り。
- 引数:

| 名前 | 型 | 説明 |
|------|----|------|
| name | &str | シンボル名 |
| max_candidates | usize | 収集上限 |

- 戻り値:

| 型 | 説明 |
|----|------|
| Vec<SymbolId> | 候補リスト（0〜max_candidates） |

- 使用例:
```rust
let cands = cache.lookup_candidates("Foo", 5);
for id in cands { /* 照合 */ }
```
- エッジケース: lookup_by_nameに同じ。

7) SymbolHashCache::build_from_symbols
- 目的と責務: シンボル列からファイルを構築（全バケット再配置、ヘッダとオフセット・データを書き出す）。（行番号: 不明）
- アルゴリズム:
  1. DEFAULT_BUCKET_COUNTのVec<Vec<CacheEntry>>を準備
  2. 各SymbolをCacheEntryに変換、name_hashでバケット分配
  3. ヘッダ＋オフセット配列の開始オフセットから各バケットの開始位置を算出
  4. Windowsのファイルロックエラー（os error 1224）に遭遇した場合、削除とリトライ（最大3回）
  5. ヘッダ（MAGIC, VERSION, bucket_count, symbol_count, reserved）を書き込み
  6. バケットオフセット配列を書き込み
  7. 各バケットの件数とCacheEntryフィールドを順次書き込み
  8. sync_allでフラッシュ
- 引数:

| 名前 | 型 | 説明 |
|------|----|------|
| path | impl AsRef<Path> | 出力先 |
| symbols | impl Iterator<Item=&'a Symbol> | 入力シンボル列 |

- 戻り値:

| 型 | 説明 |
|----|------|
| io::Result<()> | 成否 |

- 使用例:
```rust
SymbolHashCache::build_from_symbols("sym.cache", symbols.iter())?;
let cache = SymbolHashCache::open("sym.cache")?;
```
- エッジケース:
  - Windowsロックで削除を試みるが、パスが重要ファイルを指す場合のリスク（削除）
  - 書き込み途中の障害→部分的ファイルはopenでInvalidDataを招きうる

8) ConcurrentSymbolCache::new / lookup_* 系
- 目的と責務: SymbolHashCacheをArc<RwLock<_>>で包み、同時読取を安全に提供。（行番号: 不明）
- 使用例:
```rust
let cc = ConcurrentSymbolCache::new(cache);
let id = cc.lookup_by_name("Foo");
```
- エッジケース: 同時書き込み操作は設計外。読み取りのみ。

データ契約（ファイルフォーマット）:
- ヘッダ（32バイト）
  - [0..4): MAGIC_BYTES="SYMC"
  - [4..8): VERSION (u32, LE)
  - [8..12): bucket_count (u32, LE)
  - [12..20): symbol_count (u64, LE)
  - [20..32): reserved (12バイトゼロ)
- バケットオフセット配列
  - bucket_count個のu64（LE）。各値はファイル先頭からのバイトオフセットで、当該バケットの「件数u32」が置かれる位置。
- バケットデータ
  - 先頭u32: entry_count（エントリ数）
  - 続くentry_count×CacheEntry（24バイト固定）

CacheEntryのレイアウト（repr(C), SIZE=24）:
- symbol_id: u32
- name_hash: u64
- file_id: u32
- line: u32
- column: u16
- kind: u8
- _padding: u8

## Walkthrough & Data Flow

- build_from_symbols:
  - 入力: Iterator<&Symbol>
  - 変換: CacheEntry::from_symbolでname_hash等を埋める
  - 分配: name_hash % DEFAULT_BUCKET_COUNTでバケットへ
  - オフセット計算: ヘッダ＋オフセット表の後から各バケットの開始位置を順次加算
  - 書き出し: ヘッダ→オフセット表→各バケット（件数＋エントリ列）
  - 出力: 完了後sync_all

- open:
  - 入力: ファイルパス
  - 処理: メモリマップ→ヘッダ検証→bucket_offsets読み取り
  - 出力: SymbolHashCache（mmap=Some）

- lookup_by_name / lookup_candidates:
  - 入力: name
  - 処理:
    - name_hash計算
    - bucket_idx算出
    - bucket_start/bucket_end境界決定
    - 件数読み取り→エントリ線形走査→name_hash一致時にsymbol_id抽出、候補追加（lookup_candidatesでは上限で打ち切り）
  - 出力: Option<SymbolId> or Vec<SymbolId>

- ConcurrentSymbolCache:
  - データ流: RwLockのread()でSymbolHashCacheにアクセス、lookup_*を委譲

### Mermaidフローチャート（Windowsロック回避ロジック）

```mermaid
flowchart TD
  A[OpenOptions::open(path)] -->|Ok(file)| B[write header & data]
  A -->|Err(e)| C{cfg(windows) && os error 1224?}
  C -->|Yes| D[attempts += 1; if path.exists() remove_file(path)]
  D --> E[sleep(100ms); retry]
  C -->|No| F[return Err(e)]
  E --> A
  B --> G[file.sync_all(); return Ok(())]
```

上記の図は`build_from_symbols`関数のファイルオープン・リトライの主要分岐を示す（行番号: 不明）。

## Complexity & Performance

- build_from_symbols:
  - 時間: O(n + buckets)。n件のハッシュ計算と分配、順次書き出し。
  - 空間: O(n + buckets)。各バケットのVec蓄積。
  - ボトルネック: ハッシュ分配でバケットに偏りがある場合、単一バケットが巨大化。書き込みはシーケンシャルで高速。

- open:
  - 時間: O(buckets)（オフセット配列読み取り）
  - 空間: O(buckets)（bucket_offsets保持）

- lookup_by_name:
  - 時間: 平均O(1)（均等分布前提）、最悪O(n)（全衝突）。実際はO(k)。
  - 空間: O(1)
  - I/O: メモリマップなのでページフォルト以外I/Oなし。CPUキャッシュヒット率に影響。

- lookup_candidates:
  - 時間: O(min(k, max_candidates))
  - 空間: O(max_candidates)

スケール限界:
- DEFAULT_BUCKET_COUNT=256は大規模プロジェクトではバケット平均が大きくなり線形走査コストが増加。動的リサイズや可変バケット数の採用が望ましい。
- name_hashのみに基づく一致は偽陽性を生みうるため、呼び出し側で名前照合が必要（現設計に沿った前提）。

## Edge Cases, Bugs, and Security

セキュリティ/堅牢性チェックリスト結果:
- メモリ安全性:
  - unsafeブロック: memmap2::MmapOptions::map（open内、行番号: 不明）。ファイルが他プロセスによりトランケートされるとSIGBUS等の危険。オフセット検証不足によりmmapスライスインデックスでパニックの可能性。
  - 境界チェック: bucket_endとの比較はあるが、bucket_startがmmap.len()を超えていても検出できない場合がある。
  - 整数演算: bucket_idx = (hash as usize) % bucket_count で bucket_count==0 の場合パニック。
  - u64→usizeキャスト: 32bit環境ではオフセットがusizeに収まらないリスク（パニックの可能性）。
- インジェクション:
  - SQL/Command/Path traversal: 該当なし。ただし build_from_symbols でロック回避時に remove_file(path) を実行するため、pathが意図しない重要ファイルだと破壊的（権限・検証が必要）。
- 認証・認可:
  - 該当なし。ファイルアクセス権限はOSに依存。
- 秘密情報:
  - ハードコード秘密なし。eprintlnでファイル削除メッセージを出すが秘密情報は出さない。
- 並行性:
  - RwLockで読み取りは安全。書き込み（build）と同時に open/lookup が走る場合の整合性は外部で要調整。メモリマップの再マップ機能は未提供。

エッジケース詳細:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| ヘッダ不足 | ファイル長<32 | Err(InvalidData) | openで検証あり | 安全 |
| マジック不一致 | "ABCD" | Err(InvalidData) | openで検証あり | 安全 |
| バージョン不一致 | VERSION≠1 | Err(InvalidData) | openで検証あり | 安全 |
| bucket_count=0 | ヘッダ8..12=0 | Errまたは安全な早期終了 | 検証なし（lookupで%0パニック） | バグ |
| バケットオフセット>ファイル長 | offsets[i] > mmap.len() | Errまたはスキップ | 検証なし（インデックスでパニック可能） | バグ |
| バケットオフセット非単調 | offsets[i] > offsets[i+1] | Err | 検証なし（bucket_start>bucket_endで件数読み取り前に安全終了するが、他ケースで危険） | 欠落 |
| 32bit環境でu64→usize溢れ | offsetsが大 | Err | 検証なし | 欠落 |
| Windowsロック検出 | os error 1224 | リトライ＆必要時削除 | 実装あり（文字列判定） | 改善余地 |
| 名前ハッシュ衝突 | 同名/異名で同ハッシュ | 複数候補返却→上位照合 | lookup_candidatesで対応 | 安全（設計前提） |
| シンボルID無効 | SymbolId::newがNone | スキップ or None返却 | 対応あり | 安全 |
| 最終バケット終端 | 最終バケットのend=mmap.len() | 正常動作 | 実装あり | 安全 |

## Design & Architecture Suggestions

- バケット数とオフセット検証を追加
  - open時に:
    - bucket_count > 0 のチェック（ゼロならInvalidData）
    - 各bucket_offsets[i]がHEADER_SIZE + bucket_count*8 以上であること
    - bucket_offsets[i] ≤ mmap.len() かつ単調非減少であること
    - 32bit環境を考慮し、u64→usizeに収まるか検証
- フォーマット拡張のためのヘッダ検証強化
  - reserved領域の将来拡張に備えたフィーチャフラグ等
- バケット数の動的決定
  - build時にnとロードファクタから最適バケット数（2の冪）を選択し、open時もその値を利用
  - 2の冪が保証されるなら bucket_idx = (hash as usize) & (bucket_count - 1) に置換し高速化
- Windowsロック検出の改善
  - e.raw_os_error() == Some(1224) で判定（文字列依存を避ける）
  - remove_fileの前に対象ファイルが「キャッシュ領域」であることを検証（パスプレフィックス等）
- 書き込みアトミック性
  - 一時ファイルに書いて rename で置換（同一FS上ならアトミック）。open側は部分ファイルを避けられる。
- データ整合性チェックの追加
  - open時に、各バケットのentry_countと実データ長が整合するか軽いスキャンで検証（オプション）
- 候補の返却に名前の追加（オプション）
  - name_hash一致だけでなく、別リソースの文字列照合を容易にするための補助API（このファイルには生データがないので契約設計は外部）

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト（tempdirを用いる）
  - build→open→lookupの往復
  - 衝突のあるケース（同じハッシュになるように人工データを作る）
  - 境界バケット（最終バケット、空バケット）
  - 無効なSymbolId（SymbolId::newがNoneになるようモック）
  - Windowsロック処理（cfg(windows)でraw_os_errorモック）

例: 基本往復テスト
```rust
#[test]
fn build_open_lookup_roundtrip() -> std::io::Result<()> {
    use std::path::PathBuf;
    // 仮のSymbol型を準備（本プロジェクトのSymbolに合わせる必要あり）
    let symbols = vec![
        /* ... Symbolのモック作成 ... */
    ];
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("sym.cache");

    SymbolHashCache::build_from_symbols(&path, symbols.iter())?;
    let cache = SymbolHashCache::open(&path)?;
    assert_eq!(cache.symbol_count(), symbols.len());

    // 名前で検索
    let name = symbols[0].name.clone();
    let id = cache.lookup_by_name(&name).expect("found");
    // 呼び出し側で最終照合
    assert!(symbols.iter().any(|s| s.id == id));
    Ok(())
}
```

例: バケット境界テスト
```rust
#[test]
fn bucket_boundaries_and_empty_buckets() -> std::io::Result<()> {
    // バケットを意図的に空・少数・多数に分散させるデータを用意
    // build→open→各バケットに対応する名前の検索が成功することを確認
    Ok(())
}
```

例: 不正ファイル防御のテスト（今後の修正後）
```rust
#[test]
fn open_rejects_zero_bucket_count() {
    // ヘッダのみでbucket_count=0のファイルを作ってopenがInvalidDataを返すことを確認
}
```

## Refactoring Plan & Best Practices

- 防御的open:
  - validate_bucket_count()
  - validate_offsets_monotonic_and_in_range()
  - usize_safety_check_for_offsets()
- 書き込みフローのアトミック化:
  - path.tmpへの書き込み→sync_all→rename(path.tmp, path)
- APIの拡張:
  - lookup_by_hash(name_hash)を公開し、呼び出し側が独自の文字列照合ロジックを適用できるようにする（必要なら）。ただし本チャンクには未実装。
- 定数の明確化:
  - HEADER_SIZEを構造体的に表現（Header struct）し、serialize/deserializeを集約
- ロギング基盤の利用:
  - eprintlnをtracing/logクレートに置換し、レベル管理と抑制を可能にする
- パフォーマンス微調整:
  - バケット数を2の冪で保証し、インデックス計算を & (bucket_count - 1) に変更
  - lookup_candidatesでSIMD的な先読みは過剰だが、プリフェッチ（std::arch::x86_64::_mm_prefetch）検討はunsafe域のため要注意

## Observability (Logging, Metrics, Tracing)

- ログ:
  - Windowsファイルロックリトライ時のログはeprintln→tracing::warnへ
  - build成功/失敗ログを追加（path, n, buckets）
- メトリクス:
  - ルックアップヒット率
  - 1バケット内の平均/最大エントリ数（ロードファクタ）
  - 衝突率（name_hash一致件数）
- トレーシング:
  - 長時間のbuild処理にspanを付与
  - lookupのホットパスには過度なトレースは避ける（性能重視）

## Risks & Unknowns

- memmap2::MmapのSend/Sync特性: 本チャンクには記載なし（不明）。ConcurrentSymbolCacheはRwLockで守っており、Mmapを共有参照で読むだけなら一般的には問題ないが、正確なトレイト境界は要確認。
- SymbolId::newの仕様詳細: 無効ID条件や範囲は本チャンクには現れない（不明）。
- キャッシュファイルの同時更新と参照の整合性: 本設計では明示的な再マップ機能なし。外部コンポーネントの契約（いつbuildするか、openタイミング）に依存。
- バージョニング戦略: VERSION=1固定。将来互換の運用（マイグレーション方針）は不明。
- OS依存挙動: Windowsのロックエラーコード以外のパス（ネットワークFS、特殊ファイルシステム）での挙動は未評価。

## Complexity & Performance

- Big-Oは前述の通り。追加観点:
  - メモリマップのページフォルト: 初回アクセス時に遅延が発生しうるが、アクセス局所性が高い（バケットデータは連続）ため良好な傾向。
  - キャッシュサイズ: CacheEntryは24バイトとコンパクト。バケットヘッダ4バイト＋オフセット表（8バイト×bucket_count）を含めても軽量。

## Edge Cases, Bugs, and Security

- 推奨修正一覧（重要度順）:
  1. bucket_count==0の拒否（openで検証、InvalidData返却）
  2. バケットオフセットの範囲・単調性検証（open時）
  3. Windowsロック検出をraw_os_errorで判定し、remove_fileの安全性を確保（キャッシュディレクトリ制約）
  4. 書き込みのアトミック化（rename）
  5. 32bit環境でのオフセット変換検証

## Walkthrough & Data Flow

- ルックアップの詳細（境界条件含む）:
  - 件数読み取り前に「pos + 4 > bucket_end」で早期終了（越境読取り抑止）
  - ただし bucket_end が mmap.len() より大きい場合に pos < bucket_end でも mmap[pos] が越境しパニックの可能性。オフセット検証の追加が必要。

## Testing Strategy (Unit/Integration) with Examples

- 上記の単体テストに加え、破損ファイル（ヘッダは正しいがオフセットが異常）に対するエラー応答のテストも推奨（修正後）。
- ベンチマーク（criterion）:
  - n=10^5、バケット数256、名前ルックアップで<10μsを検証（環境依存）。

## Refactoring Plan & Best Practices

- Header/Offset/Entryのシリアライザ/デシリアライザモジュール化
- SymbolHashCache::openの防御的プログラミング強化
- ConcurrentSymbolCacheへリフレッシュAPI追加（必要なら）

## Observability (Logging, Metrics, Tracing)

- メトリクス例の実装スケッチ:
```rust
// 擬似コード：lookupでメトリクス更新
fn lookup_by_name(&self, name: &str) -> Option<SymbolId> {
    let start = std::time::Instant::now();
    let res = /* ... 現行処理 ... */;
    metrics::timing!("symbol_cache.lookup_ms", start.elapsed());
    if res.is_some() { metrics::counter!("symbol_cache.hits", 1); }
    else { metrics::counter!("symbol_cache.misses", 1); }
    res
}
```

## Risks & Unknowns

- 破損ファイルの取り扱い方針（自動再生成か、失敗を上位に伝播するか）は不明。
- バケット数を動的に変更した場合のバージョン間互換性ポリシーは不明。