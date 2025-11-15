# mcp\http_server.rs Review

## TL;DR

- 目的: MCP向けの永続HTTPサーバを起動し、SSEによるリアルタイム通知と簡易OAuthフロー、トークン検証を提供する。
- 主な公開API: pub async fn serve_http(config, watch, bind) -> anyhow::Result<()>（feature http-serverの有無で実装が分岐）。
- コアロジック: Index/Config/FileSystemの3種のウォッチャ起動、SSEサーバ構築、Axumルーター作成、Bearerトークン検証ミドルウェア、Ctrl+Cによるグレースフルシャットダウン。
- 複雑箇所: 複数の非同期タスクとCancellationTokenの連携、SSE接続毎のサービス生成と通知購読、OAuth関連のエンドポイント設計。
- 重大リスク: ダミーOAuth/トークンのまま本番利用すると認証・認可が機能しない、redirect_uri/stateの無検証によりOpen Redirect/XSSの可能性、TLS未使用。
- Rust安全性: unsafe未使用、Arc/RwLockで共有状態管理。長期ロック内の同期I/Oには注意が必要。
- 不明点: CodeIntelligenceServerの仕様・インデクサの内部処理・外部SseServerのキャンセル動作詳細（このチャンクには現れない）。

## Overview & Purpose

このファイルはMCPのHTTPサーバの実装を提供する。SSE（Server-Sent Events）により、複数クライアントに対してインデックス更新などのリアルタイム通知を配信し、簡易なOAuthエンドポイント群（metadata/register/token/authorize）を提供する。主要機能は以下の通り。

- serve_http: 設定・ウォッチ有無・バインドアドレスを受け取り、サーバを起動。SSEルーターをBearerトークンミドルウェアで保護し、OAuth/Healthエンドポイントを公開。Ctrl+Cで優雅に停止。
- Index/FileSystem/Configウォッチャ: インデックスファイルのポーリング、ファイルシステムイベント監視、設定ファイルの監視を起動し、通知ブロードキャスターで各クライアントへイベント配信。
- SSE接続ハンドリング: 接続毎にCodeIntelligenceServerを生成し、通知購読タスクをspawn。
- 簡易OAuth: 認可コードフローを模したダミー実装（無登録検証、固定コード/トークン）を提供。

## Structure & Key Components

| 種別 | 名前 | 公開範囲 | 責務 | 複雑度 |
|------|------|----------|------|--------|
| Function | serve_http (cfg(feature="http-server")) | pub | HTTP/SSEサーバ起動、ウォッチャ起動、ルーター構築、終了制御 | High |
| Function | serve_http (cfg(not(feature="http-server"))) | pub | 機能未コンパイル時の案内・終了 | Low |
| Function | health_check | private | ヘルスチェック応答 ("OK") | Low |
| Closure | oauth_metadata | private | OAuthメタデータJSONを返す | Low |
| Function | oauth_register | private | ダミー登録（入力を反映したJSON） | Low |
| Function | oauth_token | private | x-www-form-urlencodedのパース、ダミートークン発行 | Med |
| Function | oauth_authorize | private | ダミー認可画面HTML、redirect_uriへcode/stateを付加 | Med |
| Function | shutdown_signal | private | Ctrl+C待ち | Low |
| Function | validate_bearer_token | private | /mcp/以下のBearerトークン検証ミドルウェア | Med |
| Struct(外部) | SseServer/SseServerConfig | 外部rmcp | SSEサーバ構築・ルーティング | Med |
| Struct(外部) | NotificationBroadcaster | crate::mcp | イベント発行/購読（SSE向け） | Med |
| Struct(外部) | SimpleIndexer | crate | インデックス共有状態 | Med |
| Struct(外部) | IndexPersistence | crate | 既存インデックスのロード/存在確認 | Low |
| Struct(外部) | IndexWatcher | crate::mcp::watcher | ポーリング型インデックス監視 | Med |
| Struct(外部) | FileSystemWatcher | crate::indexing | FSイベント監視 | Med |
| Struct(外部) | ConfigFileWatcher | crate::indexing | 設定ファイル監視 | Med |

### Dependencies & Interactions

