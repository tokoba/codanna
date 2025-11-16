//! Phase 1: Windows I/O Error Retry Helpers (Section 11.7.2)
//!
//! Windows環境での一時的なI/Oエラー（AV干渉等）に対する
//! リトライヘルパー関数とエラー分類ロジック。

use std::time::Duration;

// ============================================================================
// 内部ヘルパー関数
// ============================================================================

/// エラーメッセージから os error コードを抽出
/// "os error 5" または "code: 5" 形式に対応
fn extract_error_code_from_message(msg: &str) -> Option<i32> {
    if msg.contains("os error") {
        msg.split("os error")
            .nth(1)
            .and_then(|s| s.trim().trim_start_matches(':').trim().split(')').next())
            .and_then(|s| s.trim().parse::<i32>().ok())
    } else if msg.contains("code:") {
        // "code: 5" 形式のエラーメッセージに対応（OpenWriteError等）
        msg.split("code:")
            .nth(1)
            .and_then(|s| s.trim().split_whitespace().next())
            .and_then(|s| s.trim_end_matches(',').parse::<i32>().ok())
    } else {
        None
    }
}

// ============================================================================
// ヘルパー関数 (Section 11.7.2.2)
// ============================================================================

/// heap_sizeの正規化（絶対最小15MB、最大2GB）
/// Windows環境では最小50MB/推奨100MB以上を強く推奨。
pub fn normalized_heap_bytes(heap_bytes: usize) -> usize {
    const MIN_HEAP: usize = 15 * 1024 * 1024; // 15MB
    const MAX_HEAP: usize = 2 * 1024 * 1024 * 1024; // 2GB
    heap_bytes.clamp(MIN_HEAP, MAX_HEAP)
}

/// リトライ用バックオフ（ミリ秒）
/// attempt=0: 80–120ms（初回）
/// attempt>=1: 100→200→400→800ms + 0–50msジッター
pub fn backoff_with_jitter_ms(attempt: u32) -> u64 {
    fn pseudo_jitter(limit_inclusive: u64) -> u64 {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_else(|_| Duration::from_nanos(0))
            .subsec_nanos() as u64;
        nanos % (limit_inclusive + 1)
    }

    if attempt == 0 {
        return 80 + pseudo_jitter(40);
    }

    let base = match attempt {
        1 => 100,
        2 => 200,
        3 => 400,
        _ => 800,
    };

    base + pseudo_jitter(50)
}

// ============================================================================
// エラー分類 (Section 11.7.2.3)
// ============================================================================

/// "Index writer was killed"（致命的）を型ベースで検出
pub fn is_writer_killed(err: &tantivy::TantivyError) -> bool {
    match err {
        tantivy::TantivyError::ErrorInThread(msg) => msg.contains("Index writer was killed"),
        _ => false,
    }
}

/// Windows一時I/Oエラー（AV干渉等）の包括判定
/// 常時リトライ: 32/33/1224/995
/// 条件付き: 5 (PermissionDenied - Phase 0観測でセグメント書き込み時に頻発)
/// 限定的: 80/183/145 (1-2回推奨)
pub fn is_windows_transient_io_error(err: &tantivy::TantivyError) -> bool {
    if is_writer_killed(err) {
        return false;
    }

    // エラーメッセージから直接os errorコードを抽出（フォールバック）
    let msg = err.to_string();
    let code_from_msg = extract_error_code_from_message(&msg);

    let mut src = std::error::Error::source(err);
    while let Some(e) = src {
        if let Some(ioe) = e.downcast_ref::<std::io::Error>() {
            let code = ioe.raw_os_error().or(code_from_msg);
            if let Some(code) = code {
                match code {
                    32 | 33 | 1224 | 995 => return true, // 常時リトライ
                    5 => {
                        // 条件付き：Phase 0観測により、code 5はcommit時に頻発
                        // PermissionDeniedであれば、大半がAV干渉と推測
                        let is_perm = ioe.kind() == std::io::ErrorKind::PermissionDenied;
                        if is_perm {
                            return true;
                        }
                    }
                    80 | 183 | 145 => return true, // 限定的リトライ対象
                    _ => {}
                }
            }
        }
        src = e.source();
    }

    // エラーメッセージからのフォールバック検出
    if let Some(code) = code_from_msg {
        match code {
            32 | 33 | 1224 | 995 => return true,
            5 if msg.contains("PermissionDenied") || msg.contains("アクセスが拒否") => return true,
            80 | 183 | 145 => return true,
            _ => {}
        }
    }

    false
}

