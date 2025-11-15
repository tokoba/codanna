# file_info.rs Review

## TL;DR

- 目的: **ファイル内容のSHA-256ハッシュ**と**UTC秒タイムスタンプ**を保持し、インクリメンタルインデックス更新の差分検出を効率化
- 公開API: **FileInfo::new**, **FileInfo::has_changed**, **calculate_hash**, **get_utc_timestamp**
- 複雑/注意点: **UTCタイムスタンプの型変換(i64→u64)**による負値キャスト問題、**UTF-8前提のハッシュ計算**でバイナリ非対応
- 重大リスク: システム時計が1970年以前の場合に**u64へのキャストが巨大値**になり不正（単調性や範囲保証が崩れる）
- パフォーマンス: ハッシュ計算は**O(n)**（入力サイズに比例）、出力は常に64桁の16進文字列で**O(1)メモリ**
- 安全性: **unsafeなし**、所有権・借用は明確、**並行性なし**（テストでthread::sleepを使用するのみ）
- テスト: ハッシュ一貫性・タイムスタンプ妥当性・変更検出をカバー。ただし**マイナス時刻**や**バイナリ入力**は未検証

## Overview & Purpose

このモジュールは、インデックス済みファイルのメタ情報（ID、パス、内容ハッシュ、最終インデックス時刻）を保持し、内容の変化を**ハッシュ比較**で検出する仕組みを提供します。インクリメンタルインデックスのコンテキストで、再処理が必要なファイルを効率的に判定するための**軽量な状態管理**を目的としています。

- ハッシュ形式は**SHA-256（小文字16進、64文字）**
- 時刻は**UTCのUNIXエポック秒**（i64→u64キャスト）
- ファイル内容は**&str**で受け取り、**UTF-8前提**（バイナリ非対応）

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | FileInfo | pub | ファイルID・パス・内容ハッシュ・最終更新時刻の保持 | Low |
| Impl Method | FileInfo::new | pub | 初期化（ハッシュ計算・UTC秒取得） | Low |
| Impl Method | FileInfo::has_changed | pub | 与えられた内容のハッシュと既存値を比較 | Low |
| Function | calculate_hash | pub | &strのSHA-256ハッシュ計算（16進文字列） | Low |
| Function | get_utc_timestamp | pub | 現在UTC時刻のUNIX秒（u64）取得 | Low |

### Dependencies & Interactions

- 内部依存
  - FileInfo::new → calculate_hash, get_utc_timestamp
  - FileInfo::has_changed → calculate_hash
- 外部依存（このチャンク内で使用）
  - chrono::Utc（現在時刻の取得）
  - sha2::{Digest, Sha256}（ハッシュ計算）
  - std::path::PathBuf（パス保持）
  - crate::FileId（ID型。詳細はこのチャンクには現れない）
- 被依存推定
  - インデクサ/キャッシュ管理モジュールがFileInfoを参照して**差分判定**や**再インデックス判定**に利用
  - 具体的な呼び出し元は不明（このチャンクには現れない）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| FileInfo::new | fn new(id: FileId, path: PathBuf, content: &str) -> Self | 初期化（ハッシュとUTC秒を設定） | O(n) | O(1) |
| FileInfo::has_changed | fn has_changed(&self, content: &str) -> bool | 内容変更検出（ハッシュ比較） | O(n) | O(1) |
| calculate_hash | fn calculate_hash(content: &str) -> String | SHA-256を16進文字列で返す | O(n) | O(1) |
| get_utc_timestamp | fn get_utc_timestamp() -> u64 | 現在UTCのUNIXエポック秒（u64） | O(1) | O(1) |

nはcontentのバイト長。

### FileInfo::new

1. 目的と責務
   - FileInfoの初期化。内容から**SHA-256ハッシュ**を算出し、**現在UTC秒**を設定。

2. アルゴリズム（ステップ）
   - calculate_hash(content)でハッシュ計算
   - get_utc_timestamp()でUTC秒取得
   - フィールドに格納してSelfを返す

3. 引数

