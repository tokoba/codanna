# memo.rs Review

## TL;DR

- 目的: **Sha256Hash** をキーにした、読み取りが安価なスレッドセーフなメモ化用インメモリマップを提供
- 主な公開API: **new**, **Default**, **insert**, **get**, **clear**
- コアロジック: **parking_lot::RwLock + HashMap + Arc** による多読少書向けの並行アクセス最適化
- 注意点: ドキュメンテーションが実装と不一致（値型に Clone 境界は不要、Arc を利用）
- 並行性: 読み取りはロックを共有できるため高スループット、書き込みは全体ロック
- リスク: エビクションなしのためメモリ増加、V の Send/Sync 境界未提示（設計意図の明確化推奨）
- セキュリティ: インジェクション等は該当なし、ログ・秘密情報取り扱いなし

## Overview & Purpose

このファイルは、**Sha256Hash** をキーとし、値を **Arc<V>** として保持するスレッドセーフなメモ化マップを提供します。内部実装には **parking_lot::RwLock** と **HashMap** を用い、読み取りの多いアクセスパターンに適した設計です。get() は **Arc** をクローンして返すため、値型 **V** 自体を Clone にする必要なく、読み取りが安価で内的可変性を晒しません。

用途は「計算済み結果のキャッシュ」「同一ハッシュの解決結果の共有」など、解決処理の再利用・重複作業抑制です。

このチャンクに他モジュールの目的やプロジェクト全体での役割に関する記述は「不明」です。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | ResolutionMemo<V> | pub | Sha256Hash→Arc<V> のマップを RwLock で保護し、メモ化のCRUDを提供 | Low |
| Method | new | pub | 内部マップの初期化 | Low |
| Trait Impl | Default for ResolutionMemo<V> | pub | new() の委譲 | Low |
| Method | insert | pub | キーに対応する値を挿入/更新 | Low |
| Method | get | pub | キーに対応する値（Arcクローン）を取得 | Low |
| Method | clear | pub | 全エントリ削除 | Low |

### Dependencies & Interactions

- 内部依存
  - ResolutionMemo<V>::new → RwLock<HashMap<…>> の初期化
  - ResolutionMemo<V>::insert → self.inner.write() で書き込みロック取得 → map.insert(key, Arc::new(value))
  - ResolutionMemo<V>::get → self.inner.read() で読み取りロック取得 → map.get(key).cloned()
  - ResolutionMemo<V>::clear → self.inner.write() で書き込みロック取得 → map.clear()
- 外部依存（このチャンクに現れるもの）
  | クレート/モジュール | 用途 | 備考 |
  |--------------------|------|------|
  | parking_lot::RwLock | 読み取り/書き込みロック | パニック時のポイズニングなし。高速・公平性設定なし。 |
  | std::collections::HashMap | キー→値の格納 | 平均 O(1) の検索/挿入 |
  | std::sync::Arc | 値共有のための参照カウント | V は Clone不要。Arc クローンは O(1) |
  | super::Sha256Hash | キー型 | 実体は「不明」 |
- 被依存推定（このモジュールを利用する可能性のある箇所）
  - ハッシュベースのリゾルバ/キャッシュ層
  - 計算結果のメモ化を必要とする高負荷処理
  - 並行アクセス下での結果共有（値が不変で共有可能な場合）

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| new | fn new() -> Self | 空のメモを作成 | O(1) | O(1) |
| Default | impl Default for ResolutionMemo<V> | new() へ委譲 | O(1) | O(1) |
| insert | fn insert(&self, key: Sha256Hash, value: V) | 値の挿入/上書き | 平均O(1) | O(1) 追加（Arc割当） |
| get | fn get(&self, key: &Sha256Hash) -> Option<Arc<V>> | 値の取得（Arcクローン） | 平均O(1) | O(1) |
| clear | fn clear(&self) | 全エントリ削除 | O(n) | O(1)（解放） |

詳細:

1) new
- 目的と責務: 空の RwLock(HashMap) を初期化する。
- アルゴリズム:
  1. HashMap::new() を生成
  2. RwLock に包む
  3. ResolutionMemo を返す
- 引数:
  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | なし | - | - | - |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | ResolutionMemo<V> | 空のメモ構造体 |
- 使用例:
  ```rust
  let memo: ResolutionMemo<String> = ResolutionMemo::new();
  ```