- 内部依存
  - serve_httpが全体のオーケストレーションを担当し、health_check, oauth_* 関数、validate_bearer_token、shutdown_signalを定義・使用。
  - serve_http内でNotificationBroadcaster, SimpleIndexer, IndexPersistence, IndexWatcher, FileSystemWatcher, ConfigFileWatcher, SseServerを生成・起動。
  - SSE接続毎にCodeIntelligenceServerを生成し、NotificationBroadcasterからsubscribeしたreceiverを渡して通知リスナーをspawn。

- 外部依存（抜粋）

| クレート/モジュール | 用途 |
|---------------------|------|
| axum | HTTPルーター、ミドルウェア、レスポンス、エクストラクタ |
| tokio | 非同期ランタイム、spawn、select、signal、TcpListener |
| tokio_util::sync::CancellationToken | 協調キャンセル |
| rmcp::transport::SseServer | SSEルーター・サーバ生成 |
| serde_json | JSONレスポンス生成 |
| serde_urlencoded | application/x-www-form-urlencodedのパース |
| std::sync::Arc, tokio::sync::RwLock | 共有状態（インデクサ）の安全な共有 |

- 被依存推定
  - このモジュールはCLIや上位サーバ起動ロジックから呼び出される想定（例: main.rsなど）。外部クライアント（ブラウザ、IDE拡張、Claude Code等）がHTTP/SSE/OAuthエンドポイントを利用。

## API Surface (Public/Exported) and Data Contracts

| API名 | シグネチャ | 目的 | Time | Space |
|-------|------------|------|------|-------|
| serve_http (enabled) | pub async fn serve_http(config: crate::Settings, watch: bool, bind: String) -> anyhow::Result<()> | HTTP/SSEサーバ起動と全体オーケストレーション | 起動O(1)、ランタイムはイベント駆動 | 共有状態はインデクササイズに依存 |
| serve_http (disabled) | pub async fn serve_http(_config: crate::Settings, _watch: bool, _bind: String) -> anyhow::Result<()> | 未コンパイル時の案内・終了 | O(1) | O(1) |
| /health | GET -> "OK" | ヘルスチェック | O(1) | O(1) |
| /.well-known/oauth-authorization-server | GET -> OAuthメタデータJSON | OAuthメタデータ提供 | O(1) | O(1) |
| /oauth/register | POST JSON -> 登録情報JSON | ダミークライアント登録 | O(n)（JSONサイズ） | O(n) |
| /oauth/token | POST form -> トークンJSON/エラーJSON | 認可コードの交換（ダミー） | O(n)（フォームサイズ） | O(n) |
| /oauth/authorize | GET query -> HTML | 認可画面とredirect | O(n)（テンプレート生成） | O(n) |
| /mcp/sse | SSEストリーム（ミドルウェア保護） | リアルタイム通知（rmcp実装） | O(1)/イベント駆動 | 接続数に比例 |
| /mcp/message | POST（ミドルウェア保護） | メッセージ送信（rmcp実装） | O(n)（ペイロード） | O(n) |

以下、主要APIの詳細。

### serve_http（feature "http-server" 有効時）

1) 目的と責務
- 設定に従いHTTPサーバ・SSEサーバを起動し、インデクサとウォッチャ、通知ブロードキャスターを構成。
- ルーターにOAuth/Health/MCP（SSE/POST）エンドポイントを登録。
- Ctrl+Cで協調キャンセルし、タスクを終了。

2) アルゴリズム（ステップ分解）
- 設定ログ出力、NotificationBroadcasterとArc<RwLock<SimpleIndexer>>生成。
- IndexPersistenceで既存インデックスをロード（あれば置換）。
- CancellationToken作成。
- watchフラグに応じIndexWatcherをspawn。
- file_watch.enabledまたはwatchがtrueならFileSystemWatcher/ConfigFileWatcherをspawn。
- bindアドレスをSocketAddrにparseしSseServerConfig作成、SseServer生成。
- sse_server.with_serviceで各SSE接続毎にCodeIntelligenceServerを生成し通知リスナーをspawn。
- validate_bearer_tokenミドルウェアで保護したSSEルーターを作成。
- OAuth/Healthエンドポイントを持つAxum Routerにマージ。
- TcpListenerでbindしaxum::serve起動。tokio::selectでサーバ終了とCtrl+Cを待機し、キャンセル発火。

3) 引数

| 引数 | 型 | 説明 |
|------|----|------|
| config | crate::Settings | 全体設定（mcp.debug, index_path, file_watch, workspace_root等） |
| watch | bool | ポーリング型IndexWatcherの有効化 |
| bind | String | "127.0.0.1:3118" のようなバインドアドレス |

