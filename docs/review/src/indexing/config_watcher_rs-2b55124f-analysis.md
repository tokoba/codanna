# indexing\config_watcher.rs Review

## TL;DR

- 目的: settings.toml の indexed_paths 変更を監視し、追加パスの自動インデクシングと通知配信を行う。
- 公開API: ConfigFileWatcher::new, with_broadcaster, watch（非同期ループ）。struct 自体はpubだがフィールドは非公開。
- コアロジック: notify::RecommendedWatcher → tokio::mpsc 経由でイベントを受け、対象ファイルのみ判定、差分（added/removed）抽出、追加分を SimpleIndexer で同期的に再インデックス、通知送信。
- 複雑箇所: 非同期（tokio）×同期I/O（index_directory）混在、blocking_sendと小容量チャンネル(10)によるバックプレッシャ、パス比較の厳密性（正規化なし）。
- 重大リスク:
  - 長時間の同期インデクシングがasyncランタイムスレッドをブロック。
  - notifyコールバックでのblocking_sendがチャンネル満杯時に停止しイベント欠落や遅延の温床。
  - パスの完全一致比較により、rename/atomic-writeや大文字小文字差・シンボリックリンクで見落としの可能性。
  - ログがeprintln中心で観測性が限定的。デバウンスは100ms固定でイベントストーム時に過剰再処理の可能性。

## Overview & Purpose

本ファイルは、設定ファイル settings.toml の変更（特に indexing.indexed_paths の追加・削除）を監視し、変化があれば以下を実施する監視コンポーネントを提供する。

- 追加パスのインデクシング（SimpleIndexer.index_directory）
- インデックス変更の通知（NotificationBroadcaster.send(FileChangeEvent::IndexReloaded)）
- 起動時に設定と実インデックスの差分を確認し、未インデックスのパスを補完（check_initial_sync）

削除パスの実体クリーニングはこのモジュールでは行わず、ユーザに codanna clean / codanna index 実行を促すに留める。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Struct | ConfigFileWatcher | pub（フィールドはprivate） | 設定ファイル監視、差分検出、インデクシングトリガ、通知送出 | Med |
| Method | new | pub | ウォッチャ初期化（チャンネル、notifyウォッチャ、初期設定ロード） | Med |
| Method | with_broadcaster | pub | 通知ブロードキャスタの注入 | Low |
| Method | watch | pub async | 設定ディレクトリ監視開始、初期同期、イベントループ、差分処理 | High |
| Method | check_initial_sync | private async | 起動時に設定と現インデックス差分を埋める | Med |
| Method | handle_config_change | private async | 設定変更イベント後の差分解析とインデクシング、通知 | High |

### Dependencies & Interactions

- 内部依存
  - watch → check_initial_sync, handle_config_change を呼ぶ。
  - handle_config_change → Settings::load_from, SimpleIndexer::{get_indexed_paths, index_directory}, NotificationBroadcaster::send
  - check_initial_sync → SimpleIndexer::{get_indexed_paths, index_directory}, NotificationBroadcaster::send
- 外部依存

| クレート/モジュール | 用途 | 重要点 |
|---------------------|------|--------|
| notify::{RecommendedWatcher, Event, EventKind, RecursiveMode, Watcher} | ファイルシステム変更監視 | recommended_watcherのコールバックでtokio mpscへblocking_send |
| tokio::sync::{mpsc, RwLock} | 非同期チャンネル、インデクサ共有ロック | mpsc容量10、RwLock越しに同期処理実行 |
| std::{collections::HashSet, path::PathBuf, sync::Arc} | 基本構造 | PathBufで比較、正規化なし |
| crate::config::Settings | 設定読み込み | indexing.indexed_paths を利用 |
| crate::mcp::notifications::{FileChangeEvent, NotificationBroadcaster} | 通知送出 | IndexReloaded 通知 |
| crate::{IndexError, IndexResult, SimpleIndexer} | エラー型、結果型、インデクサ | index_directoryが同期処理の前提 |