| 名称 | 型 | 必須 | 説明 |
|-----|----|------|------|
| id | FileId | 必須 | 一意なファイル識別子 |
| path | PathBuf | 必須 | ファイルパス（相対/絶対いずれも可） |
| content | &str | 必須 | ファイル内容（UTF-8前提） |

4. 戻り値

| 型 | 説明 |
|----|------|
| FileInfo | 初期化済み構造体 |

5. 使用例
```rust
use std::path::PathBuf;
// FileIdの詳細はこのチャンクには現れないが、テストではFileId::new(…)が使われている
let file_id = FileId::new(123).unwrap();
let path = PathBuf::from("src/lib.rs");
let content = "pub fn greet() {}";
let info = FileInfo::new(file_id, path, content);
// info.hashは64文字の小文字16進、info.last_indexed_utcはUTC秒
```

6. エッジケース
- contentが空文字
- pathが相対/絶対混在
- システム時計が1970年より前（負のtimestamp）

### FileInfo::has_changed

1. 目的と責務
   - 現在のFileInfo.hashと引数contentのハッシュを比較して変更有無を返す。

2. アルゴリズム
   - calculate_hash(content)を計算
   - self.hashと不一致ならtrue、一致ならfalse

3. 引数

| 名称 | 型 | 必須 | 説明 |
|-----|----|------|------|
| content | &str | 必須 | 比較対象の内容 |

4. 戻り値

| 型 | 説明 |
|----|------|
| bool | 変更ありならtrue、なければfalse |

5. 使用例
```rust
let changed = info.has_changed("pub fn greet() { println!(\"hi\"); }");
if changed {
    // 再インデックス処理へ
}
```

6. エッジケース
- 大きな内容（ハッシュ計算コスト増）
- 空文字比較
- 同一内容の場合のfalse確認

### calculate_hash

1. 目的と責務
   - 与えられた文字列のSHA-256ハッシュを計算し、**小文字16進（64文字）**で返す。

2. アルゴリズム
   - Sha256::new()で初期化
   - content.as_bytes()をupdate
   - finalize()し、format!("{:x}", …)で16進化

3. 引数

| 名称 | 型 | 必須 | 説明 |
|-----|----|------|------|
| content | &str | 必須 | 入力テキスト（UTF-8） |

4. 戻り値

| 型 | 説明 |
|----|------|
| String | 64文字の16進ハッシュ |

5. 使用例
```rust
let h = calculate_hash("Hello, World!");
assert_eq!(h.len(), 64);
```

6. エッジケース
- 空文字 → 既知のSHA-256空ハッシュ
- 非ASCII文字 → UTF-8としてそのバイト列をハッシュ
- バイナリデータ → 非対応（&str前提）

### get_utc_timestamp

1. 目的と責務
   - 現在UTC時刻のUNIXエポック秒を**u64**として返す。

2. アルゴリズム
   - chrono::Utc::now().timestamp()（i64秒）を取得
   - as u64でキャスト（負値の場合の挙動に注意）

3. 引数
- なし

4. 戻り値

| 型 | 説明 |
|----|------|
| u64 | 現在UTCのUNIX秒 |

5. 使用例
```rust
let ts = get_utc_timestamp();
// 単調増加を期待するが、秒分解能で同値になる場合あり
```

6. エッジケース
- システム時計が過去（1970年未満）→ 負値キャストで巨大値
- NTP調整や時計逆行 → 単調性が崩れる可能性

## Walkthrough & Data Flow

一般的な処理フロー:
1. ファイル読み取り（このモジュール外）
2. FileInfo::new(id, path, content)で初期情報生成
   - calculate_hash(content)により**hash**を生成
   - get_utc_timestamp()により**last_indexed_utc**を設定
3. 後続で内容が更新された場合、FileInfo::has_changed(new_content)で変更検出
   - 変更ありなら再インデックス＋last_indexed_utcを更新（更新処理はこのチャンクには現れない）

このチャンクでは分岐は少なく、Mermaid図は基準により省略（条件分岐が4つ以上ではない）。