- エッジケース:
  - 特になし（初期化は定数時間）

2) Default
- 目的と責務: new() のショートハンド。
- アルゴリズム: Self::new() を呼ぶ。
- 引数/戻り値/使用例:
  ```rust
  let memo: ResolutionMemo<String> = Default::default();
  ```
- エッジケース: 特になし。

3) insert
- 目的と責務: キーに対する値を挿入、既存なら上書き。
- アルゴリズム:
  1. writeロック取得
  2. value を Arc::new(value) に包む
  3. map.insert(key, Arc<V>) を呼ぶ
- 引数:
  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | key | Sha256Hash | はい | キー |
  | value | V | はい | 値（Arcに包まれる） |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | () | 返り値なし。上書き可能。 |
- 使用例:
  ```rust
  let memo = ResolutionMemo::new();
  let key: Sha256Hash = /* 生成方法はこのチャンクでは不明 */;
  memo.insert(key, "computed".to_string());
  ```
- エッジケース:
  - 同一キー再挿入で上書きされる
  - 大きな値でも Arc により複製は安価（ただし割当は発生）

4) get
- 目的と責務: キーに対応する値を取得（Arcをクローンして返す）。
- アルゴリズム:
  1. readロック取得
  2. map.get(key) を呼ぶ
  3. Option<&Arc<V>> を cloned() で Option<Arc<V>> に変換
- 引数:
  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | key | &Sha256Hash | はい | キーの参照 |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | Option<Arc<V>> | 見つかった場合 Arc<V>、なければ None |
- 使用例:
  ```rust
  let memo = ResolutionMemo::new();
  let key: Sha256Hash = /* 不明 */;
  memo.insert(key, 42_u32);
  if let Some(val_arc) = memo.get(&key) {
      // Arc のクローンが返るため、クローンコストは O(1)
      assert_eq!(*val_arc, 42);
  }
  ```
- エッジケース:
  - 未登録キーは None
  - 返されるのは Arc のクローンであり、所有権は共有される

5) clear
- 目的と責務: 全エントリの削除。
- アルゴリズム:
  1. writeロック取得
  2. map.clear() を呼ぶ
- 引数:
  | 名前 | 型 | 必須 | 説明 |
  |------|----|------|------|
  | なし | - | - | - |
- 戻り値:
  | 型 | 説明 |
  |----|------|
  | () | 返り値なし |
- 使用例:
  ```rust
  let memo = ResolutionMemo::new();
  let key: Sha256Hash = /* 不明 */;
  memo.insert(key, vec![1,2,3]);
  memo.clear();
  assert!(memo.get(&key).is_none());
  ```
- エッジケース:
  - 保持している Arc の外部クローンはそのまま有効（メモからは消える）

## Walkthrough & Data Flow

- insert:
  - 書き込みロックで HashMap にアクセス
  - 値 V を Arc<V> に包むことで、以後の取得は Arc クローンのみで済む
  - 同一キーが存在する場合は古い Arc を破棄（外部保持の Arc があれば、その参照が 0 になるまで生存）

- get:
  - 読み取りロックで HashMap にアクセス
  - &Arc<V> を cloned() して Arc<V> を返却
  - 読み取りロックは複数スレッドで共有可能

- clear:
  - 書き込みロックで HashMap をクリア
  - マップ内の Arc 参照が削除されるが、外部にクローンがあれば生存継続

データ契約の観点:
- キー: Sha256Hash（詳細は不明）
- 値: Arc<V> として保持（V は Clone 不要）
- get は Option<Arc<V>> により存在有無を明確化

## Complexity & Performance

- 時間計算量
  - insert/get: 平均 O(1)（HashMap の前提）
  - clear: O(n)
- 空間計算量
  - 全体 O(n)（登録件数に比例）
  - 各挿入時に Arc の割当（小さい定数オーバーヘッド）
- ボトルネック
  - 書き込み集中時に RwLock の競合が発生
  - HashMap のリサイズ時に一時的に挿入コスト増
- スケール限界
  - 非分散の単一プロセス内メモ。大規模データではメモリが増大し、OOM のリスク
  - 書き込みが多いワークロードだと RwLock でスループットが低下
- 実運用負荷要因
  - 読み取り中心であれば高スループット
  - エビクションや容量制限がなく、長時間運用でメモリ増加