- 被依存推定
  - MCPサーバの起動コード（設定監視担当）
  - CLI/デーモンの初期化箇所でインスタンス化して tokio::spawn で常駐運転
  - 実際のインデクサを共有し、別のAPI（手動再インデックス等）と共存

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|-----------|------|------|-------|
| ConfigFileWatcher::new | pub fn new(settings_path: PathBuf, indexer: Arc<RwLock<SimpleIndexer>>, mcp_debug: bool) -> IndexResult<Self> | ウォッチャの構築、初期状態の読み込み | O(P) 読み込み・パース | O(K) indexed_paths数 |
| ConfigFileWatcher::with_broadcaster | pub fn with_broadcaster(self, broadcaster: Arc<NotificationBroadcaster>) -> Self | 通知ブロードキャスタの注入（ビルダー） | O(1) | O(1) |
| ConfigFileWatcher::watch | pub async fn watch(self) -> IndexResult<()> | 監視開始、初期同期、イベント処理ループ | 各イベントあたり O(1)+インデクスコスト | O(K) |

K=tracked paths数, P=設定ファイルサイズ。インデックスコストは SimpleIndexer に依存（不明）。

以下、各APIの詳細。

### ConfigFileWatcher::new

1) 目的と責務
- mpscチャンネルを生成（容量10）
- notify::recommended_watcher でFSイベントを受け取り tx.blocking_send でmpscへ橋渡し
- Settings::load_from で初期 indexed_paths をHashSetに取り込む

2) アルゴリズム
- mpsc::channel(10) 生成
- recommended_watcher(move |res| { let _=tx.blocking_send(res); })
- Settings::load_from(&settings_path) のエラーを IndexError::ConfigError に変換
- 構造体フィールド初期化（_watcherを保持してライフタイム維持）

3) 引数

| 名称 | 型 | 説明 |
|-----|----|------|
| settings_path | PathBuf | 監視対象設定ファイルへのパス |
| indexer | Arc<RwLock<SimpleIndexer>> | 共有インデクサ |
| mcp_debug | bool | デバッグ出力を有効化 |

4) 戻り値

| 型 | 説明 |
|----|------|
| IndexResult<Self> | 成功時に構築済みウォッチャ、失敗時にIndexError |

5) 使用例

```rust
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

async fn start_watcher(indexer: Arc<RwLock<SimpleIndexer>>, broadcaster: Arc<NotificationBroadcaster>) -> IndexResult<()> {
    let watcher = ConfigFileWatcher::new(PathBuf::from("settings.toml"), indexer, true)?
        .with_broadcaster(broadcaster);

    tokio::spawn(async move {
        if let Err(e) = watcher.watch().await {
            eprintln!("watcher exited with error: {e}");
        }
    });

    Ok(())
}
```

6) エッジケース
- settings_pathの親ディレクトリが存在しない場合、watchでNotFoundエラー
- 設定ファイルの読み込み失敗時は IndexError::ConfigError
- notify初期化失敗時は IndexError::FileRead（sourceに変換）

根拠: new（行番号: 不明）。Settings::load_from, recommended_watcher の使用は該当コードに明示。

### ConfigFileWatcher::with_broadcaster

1) 目的と責務
- 通知用 NotificationBroadcaster を注入するビルダー関数

2) アルゴリズム
- self.broadcaster = Some(broadcaster); self を返す

3) 引数

| 名称 | 型 | 説明 |
|-----|----|------|
| broadcaster | Arc<NotificationBroadcaster> | 通知送出先 |

4) 戻り値

| 型 | 説明 |
|----|------|
| Self | 自身を返す（メソッドチェーン可） |

5) 使用例

```rust
let watcher = ConfigFileWatcher::new(path, indexer, false)?
    .with_broadcaster(broadcaster);
```

6) エッジケース
- None。単純なフィールド設定のみ。

根拠: with_broadcaster（行番号: 不明）

### ConfigFileWatcher::watch

1) 目的と責務
- 親ディレクトリを監視に登録
- 起動時の初期同期を実施（設定と現インデックスの差分）
- イベントループで設定ファイルに関するModify/Createイベントのみ処理
- 追加パスを同期インデクス、削除は告知のみ、最後に通知送信

2) アルゴリズム（ステップ）
- settings_path.parent() を取得し RecursiveMode::NonRecursive でwatch登録
- check_initial_sync().await を試行
- ループ:
  - event_rx.recv().await でイベント受信
  - Ok(event) の場合、event.paths に settings_path が含まれるか判定
  - kindがModify(_)またはCreate(_)なら handle_config_change().await
  - Err(e) はログに警告

3) 引数