## Complexity & Performance

- calculate_hash: 時間O(n)、空間O(1)（出力は常に64文字）
- FileInfo::new: 時間O(n)、空間O(1)
- FileInfo::has_changed: 時間O(n)、空間O(1)
- get_utc_timestamp: 時間O(1)、空間O(1)

ボトルネック:
- nが大きい（巨大ファイル内容）場合のハッシュ計算時間
- 秒分解能のタイムスタンプは**高頻度更新**の識別に不向き（同秒に複数イベント）

スケール限界:
- 1ファイルあたりの処理は軽量だが、**大量のファイルで連続ハッシュ**計算するとCPU負荷増。I/O/ネットワーク/DBはこのチャンクには現れない。

## Edge Cases, Bugs, and Security

セキュリティチェックリスト評価:
- メモリ安全性
  - Buffer overflow: なし（safe Rust、境界チェック済みAPIのみ使用）
  - Use-after-free: なし（所有権/借用はRustが保証）
  - Integer overflow: 潜在あり。i64→u64キャストで負値時に**ラップアラウンド**（get_utc_timestamp）
- インジェクション
  - SQL/Command/Path traversal: 該当なし（I/Oや外部コマンド未使用）
- 認証・認可
  - 該当なし
- 秘密情報
  - Hard-coded secrets: なし
  - Log leakage: 本番コードではログなし（テスト出力は存在）
- 並行性
  - Race condition/Deadlock: 該当なし（共有可変状態なし、同期なし）
  - 単調時刻: Utc::now()は**モノトニックではない**ためNTP調整で逆行しうる

Rust特有の観点:
- 所有権
  - FileInfo::new(id: FileId, path: PathBuf, content: &str)でidとpathは**ムーブ**され、構造体に所有される。contentは借用。
- 借用
  - FileInfo::has_changed(&self, content: &str)は不変借用のみで安全。
- ライフタイム
  - 明示的ライフタイムは不要（&strの短期借用のみ）
- unsafe境界
  - unsafe未使用
- 並行性/非同期
  - Send/Sync: FileInfoは標準型の集まりで、一般にSend/Syncだが、crate::FileIdの実装次第（このチャンクには現れない）
  - await境界/キャンセル: 該当なし
- エラー設計
  - Result/Option未使用。get_utc_timestampの**負値**などをエラーとして返さないため、堅牢性は限定的
- panic箇所
  - なし（unwrapはテスト内のFileId::newでのみ使用）

エッジケース詳細:

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 空文字列 | "" | ハッシュは空文字のSHA-256（64桁）/ has_changedは適切に比較 | calculate_hash/has_changed | OK |
| 非ASCII文字 | "こんにちは" | UTF-8バイトでハッシュ/比較 | calculate_hash | OK |
| バイナリ内容 | &[0, 159, 146, 150] | バイナリを扱えること | &str前提で非対応 | 要改善 |
| 巨大ファイル | "x"×10MB | 時間は増加、メモリは一定（出力64桁） | Sha256で線形時間 | OK（性能注意） |
| 相対パス/重複 | "src/lib.rs" | 同一ファイルの正規化が行われること | 正規化なし | リスク（重複検出漏れ） |
| 負のUNIX秒 | システム時計<1970 | 0やErrなどで保護 | i64→u64キャスト | バグ潜在 |
| 単調時刻期待 | NTP調整あり | 時刻逆行時にも整合性維持 | Utc::now()依存 | リスク |

根拠（関数名:行番号）：行番号はこのチャンクには現れないため、関数名のみで言及。

## Design & Architecture Suggestions

- get_utc_timestampの堅牢化
  - chrono依存維持でも、負値対策として**clamp**や**Result<u64, Error>**返却に変更
  - もしくは標準のSystemTimeを使用し、`SystemTime::now().duration_since(UNIX_EPOCH)`で**Result**にする
- バイナリ対応
  - calculate_hashを`&[u8]`に変更し、`&str`オーバーロードを用意（`as_bytes()`で委譲）