4) 戻り値

| 型 | 説明 |
|----|------|
| anyhow::Result<()> | サーバ起動・終了の結果。エラー時は詳細を含む。 |

5) 使用例

```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = crate::Settings::load_default()?;
    // ローカルでSSE/HTTPを起動
    mcp::http_server::serve_http(settings, true, "127.0.0.1:3118".into()).await
}
```

6) エッジケース
- bindが不正（parse失敗）→ anyhowエラーを返す。
- 既存インデックスロード失敗→ログ出力し空インデックスで継続。
- FileSystemWatcher/ConfigFileWatcher生成失敗→ログ出力し監視無しで継続。
- SSE接続時の通知購読タスクはHandshake完了を待つため1秒スリープ。

注: 行番号は本チャンクで提供されていないため、主張に行番号は付せません。

### validate_bearer_token

```rust
async fn validate_bearer_token(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    let path = req.uri().path();

    if path.starts_with("/mcp/") {
        if let Some(auth_header) = req.headers().get("Authorization") {
            if let Ok(auth_str) = auth_header.to_str() {
                if auth_str == "Bearer mcp-access-token-dummy" {
                    eprintln!("MCP request authorized with Bearer token");
                    return Ok(next.run(req).await);
                }
            }
        }

        if req.method() == axum::http::Method::OPTIONS {
            return Ok(next.run(req).await);
        }

        eprintln!("MCP request rejected - invalid or missing Bearer token");
        return Err(axum::http::StatusCode::UNAUTHORIZED);
    }

    Ok(next.run(req).await)
}
```

1) 目的と責務
- /mcp/配下のSSE/POSTの認可検証。Bearerトークンが固定文字列の場合のみ通過。
- CORSプリフライト（OPTIONS）は無条件許可。

2) アルゴリズム
- パスが/mcp/で始まるか確認。
- Authorizationヘッダの文字列比較。
- OPTIONSはバイパス。
- それ以外は401。

3) 引数・戻り値
- 引数: Request, Next
- 戻り値: Ok(Response) or Err(StatusCode::UNAUTHORIZED)

4) 使用例（ミドルウェア適用）
```rust
let protected_sse_router = sse_router.layer(axum::middleware::from_fn(validate_bearer_token));
```

5) エッジケース
- Authorizationヘッダがない/不正: 401
- Authorizationヘッダのエンコード不正: to_str()失敗で401
- 非/mcp/パス: 検証無し

### OAuth関連

health_check（短いので全文）

```rust
async fn health_check() -> &'static str {
    "OK"
}
```

oauth_metadata（Closure）
- 認可サーバメタデータJSONを返す。issuer/authorization_endpoint/token_endpoint等をbindアドレスから構築。
- 注意: "http://"での公開、TLS非対応。

oauth_register
```rust
async fn oauth_register(
    axum::Json(payload): axum::Json<serde_json::Value>,
) -> axum::Json<serde_json::Value> {
    eprintln!("OAuth register endpoint called with: {payload:?}");
    axum::Json(serde_json::json!({
        "client_id": "dummy-client-id",
        "client_secret": "",
        "client_id_issued_at": 1234567890,
        "grant_types": ["authorization_code", "refresh_token"],
        "response_types": ["code"],
        "redirect_uris": payload.get("redirect_uris").unwrap_or(&serde_json::json!([])).clone(),
        "client_name": payload.get("client_name").unwrap_or(&serde_json::json!("MCP Client")).clone(),
        "token_endpoint_auth_method": "none"
    }))
}
```

- 任意の入力を反映しつつダミー登録結果を返す。入力検証・永続化は無し。

oauth_token
```rust
async fn oauth_token(body: String) -> axum::Json<serde_json::Value> {
    eprintln!("OAuth token endpoint called with body: {body}");
    let params: std::collections::HashMap<String, String> =
        serde_urlencoded::from_str(&body).unwrap_or_default();
    let grant_type = params.get("grant_type").cloned().unwrap_or_default();
    let code = params.get("code").cloned().unwrap_or_default();

    if grant_type == "refresh_token" {
        return axum::Json(serde_json::json!({
            "error": "unsupported_grant_type",
            "error_description": "only authorization_code is supported"
        }));
    }

    if grant_type == "authorization_code" && code == "dummy-auth-code" {
        axum::Json(serde_json::json!({
            "access_token": "mcp-access-token-dummy",
            "token_type": "Bearer",
            "expires_in": 3600,
            "scope": "mcp"
        }))
    } else {
        axum::Json(serde_json::json!({
            "error": "invalid_grant",
            "error_description": "Invalid authorization code or grant type"
        }))
    }
}
```