| 名称 | 型 | 説明 |
|-----|----|------|
| self | Self（mut, by value） | 実行中はselfを保持し続ける |

4) 戻り値

| 型 | 説明 |
|----|------|
| IndexResult<()> | 実行中は通常戻らない。初期化やwatch登録失敗時にエラー |

5) 使用例
- 上の new の例中で tokio::spawn して使用

6) エッジケース
- 親ディレクトリが取れない→FileReadエラー
- notify側からのErrはログのみで継続
- 設定ファイルがatomic rename等でCreate/Removeの組合せになる場合はCreateで反応、Removeは無視

根拠: watch（行番号: 不明）のイベント種別判定とループロジック。

### Data Contracts（このチャンクから読み取れる契約）

- Settings
  - 関数: Settings::load_from(&Path) -> Result<Settings, _>（正確な型は不明）
  - フィールド: indexing.indexed_paths: IntoIterator<Item=PathBuf> として利用可能（実型は不明）
- SimpleIndexer
  - メソッド: get_indexed_paths(&self) -> IntoIterator<Item=&PathBuf> として利用（実型は不明）
  - メソッド: index_directory(path: &PathBuf, bool, bool) -> Result<Stats, IndexError>（Statsの詳細は不明だが files_indexed, symbols_found を持つ）
- NotificationBroadcaster
  - メソッド: send(FileChangeEvent) -> () ないし戻り値無視（詳細不明）
- FileChangeEvent
  - 列挙子: IndexReloaded を使用

いずれも「このチャンクには現れない」ため、正確な型・実装は不明。使用方法は上記コードからの推測。

## Walkthrough & Data Flow

1) 初期化
- mpscチャンネル（容量10）を作成
- notify::RecommendedWatcherのコールバックで、受信イベントをtx.blocking_sendでチャンネルへ投入
- Settings::load_from で settings.toml を読み、indexed_paths をHashSetに保持

2) watch開始
- 親ディレクトリを監視対象に登録（ファイル直監視の不安定性回避）
- check_initial_sync() で、現在のインデックスと設定の差分を追加インデックス

3) イベントループ
- event_rx.recv().await で通知受信
- イベント内の paths に settings_path が存在かチェックし、Modify/Createのみ処理
- handle_config_change() にて100ms待機後、設定再読込、差分抽出、追加分をインデックス、削除分はログ告知、最後に通知

抜粋（イベントフィルタ部分）:

```rust
if let Some(res) = self.event_rx.recv().await {
    match res {
        Ok(event) => {
            if event.paths.iter().any(|p| p == &self.settings_path) {
                match event.kind {
                    EventKind::Modify(_) | EventKind::Create(_) => {
                        if let Err(e) = self.handle_config_change().await {
                            eprintln!("Config watcher error: {e}");
                        }
                    }
                    _ => {}
                }
            }
        }
        Err(e) => {
            eprintln!("Config watch error: {e}");
        }
    }
}
```

Mermaidシーケンス図（アクター≥3、分岐≥4のため作成）:

```mermaid
sequenceDiagram
    participant FS as File System
    participant Notify as notify::RecommendedWatcher
    participant TX as mpsc::Sender
    participant RX as ConfigFileWatcher.watch
    participant Cfg as Settings
    participant Idx as SimpleIndexer
    participant Brd as NotificationBroadcaster

    FS-->>Notify: Change on settings.toml
    Notify->>TX: callback(res) blocking_send
    TX-->>RX: recv().await -> Event
    RX->>RX: event.paths contains settings_path?
    alt Modify/Create
        RX->>RX: tokio::time::sleep(100ms)
        RX->>Cfg: Settings::load_from(settings_path)
        Cfg-->>RX: new_config
        RX->>RX: diff added / removed
        opt added not empty
            RX->>Idx: write().await; index_directory(added...)
            Idx-->>RX: stats / errors
            RX->>Idx: drop(write lock)
        end
        opt removed not empty
            RX->>RX: log advisories
        end
        RX->>Brd: send(IndexReloaded)
    else Other kinds
        RX->>RX: ignore
    end
```

上記の図は watch 関数と handle_config_change 関数の主要分岐を示す（行番号: 不明）。

## Complexity & Performance

- 初期化
  - 設定ロード: O(P)（P=設定ファイルサイズ）、メモリ O(K)（K=indexed_paths数）