- パス正規化
  - `std::fs::canonicalize`やケース感度、シンボリックリンク考慮で**一意性向上**
- タイムスタンプの分解能改善
  - 秒ではなく**ミリ秒/ナノ秒**を用いる（必要性とストレージフォーマット要件次第）
- APIの拡張
  - FileInfoに`update(content: &[u8])`などを追加して**ハッシュと時刻の更新**を一括実行
  - ハッシュ関数を差し替え可能にする（traitで抽象化）

## Testing Strategy (Unit/Integration) with Examples

既存テストは以下をカバー:
- ハッシュ一貫性・長さ（64桁）
- UTC時刻が0より大きい、単調非減少
- 変更検出（has_changed）

追加推奨テスト:
- 空文字・非ASCII
```rust
#[test]
fn test_empty_and_non_ascii() {
    assert_eq!(calculate_hash(""), calculate_hash(""));
    assert!(!FileInfo::new(FileId::new(1).unwrap(), PathBuf::from("a"), "")
        .has_changed(""));
    let h = calculate_hash("こんにちは");
    assert_eq!(h.len(), 64);
}
```
- バイナリ対応（設計変更後）
```rust
#[test]
fn test_binary_hash() {
    let data: &[u8] = &[0, 159, 146, 150];
    // 変更後: calculate_hash_bytes(&[u8]) を導入
    let h = calculate_hash_bytes(data);
    assert_eq!(h.len(), 64);
}
```
- 負の時刻の防御（SystemTime使用時）
```rust
#[test]
fn test_timestamp_non_negative() {
    let ts = get_utc_timestamp();
    assert!(ts >= 0);
}
```
- パス正規化（導入後）
```rust
#[test]
fn test_path_canonicalization() {
    // update: FileInfo::newが正規化する前提のテスト
}
```

## Refactoring Plan & Best Practices

- get_utc_timestampを安全化
  - SystemTimeベースへ移行し、負時刻を**Err**で返すか、0にフォールバック
```rust
pub fn get_utc_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(d) => d.as_secs(),
        Err(_) => 0, // あるいはResult<u64, SystemTimeError>で返す
    }
}
```
- バイナリ入力対応
```rust
pub fn calculate_hash_bytes(content: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content);
    format!("{:x}", hasher.finalize())
}
// 既存関数はこれへ委譲
pub fn calculate_hash(content: &str) -> String {
    calculate_hash_bytes(content.as_bytes())
}
```
- API明確化
  - FileInfoに`recompute(&mut self, new_content: &[u8])`を追加して、ハッシュ再計算と時刻更新を一体化
- ドキュメント強化
  - ハッシュ形式（小文字16進、64文字）、時刻仕様（UTC秒）を**Data Contract**として明記

## Observability (Logging, Metrics, Tracing)

- ログ
  - 変更検出時に**ファイルID/パス**とハッシュの先頭数文字をINFOで出力（漏洩防止のため全ハッシュは避ける）
- メトリクス
  - ハッシュ計算時間（ヒストグラム）
  - 変更検出率、再インデックス件数
- トレーシング
  - インデックス処理のスパンにFileInfo再計算を**子スパン**として関連付け
- 注意点
  - ハッシュ値は機密ではないが、過剰な出力を避ける（*log leakage*防止）

## Risks & Unknowns

- FileIdの仕様が不明
  - 一意性の保証やスレッドセーフ性、シリアライズ要件がこのチャンクには現れない
- パスの正規化方針が不明
  - OS差異（Windowsの大文字小文字、UNC、シンボリックリンク）への対応はこのチャンクには現れない
- タイムスタンプの利用方法が不明
  - 単調性の要求レベル（NTP揺らぎ許容か）がこのチャンクには現れない
- ハッシュ関数の固定化
  - SHA-256で十分か、将来的なアルゴリズム変更の可能性は不明

以上により、このモジュールはインクリメンタルインデックスの基盤としてシンプルかつ有用ですが、時刻取得の安全性と入力形式（バイナリ対応）、パス正規化の設計を補強することで、より堅牢な運用が可能になります。