- フォームのパースに失敗時は空パラメータとして処理。refresh_tokenは明示拒否。認可コード"dummy-auth-code"のみ受理。

oauth_authorize（抜粋）
```rust
async fn oauth_authorize(
    axum::extract::Query(params): axum::extract::Query<
        std::collections::HashMap<String, String>,
    >,
) -> impl axum::response::IntoResponse {
    let redirect_uri = params
        .get("redirect_uri")
        .cloned()
        .unwrap_or_else(|| "http://localhost:3118/callback".to_string());
    let state = params.get("state").cloned().unwrap_or_default();
    let callback_url = format!("{redirect_uri}?code=dummy-auth-code&state={state}");
    let html = format!(r#" ... <button onclick="window.location.href='{callback_url}'">Continue</button> ... "#);
    axum::response::Html(html)
}
```

- callback_urlをそのままHTMLに埋め込む。redirect_uri/stateの許可リスト検証やエスケープは無し。

使用例（curl）
```bash
# メタデータ
curl http://127.0.0.1:3118/.well-known/oauth-authorization-server

# 認可
open "http://127.0.0.1:3118/oauth/authorize?redirect_uri=http://localhost:3118/callback&state=xyz"

# トークン交換
curl -X POST http://127.0.0.1:3118/oauth/token \
  -H "Content-Type: application/x-www-form-urlencoded" \
  --data "grant_type=authorization_code&code=dummy-auth-code"

# SSE接続（ダミートークン必須）
curl -H "Authorization: Bearer mcp-access-token-dummy" http://127.0.0.1:3118/mcp/sse
```

## Walkthrough & Data Flow

- 起動時
  - 設定からNotificationBroadcaster(容量100)とArc<RwLock<SimpleIndexer>>を作成。
  - IndexPersistenceで既存インデックスロードを試みる（失敗時はログのみ）。
  - CancellationTokenを作成。
  - watch=trueならIndexWatcherを5秒間隔でspawn（tokio::selectでct連携）。
  - file_watch.enabledまたはwatch=trueならFileSystemWatcher/ConfigFileWatcherをspawn（ct連携）。
  - rmcp::SseServerを"/mcp/sse"と"/mcp/message"で構築。SSE keep-aliveは15秒。
  - with_serviceでSSE接続毎にCodeIntelligenceServerを生成し、通知購読タスクをspawn。
  - validate_bearer_tokenで保護したSSEルーターをAxum Routerにマージ。OAuthとHealthは非保護。
  - TcpListenerでbind。axum::serveを起動し、Ctrl+Cを待ってct.cancel()。

Mermaidフローチャート（主要分岐）

```mermaid
flowchart TD
  A[serve_http開始] --> B[NotificationBroadcaster作成]
  B --> C[SimpleIndexer(RwLock)作成]
  C --> D[IndexPersistence.exists?]
  D -->|Yes| E[load_with_settings -> 成功/失敗]
  D -->|No| F[空インデックスで継続]
  E --> G[loadedをRwLockに反映]
  F --> G
  G --> H[CancellationToken作成]
  H --> I{watch or file_watch.enabled?}
  I -->|watch=true| J[IndexWatcher.spawn(select: watch vs ct)]
  I -->|file_watch.enabled=true| K[FileSystemWatcher.spawn]
  I -->|file_watch.enabled=true| L[ConfigFileWatcher.spawn]
  I --> M[継続]
  M --> N[SseServerConfig作成(bind, paths, ct)]
  N --> O[SseServer::new]
  O --> P[with_service: 接続毎にCodeIntelligenceServer生成]
  P --> Q[通知リスナーspawn(select: listener vs ct)]
  Q --> R[validate_bearer_tokenミドルウェア適用]
  R --> S[OAuth/Healthルーティング]
  S --> T[TcpListenerでbind]
  T --> U[axum::serve開始]
  U --> V{tokio::select}
  V -->|server終了| W[Result処理]
  V -->|Ctrl+C| X[ct.cancel() -> graceful shutdown]
```