- イベント処理（Modify/Create）
  - 設定再ロード: O(P)
  - 差分計算: O(K)
  - 追加分インデックス: O(ΣFi)（Fi=各ディレクトリ内ファイル数と解析コスト。非同期ではなく同期I/O）
  - メモリ: O(K) + インデクス一時データ（SimpleIndexer側）
- ボトルネック/スケール限界
  - index_directory が同期I/Oで長時間ブロックし、asyncランタイムスレッドを専有
  - mpsc容量10かつ blocking_send により、爆発的イベントでnotify側スレッドが停滞
  - 設定に多数のパス（K大）で差分計算自体はHashSetで効率的だが、再ロード頻度が高いとP/Kに比例してCPU使用
- 実運用負荷要因
  - ファイルI/O（設定ロード）、ディスク走査（インデクス）、通知発行（軽微）
  - ネットワーク/DBは関連なし（このチャンクでは不明）

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 親ディレクトリなし | settings.toml = "/settings.toml" | エラーで即時返却 | watchでNotFoundを返す | 対応済 |
| 設定ファイル読み込み失敗 | パース不能TOML | エラーを返す/ログ | new/handle_config_changeでConfigError | 対応済 |
| atomic rename更新 | 書換時にCreate/Remove | Createで処理、Removeは無視でOK | Modify/Createのみ処理 | 一部対応 |
| 小文字大文字差/シンボリックリンク | event.paths ≠ settings_path（実体同一） | 同一判定で処理 | PathBuf == 比較のみ | 要改善 |
| イベントストーム | 高頻度保存（エディタの自動保存） | デバウンス/コアレッシング | sleep(100ms)のみ | 要改善 |
| チャンネル飽和 | 10件超の連続イベント | 劣化しない（ドロップ/スロットリング） | blocking_sendで停滞 | リスク |
| 長時間インデクシング | 追加パスが巨大 | 非同期化/別スレッド | 同期呼び出し | リスク |
| 削除パス処理 | 設定から削除 | インデクスから除去 | クリーニングは手動案内 | 仕様通り |
| ブロードキャスタなし | broadcaster=None | 通知スキップで継続 | if Some(..) で送信 | 対応済 |
| notifyエラーイベント | Err(e) | ログして継続 | eprintlnのみ | 最低限 |
| Windowsのパス比較 | 大文字小文字非区別 | 誤検知回避 | そのまま | 未対応 |
| 監視対象以外のイベント | 同一ディレクトリの他ファイル | 無視 | pathsにsettings_path一致のみ処理 | 対応済 |

セキュリティチェックリスト
- メモリ安全性
  - unsafe未使用。Arc/RwLockの使用は適切。Use-after-free等の可能性なし。
- インジェクション
  - SQL/コマンド/パス トラバーサル無し。PathBuf比較のみ。
- 認証・認可
  - 対象外。このコードに認可はない。
- 秘密情報
  - ハードコード秘密無し。ログ出力に機密なし。
- 並行性
  - Race condition: RwLockでインデクサ保護。イベント処理は単一ループで逐次。
  - Deadlock: 既知の相互待機はなし。ただし blocking_send による外部スレッド停滞でシステム全体の遅延の可能性。

Rust特有の観点（詳細）
- 所有権/借用
  - ConfigFileWatcher が _watcher を所有してスコープ生存を保証（watch中のドロップ防止）。根拠: structフィールドに保持。
  - RwLockガードを適宜 drop(...) で明示解放（check_initial_sync）。根拠: check_initial_sync 内の drop(indexer) 呼び出し（行番号: 不明）。
- ライフタイム
  - 明示ライフタイム不要。所有型で保持。
- unsafe境界
  - unsafeブロック無し。
- 並行性・非同期
  - Send/Sync: Arc<RwLock<SimpleIndexer>> が実行されている前提（SimpleIndexerがSend+Syncかはこのチャンクでは不明）。
  - await境界: 書きロック取得中にawaitしない（index_directoryは同期）。ただし同期I/Oがランタイムスレッドをブロック。
  - キャンセル: watchループは終了条件なし。シャットダウン経路はこのチャンクには現れない。
- エラー設計
  - IndexError/IndexResultを使用。unwrap/expectは不使用。
  - エラー変換: notifyエラーをIndexError::FileReadに変換。ConfigパースエラーをIndexError::ConfigErrorに変換。