## Edge Cases, Bugs, and Security

- エッジケース詳細

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 未登録キー取得 | get(&unknown) | None を返す | map.get().cloned() | OK |
| 同一キー上書き | insert(k, v1) → insert(k, v2) | 最新が取得可能。古い Arc は外部参照がなければ解放 | map.insert() | OK |
| 大きな値 | insert(k, large_v) | Arc により get の複製は安価。ただし割当は発生 | Arc::new(value) | OK |
| クリア後の取得 | clear() → get(&k) | None を返す | map.clear() | OK |
| 多数並行読み取り | N スレッドで get | 共有 read ロックでスケール | RwLock::read() | OK |
| 多数並行書き込み | N スレッドで insert/clear | 書き込みは逐次化。スループット低下 | RwLock::write() | OK |
| Sha256Hash の衝突 | ハッシュ衝突 | HashMap はキーの等価判定で保護。衝突はバケット共有 | HashMap 実装依存 | OK |

- 既知/潜在バグ
  - ドキュメントの不一致: コメント「Value type must be Clone」は誤り。Arc により V: Clone は不要。（行: ファイル先頭コメント）
  - API上の境界未明示: 目的が「スレッドセーフ共有」であるなら、型パラメータ V について Send + Sync の境界を型に明示することを検討（現状でもコンパイラが不安全な送受信を禁止するが、意図の明確化に有用）

- セキュリティチェックリスト
  - メモリ安全性: Rust の所有権/借用と Arc/RwLock により安全。Buffer overflow / Use-after-free / Integer overflow の懸念なし。
  - インジェクション: SQL/Command/Path いずれも該当なし。
  - 認証・認可: 該当なし。
  - 秘密情報: ハードコードされた秘密なし。ログ漏えいもなし。
  - 並行性: Race condition は RwLock により防止。Deadlock は1ロックのみで再入なしのため可能性低。駆動公平性は parking_lot の実装（非ポイズニング）。書き込みスタベーションはワークロード次第で起こり得る。

- Rust特有の詳細チェックリスト
  - 所有権: insert(key, value) で key と value はマップに移動（ResolutionMemo::insert: 該当行は関数内。行番号情報はこのチャンクには現れない）。get は &Sha256Hash の参照を借用し、Arc をクローンして返すため所有権の移動なし。
  - 借用: get は read ロック期間中に &Arc<V> を取得し、即座に cloned() で複製後、ロックを解放。可変借用は insert/clear の write ロック中のみ。
  - ライフタイム: 明示的ライフタイム不要。Arc が所有権を共有し、返却後も値は存続。
  - unsafe 境界: unsafe ブロックは「該当なし」。
  - Send/Sync:
    - 実際の使用で ResolutionMemo<V> をスレッド間共有するには V が Send（少なくとも）であることが望ましい。Arc<V> を他スレッドで使用するには V: Send + Sync が必要。型制約を API で明示することを推奨。
  - データ競合: RwLock により排他/共有制御。内部共有状態はロック保護。
  - await 境界/非同期: 非同期は「該当なし」。
  - キャンセル: 「該当なし」。
  - エラー設計: get は Option を返し妥当。panic を誘発する unwrap/expect は「該当なし」。From/Into によるエラー変換は「該当なし」。

## Design & Architecture Suggestions

- ドキュメント更新: 冒頭コメントの「Value type must be Clone」は誤記。**Arc を使っているため V に Clone は不要**。コメント修正推奨。
- 型境界の明示: 並行共有を意図するなら `pub struct ResolutionMemo<V: Send + Sync>` やメソッド境界での明示を検討。意図の伝達と誤用防止に有効。
- API拡張:
  - `get_or_insert_with(&self, key, f: impl FnOnce() -> V) -> Arc<V>`
  - `remove(&self, key: &Sha256Hash) -> Option<Arc<V>>`
  - `len(&self) -> usize`, `is_empty(&self) -> bool`
- 容量制御/エビクション: メモ化用途では LRU/TTL の導入や最大容量設定を検討（ライブラリ: `lru` crate など）。現状は無制限でメモリ増大リスク。
- 競合戦略: 読み取りが極端に多い場合は `DashMap` の採用やシャーディングで書き込みボトルネック緩和を検討。
- 初期容量: 頻繁なリサイズを避けるため `with_capacity` 相当の初期化 API を追加するとよい。