上記の図はserve_http関数の主要フローを示す（このチャンクには行番号情報がないため特定行の紐付けは不明）。

Mermaidシーケンス図（SSE接続時）

```mermaid
sequenceDiagram
  actor Client
  participant Router
  participant Middleware as validate_bearer_token
  participant SSE as rmcp::SseServer
  participant Service as CodeIntelligenceServer
  participant Broadcaster as NotificationBroadcaster

  Client->>Router: GET /mcp/sse (Authorization: Bearer ...)
  Router->>Middleware: Request
  alt Bearerトークン有効 or OPTIONS
    Middleware-->>Router: Ok
    Router->>SSE: 接続処理
    SSE->>Service: with_service()で新規インスタンス
    Service->>Broadcaster: subscribe()
    Service->>Service: 1秒待機（ハンドシェイク想定）
    Service->>Client: SSEイベント送出（通知）
  else 無効トークン
    Middleware-->>Client: 401 Unauthorized
  end
  Client-->>Router: Ctrl+C/切断
  Router-->>Service: ct.cancel()で停止
```

上記の図はSSE接続時の主な呼び出し関係を示す（このチャンクには行番号情報がないため特定行の紐付けは不明）。

## Complexity & Performance

- 時間計算量
  - serve_http起動: O(1)（初期化処理）
  - IndexWatcher: 監視対象に対しポーリング間隔毎にO(N)（Nは監視対象エントリ数）と推定（詳細はこのチャンクには現れない）。
  - FileSystemWatcher/ConfigFileWatcher: イベント駆動、各イベント処理はO(k)（変更対象サイズk）。
  - SSE: 通知送出はイベント駆動、接続数Cに対し送信はO(C)ブロードキャスト。
  - OAuth token: フォームパース O(n)（ボディ長）。

- 空間計算量
  - インデクサ: 保持するシンボル数に比例。
  - ブロードキャスター: チャンネル容量100 + 購読者数に比例。
  - SSE接続: 各接続毎のサービスインスタンスとバッファに比例。

- ボトルネック・スケール限界
  - RwLockの書き込みロック保持時間が長い場合、SSE応答遅延の可能性。
  - 単一プロセス・単一ノード。接続数・イベント頻度が増加するとブロードキャストがCPU/ネットワークに負荷。
  - eprintlnベースのロギングは高負荷時に標準出力がボトルネックになり得る。
  - TLS非対応のため、ネットワーク上での盗聴・改ざんリスク（性能ではなくセキュリティ面）。

- 実運用負荷要因
  - I/O: ファイルシステムウォッチ、インデックスロード/保存。
  - ネットワーク: SSEのkeep-alive（15秒）、複数クライアントへのイベント送出。
  - DB: このチャンクには該当なし。

## Edge Cases, Bugs, and Security

| エッジケース | 入力例 | 期待動作 | 実装 | 状態 |
|-------------|--------|----------|------|------|
| 不正bind | "foo:bar" | Err(ParseError) | SocketAddr::parse()? | OK |
| インデックスロード失敗 | 壊れたindex | ログ出力し起動継続 | match Errでログ | OK |
| FileWatcher生成失敗 | 権限不足 | ログ出力し監視なし継続 | Err時にログ | OK |
| Authorization欠如 | /mcp/sse | 401 | ヘッダ無しでUNAUTHORIZED | OK |
| CORSプリフライト | OPTIONS /mcp/sse | 200通過 | OPTIONSは許可 | OK |
| 無効grant_type | refresh_token | エラーJSON | 明示拒否 | OK |
| 無効code | "bad" | invalid_grant返却 | 条件分岐あり | OK |
| redirect_uriの悪意 | "';alert(1);//" | XSS/HTML崩壊防止 | エスケープ無し | 問題あり |
| Open Redirect | redirect_uri外部 | 許可リスト検証 | 検証無し | 問題あり |
| 平文通信 | http:// | TLS強制 | http固定 | 問題あり |
| ログ漏洩 | body/params出力 | 最小化 | eprintlnで出力 | 注意 |
| 長期ロック | RwLock write中に重い処理 | ロック短縮 | 不明（他コード次第） | 不明 |
| キャンセル伝播 | ct.cancel() | 全タスク停止 | selectで対応 | 概ねOK |
| SSE認可固定 | "mcp-access-token-dummy" | 環境毎に管理 | 固定文字列 | 問題あり |