/// Windowsエラーのリトライクラス分類
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WindowsIoRetryClass {
    /// 常時リトライ（最大回数まで）
    Always,
    /// 条件付き（ヒューリスティクス成立時のみ）
    Conditional,
    /// 限定的（指定回数まで、推奨1–2回）
    Limited(u32),
    /// リトライ非推奨
    None,
}

/// Windowsエラーコードに基づくリトライクラス分類
pub fn windows_error_retry_class(err: &tantivy::TantivyError) -> WindowsIoRetryClass {
    if is_writer_killed(err) {
        return WindowsIoRetryClass::None;
    }

    // エラーメッセージから直接os errorコードを抽出（フォールバック）
    let msg = err.to_string();
    let code_from_msg = extract_error_code_from_message(&msg);

    let mut src = std::error::Error::source(err);
    while let Some(e) = src {
        if let Some(ioe) = e.downcast_ref::<std::io::Error>() {
            let code = ioe.raw_os_error().or(code_from_msg);
            if let Some(code) = code {
                match code {
                    32 | 33 | 1224 | 995 => return WindowsIoRetryClass::Always,
                    5 => {
                        // Phase 0観測により、code 5はcommit時に頻発
                        let is_perm = ioe.kind() == std::io::ErrorKind::PermissionDenied;
                        if is_perm {
                            return WindowsIoRetryClass::Conditional;
                        }
                    }
                    80 | 183 | 145 => return WindowsIoRetryClass::Limited(2),
                    _ => {}
                }
            }
        }
        src = e.source();
    }

    // エラーメッセージからのフォールバック検出
    if let Some(code) = code_from_msg {
        match code {
            32 | 33 | 1224 | 995 => return WindowsIoRetryClass::Always,
            5 if msg.contains("PermissionDenied") || msg.contains("アクセスが拒否") => {
                return WindowsIoRetryClass::Conditional
            }
            80 | 183 | 145 => return WindowsIoRetryClass::Limited(2),
            _ => {}
        }
    }

    WindowsIoRetryClass::None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalized_heap_bytes() {
        // Below minimum
        assert_eq!(normalized_heap_bytes(10 * 1024 * 1024), 15 * 1024 * 1024);

        // Within range
        assert_eq!(normalized_heap_bytes(100 * 1024 * 1024), 100 * 1024 * 1024);

        // Above maximum
        assert_eq!(
            normalized_heap_bytes(3 * 1024 * 1024 * 1024),
            2 * 1024 * 1024 * 1024
        );
    }

    #[test]
    fn test_backoff_with_jitter_ms() {
        // attempt=0: 80-120ms
        for _ in 0..10 {
            let delay = backoff_with_jitter_ms(0);
            assert!(delay >= 80 && delay <= 120);
        }

        // attempt=1: 100-150ms
        for _ in 0..10 {
            let delay = backoff_with_jitter_ms(1);
            assert!(delay >= 100 && delay <= 150);
        }

        // attempt=2: 200-250ms
        for _ in 0..10 {
            let delay = backoff_with_jitter_ms(2);
            assert!(delay >= 200 && delay <= 250);
        }

        // attempt=3: 400-450ms
        for _ in 0..10 {
            let delay = backoff_with_jitter_ms(3);
            assert!(delay >= 400 && delay <= 450);
        }

        // attempt>=4: 800-850ms
        for _ in 0..10 {
            let delay = backoff_with_jitter_ms(4);
            assert!(delay >= 800 && delay <= 850);
        }
    }
}