## Testing Strategy (Unit/Integration) with Examples

- 単体テスト
  - 基本CRUD
    ```rust
    #[test]
    fn basic_crud() {
        let memo = ResolutionMemo::new();
        let k: Sha256Hash = /* このチャンクでは生成方法不明 */;
        memo.insert(k, "x".to_string());
        assert_eq!(&*memo.get(&k).unwrap(), "x");
        memo.clear();
        assert!(memo.get(&k).is_none());
    }
    ```
  - 上書き挙動
    ```rust
    #[test]
    fn overwrite() {
        let memo = ResolutionMemo::new();
        let k: Sha256Hash = /* 不明 */;
        memo.insert(k, 1u32);
        memo.insert(k, 2u32);
        assert_eq!(*memo.get(&k).unwrap(), 2u32);
    }
    ```
  - Arc クローンの独立性（メモから削除後も外部クローンが生存）
    ```rust
    #[test]
    fn arc_survives_clear() {
        let memo = ResolutionMemo::new();
        let k: Sha256Hash = /* 不明 */;
        memo.insert(k, vec![1,2,3]);
        let v = memo.get(&k).unwrap();
        memo.clear();
        assert_eq!(&*v, &vec![1,2,3]); // 外部クローンは生存
    }
    ```
- 並行テスト
  - 多読少書シナリオ
    ```rust
    use std::sync::Arc as StdArc;
    use std::thread;

    #[test]
    fn concurrent_reads_and_writes() {
        let memo = StdArc::new(ResolutionMemo::new());
        let k: Sha256Hash = /* 不明 */;

        // 初期挿入
        memo.insert(k, 0usize);

        let readers: Vec<_> = (0..8).map(|_| {
            let memo = StdArc::clone(&memo);
            thread::spawn(move || {
                for _ in 0..1000 {
                    let _ = memo.get(&k);
                }
            })
        }).collect();

        let writer = {
            let memo = StdArc::clone(&memo);
            thread::spawn(move || {
                for i in 1..100 {
                    memo.insert(k, i);
                }
            })
        };

        for r in readers { r.join().unwrap(); }
        writer.join().unwrap();

        assert!(memo.get(&k).is_some());
    }
    ```
- ベンチマーク（例）
  - `criterion` を用いて read-heavy / write-heavy パターンの計測を推奨（このチャンクには具体例は「不明」）。

## Refactoring Plan & Best Practices

- ステップ1: コメント修正（Clone 必須の記述を Arc 前提に更新）
- ステップ2: API 境界の明示（V: Send + Sync を付与するか、ドキュメントで要件を明記）
- ステップ3: ヘルパー追加（`get_or_insert_with`, `remove`, `len`）
- ステップ4: 初期容量設定 API（大規模用途でのリサイズ抑制）
- ステップ5: 必要に応じて LRU/TTL 導入（メモリ制御）
- ベストプラクティス:
  - 返却は Arc のみ（内部可変性を外に出さない）
  - ロック期間は最小化（現在も短いが、複雑化時も意識）
  - ドキュメントと実装の整合性を保つ

## Observability (Logging, Metrics, Tracing)

- 現状: ログ/メトリクス/トレースは「該当なし」
- 提案:
  - メトリクス: hit/miss、サイズ、クリア回数、挿入回数
  - ログ: 大規模クリアや異常な増加を INFO/DEBUG で記録
  - トレース: 高コスト計算のメモ化効果を可視化（外部トレーサ連携）

## Risks & Unknowns

- 不明点:
  - Sha256Hash の具体型・Eq/Hash 実装の詳細（このチャンクには現れない）
  - このメモのライフサイクル/ガベージポリシー（エビクション戦略は「不明」）
  - 想定ワークロード（読取/書込比率、データサイズ）
- リスク:
  - 無制限成長によるメモリ圧迫
  - 書き込み集中下でのスループット低下
  - 型境界未明示による API 誤用（並行安全性の意図が伝わらない可能性）
  - parking_lot の RwLock 仕様による書き込みスタベーション（ワークロード依存）

以上の通り、本ファイルはシンプルで堅牢なメモ化マップを提供しています。主な改善点はドキュメント整合性の修正、型境界の明示、容量管理の追加、および観測可能性の向上です。