セキュリティチェックリスト

- メモリ安全性
  - Buffer overflow / Use-after-free / Integer overflow: Rust安全性と標準ライブラリ使用で問題なし。unsafe無し。
- インジェクション
  - XSS: oauth_authorizeのHTMLへ未エスケープのcallback_urlを挿入。redirect_uri/stateにクオート破壊文字を含めるとXSS/HTML崩壊の恐れ。
  - Path traversal/Command/SQL: 該当なし（このチャンクには現れない）。
- 認証・認可
  - Bearerトークン固定文字列。実運用では不適切。トークン失効/検証無し。SSE/POSTが実質保護されていない。
  - OAuth: クライアント登録・redirect_uriの検証無し。Open Redirectの可能性。
- 秘密情報
  - client_secretは空文字だが、ログにペイロードを出力するため機密情報があれば漏洩。
- 並行性
  - Race condition: RwLockで共有状態保護。writeロック時間に注意。
  - Deadlock: 明示的に懸念箇所なし（外部処理次第）。
- ログ漏洩
  - eprintlnでリクエストボディやパラメータを出力。公開環境では抑制・サニタイズ推奨。

Rust特有の観点

- 所有権
  - Arc/RwLockで共有。configはArc::new(config.clone())で複数タスクへmove。クローン戦略は妥当。
- 借用
  - 非同期タスクへmoveする際にclone済み値を渡しており、借用期間の衝突は回避。
- ライフタイム
  - 明示的なライフタイム不要。非同期タスクの捕捉値は'staticに近い所有値をmove。
- unsafe境界
  - unsafe未使用。安全性はRustの所有/借用制約に依存。
- 並行性・非同期
  - CancellationTokenを各タスクに渡しtokio::selectで中断可能に設計。Send/SyncはArc<RwLock<...>>で適切に扱われる想定。
  - await境界: watch()/start_notification_listener()へのawaitはselectによりキャンセル可能。
  - キャンセル: ctrl_cでct.cancel()、spawnしたウォッチャ/通知リスナーは停止。
- エラー設計
  - トップレベルはanyhow::Result。内部で多数のErrをログ出力して握り潰し継続。API応答はJSONエラーを返却。

## Design & Architecture Suggestions

- 認証/認可の本実装化
  - ダミートークン・ダミーOAuthを別のdevモードに限定。プロダクションではOIDC/OAuthサーバと連携し、redirect_uriの許可リスト検証とHTMLエスケープを実施。
- セキュアな通信
  - TLS対応（Rustls + Axum/Tower）。メタデータのissuer/endpointもhttpsへ。
- ミドルウェア強化
  - validate_bearer_tokenをJWT検証等に差し替え。CORS設定（Origin/Methods/Headers）をtower-httpで構築。
- ロギング/メトリクス
  - eprintlnではなくtracing（span/level）とメトリクス（prometheus）導入。PII/機密のログ抑制。
- 構成分離
  - OAuthダミーエンドポイントを別モジュールへ分割。SSEサービス生成ロジックも専用モジュール/ファクトリへ。
- リソース管理
  - RwLockのwriteロック範囲を最小化。インデックス更新の重い処理はロック外で計算し、最後に短期writeで差分適用。
- コンフィグ
  - Settingsからbindやwatch、debugフラグをミドルウェア・サービスに一貫して渡すための構造体（Context）定義。

## Testing Strategy (Unit/Integration) with Examples

- ユニットテスト
  - validate_bearer_token: Authorizationあり/なし/不正/OPTIONSの分岐テスト。
  - oauth_token: grant_type=authorization_code + 正/誤code、refresh_token拒否、パース失敗時挙動。

- 統合テスト
  - Router全体をaxum::Routerで起動し、hyper::ClientやreqwestでHTTPコール検証。
  - SSE: フェイククライアントで接続し、NotificationBroadcasterへイベント流し、SSE受信確認（rmcpのテストサポート次第）。

例: validate_bearer_tokenの単体テスト