## Design & Architecture Suggestions

1) 非同期適合
- index_directory が重い同期I/Oなら tokio::task::spawn_blocking または非同期API化（async fn index_directory）でランタイムブロッキング回避。
- 追加パスごとにspawn_blockingしてjoin（並列数はセマフォで制御）により全体スループット向上。

2) イベントデバウンス/コアレッシング
- 100ms固定sleepより、タイムウィンドウ内のイベントをまとめるデバウンサを導入（例えば、一定期間イベントを集約し最後の一度だけ処理）。
- 新旧設定のハッシュ（mtime +サイズ or content hash）で冪等化。

3) チャンネル設計
- blocking_sendはウォッチャスレッドを止め得るため、try_sendでドロップor最新のみ保持するリングバッファ戦略に変更、または容量拡大。
- 専用スレッドにcrossbeam-channel等で受け、tokioへ橋渡しでも可。

4) パス正規化
- settings_pathおよび event.paths を std::fs::canonicalize し比較。Windows大小文字非区別、シンボリックリンク解決に対応。
- renameイベントも取り込み対象に追加（EventKind::Modify(_)|Create(_)|Rename(_)|Remove(_) の扱いを再検討）。

5) 観測性向上
- eprintlnではなくtracingクレートを採用し、spanで watch/check_initial_sync/handle_config_change を計装。info/warn/debug/trace レベルの適切な仕分け。
- メトリクス: 受信イベント数、ドロップイベント数、追加/削除パス数、インデクス所要時間、エラー率。

6) API拡張/テスト容易化
- Indexerトレイトを導入し、SimpleIndexer実装に差し替え可能にすることでモック化しやすくする。
- Broadcasterもトレイト化し、sendの副作用検証を容易化。
- シャットダウン用のキャンセルトークン（tokio_util::sync::CancellationToken）対応。

## Testing Strategy (Unit/Integration) with Examples

方針
- ユニットテスト: 差分計算・初期同期・変更処理のロジックを、小さな設定データとモック/フェイクのIndexer/Broadcasterで検証。
- 統合テスト: tempdir に settings.toml を作成、notifyで実際のファイル変更イベントを発火しwatchループの挙動をチェック（時間に依存するためリトライ/待機を頑健に）。

前提
- 現状SimpleIndexer/NotificationBroadcasterは具体型でモック困難。テストモジュール内でフェイク実装に置換できるようトレイト抽象化が望ましい（設計提案参照）。

ユニットテスト例（擬似コード、同一モジュールtests内でprivateメソッドも参照可能）

```rust
#[tokio::test]
async fn test_initial_sync_indexes_missing_paths() {
    // Arrange
    let tmp = tempfile::tempdir().unwrap();
    let settings_path = tmp.path().join("settings.toml");
    std::fs::write(&settings_path, r#"
        [indexing]
        indexed_paths = ["./a", "./b"]
    "#).unwrap();

    let indexer = Arc::new(RwLock::new(FakeIndexer::new_with_indexed(vec!["./a"])));
    let broadcaster = Arc::new(FakeBroadcaster::default());

    let mut watcher = ConfigFileWatcher::new(settings_path.clone(), indexer.clone(), true)
        .unwrap()
        .with_broadcaster(broadcaster.clone());

    // Act
    watcher.check_initial_sync().await.unwrap();

    // Assert
    let calls = indexer.read().await.index_calls();
    assert!(calls.iter().any(|p| p.ends_with("b")));
    assert!(broadcaster.was_sent(FileChangeEvent::IndexReloaded));
}
```

変更検知テスト例（設定ファイル書換による差分）

```rust
#[tokio::test]
async fn test_handle_config_change_adds_and_notifies() {
    // Arrange
    let tmp = tempfile::tempdir().unwrap();
    let settings_path = tmp.path().join("settings.toml");
    std::fs::write(&settings_path, r#"
        [indexing]
        indexed_paths = ["./x"]
    "#).unwrap();

    let indexer = Arc::new(RwLock::new(FakeIndexer::new_with_indexed(vec!["./x"])));
    let broadcaster = Arc::new(FakeBroadcaster::default());

    let mut watcher = ConfigFileWatcher::new(settings_path.clone(), indexer.clone(), false)
        .unwrap()
        .with_broadcaster(broadcaster.clone());

    // 更新: x + y
    std::fs::write(&settings_path, r#"
        [indexing]
        indexed_paths = ["./x", "./y"]
    "#).unwrap();

    // Act
    watcher.handle_config_change().await.unwrap();

    // Assert
    let calls = indexer.read().await.index_calls();
    assert!(calls.iter().any(|p| p.ends_with("y")));
    assert!(broadcaster.was_sent(FileChangeEvent::IndexReloaded));
}
```