```rust
#[tokio::test]
async fn test_validate_bearer_token() {
    use axum::{Router, routing::get, http::{Request, Method}};
    use tower::ServiceExt;

    async fn handler() -> &'static str { "ok" }
    let app = Router::new()
        .route("/mcp/sse", get(handler))
        .layer(axum::middleware::from_fn(validate_bearer_token));

    // 1) 正常トークン
    let req = Request::builder()
        .uri("/mcp/sse")
        .header("Authorization", "Bearer mcp-access-token-dummy")
        .body(axum::body::Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert!(resp.status().is_success());

    // 2) 不正トークン
    let req = Request::builder()
        .uri("/mcp/sse")
        .header("Authorization", "Bearer invalid")
        .body(axum::body::Body::empty()).unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), axum::http::StatusCode::UNAUTHORIZED);

    // 3) OPTIONSは許可
    let req = Request::builder()
        .method(Method::OPTIONS)
        .uri("/mcp/sse")
        .body(axum::body::Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert!(resp.status().is_success());
}
```

例: oauth_tokenテスト

```rust
#[tokio::test]
async fn test_oauth_token() {
    // 正常コード
    let body = "grant_type=authorization_code&code=dummy-auth-code".to_string();
    let resp = oauth_token(body).await;
    let v = resp.0;
    assert_eq!(v["access_token"], "mcp-access-token-dummy");

    // refresh_token拒否
    let body = "grant_type=refresh_token".to_string();
    let v = oauth_token(body).await.0;
    assert_eq!(v["error"], "unsupported_grant_type");

    // 無効コード
    let body = "grant_type=authorization_code&code=bad".to_string();
    let v = oauth_token(body).await.0;
    assert_eq!(v["error"], "invalid_grant");
}
```

例: Router統合起動（簡易）

```rust
#[tokio::test]
async fn test_health_endpoint() {
    use axum::{Router, routing::get};
    let router = Router::new().route("/health", get(health_check));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let server = tokio::spawn(axum::serve(listener, router));

    let body = reqwest::get(format!("http://{}/health", addr)).await.unwrap().text().await.unwrap();
    assert_eq!(body, "OK");

    server.abort();
}
```

## Refactoring Plan & Best Practices

- 認証/認可の抽象化
  - Token検証をtrait（TokenValidator）に抽象化し、ダミー実装と本番実装をfeatureで切替。
- ルーター構築の分離
  - RouterBuilderモジュールを作成し、OAuth/Health/SSE/ミドルウェア設定を分割。
- エラー整備
  - anyhow一辺倒から、ドメイン固有エラー型へ（thiserror）移行。ユーザ向けレスポンスとログ向け詳細を分離。
- ロギング切替
  - tracingで構造化ログ。環境変数でレベル制御。debugフラグの分岐をログレベルに統合。
- セキュリティ改善
  - redirect_uriホワイトリスト、HTML/JSの適切なエスケープ（askama/tinytemplateやaxum::response::Htmlの安全テンプレート化）。
  - HTTPS必須化、HSTS設定、CORS厳格化。
- リソース・並行性
  - RwLockの短期化、計算部分はロック外で行う。可能ならRwLock→dashmap等の分割ロック構造採用。

## Observability (Logging, Metrics, Tracing)

- ログ
  - eprintlnをtracingへ。spanで接続毎、リクエスト毎のコンテキスト付与。
  - リクエストボディ/パラメータのマスキング（PII/秘密）。
- メトリクス
  - 接続数、イベント送信数、ウォッチャイベント数、エラー率、トークン拒否率。
  - prometheus + /metricsエンドポイント。
- トレーシング
  - HTTPミドルウェアでtrace-idを生成/伝播。SSEイベントにもtrace-idを埋め込み。
- 可観測性フック
  - CancellationTokenキャンセル時のタスク停止時間を計測し、遅延をアラート。

## Risks & Unknowns

- Unknown
  - CodeIntelligenceServerの内部処理、start_notification_listenerの負荷・ロック戦略（このチャンクには現れない）。
  - rmcp::transport::SseServerのキャンセル挙動（ct連携の詳細は外部実装依存）。
  - SimpleIndexerのメモリ使用量・更新頻度・永続化方式（このチャンクには現れない）。
- Risks
  - ダミー認証のまま運用するリスク（不正アクセス、データ漏洩）。
  - redirect_uri/stateの未検証によるOpen Redirect/XSS。
  - HTTP平文通信による盗聴・改ざん。
  - 標準出力ログへの機密情報出力。
  - 高負荷時のRwLock競合による待ち時間増大。

以上により、このファイルは開発向けのMCPサーバ実装としては有用だが、本番利用に向けて認証/認可・通信・ロギング/可観測性・ロック戦略の強化が必要。