統合テスト例（notify動作確認。時間依存のため適宜待機）

```rust
#[tokio::test]
async fn test_watch_loop_reacts_to_modify_event() {
    // Arrange
    let tmp = tempfile::tempdir().unwrap();
    let settings_path = tmp.path().join("settings.toml");
    std::fs::write(&settings_path, r#"[indexing] indexed_paths=[]"#).unwrap();

    let indexer = Arc::new(RwLock::new(FakeIndexer::new_with_indexed(vec![])));
    let broadcaster = Arc::new(FakeBroadcaster::default());

    let watcher = ConfigFileWatcher::new(settings_path.clone(), indexer.clone(), true)
        .unwrap()
        .with_broadcaster(broadcaster.clone());

    tokio::spawn(async move {
        let _ = watcher.watch().await;
    });

    // Act: ファイル更新
    std::fs::write(&settings_path, r#"[indexing] indexed_paths=["./z"]"#).unwrap();

    // Assert: リトライ待機
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    assert!(broadcaster.was_sent(FileChangeEvent::IndexReloaded));
    assert!(indexer.read().await.index_calls().iter().any(|p| p.ends_with("z")));
}
```

注意: FakeIndexer/FakeBroadcasterはテスト用のダミー実装（このチャンクには現れない）。

## Refactoring Plan & Best Practices

- 非同期対応
  - index_directory呼び出しを tokio::task::spawn_blocking に包み、重い処理のランタイムブロックを回避。複数パスはSemaphoreで併行度制御。
- イベント処理改善
  - デバウンサ/スロットリング導入（例: 最終イベントからXms後に一度だけ実行）。
  - mpscチャンネル容量拡大や最新イベントのみ保持（try_send + 置換）戦略。
- パス処理
  - canonicalize + case-insensitive比較（ターゲットプラットフォーム毎の方針）。
  - EventKindのRename/Removeも考慮（atomic rename対応強化）。
- API/設計
  - Indexer/Broadcasterトレイト化でテスト容易性向上。with_broadcasterの代わりにBuilderパターンでオプション注入。
  - シャットダウンフック（CancellationToken）を watch で受け取り、優雅に終了。
- ロギング/メトリクス
  - tracing導入、spanで処理時間と結果記録。metrics（prometheus等）でイベント数、エラー数、所要時間。

## Observability (Logging, Metrics, Tracing)

- Logging
  - eprintln→tracing::{error,warn,info,debug,trace}。環境に応じたSubscriberで出力切替。
  - 重要イベント（config再読込開始/成功/失敗、差分数、各パスのインデクス結果）をinfo以上で記録。
- Metrics
  - counter: config_events_total, index_added_paths_total, index_failures_total
  - histogram: index_duration_seconds, config_reload_duration_seconds
  - gauge: pending_events（チャンネル長を観測できるよう設計変更を検討）
- Tracing
  - span: "watch_loop", "handle_config_change", "index_directory"（pathタグ、結果タグ付与）
  - error時に context を付与（path, event.kind, diff sizes）

## Risks & Unknowns

- Unknowns（このチャンクには現れない）
  - Settingsの正確なスキーマ、indexing.indexed_pathsの型
  - SimpleIndexer の同期/非同期性、スレッド安全性（Send/Sync）
  - NotificationBroadcaster::send の戻り値/失敗時挙動
  - IndexError の全バリアント
- リスク
  - ランタイムブロック：重いインデクスによるtokioワーカースレッドの占有
  - イベント滞留：blocking_send + 小容量チャンネル
  - パス一致の脆弱性：非正規化比較によるイベント見落とし
  - シャットダウン手段がなく永続ループが残存する可能性（プロセス終了時にのみ停止）
  - 多数追加パス時の単一書きロック保持時間が長く、他のインデクサ操作と競合する可能性

## Walkthrough & Data Flow

（上に記載済みのため、参照してください）