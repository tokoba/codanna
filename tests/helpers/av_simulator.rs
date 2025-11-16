//! AV Scan Simulator (Phase 0 - Section 11.7.1)
//!
//! This module simulates antivirus software behavior on Windows by:
//! 1. Detecting file creation events with `notify` crate
//! 2. Opening files exclusively (FILE_SHARE_NONE) using CreateFileW
//! 3. Holding the handle for a short duration (simulating AV scan)
//! 4. Releasing the handle
//!
//! This induces ERROR_SHARING_VIOLATION (32) during rename/delete operations,
//! mimicking real-world Windows Defender or other AV software interference.

#[cfg(all(test, target_os = "windows"))]
use std::io;
#[cfg(all(test, target_os = "windows"))]
use std::os::windows::ffi::OsStrExt;
#[cfg(all(test, target_os = "windows"))]
use std::path::Path;
#[cfg(all(test, target_os = "windows"))]
use std::time::Duration;
#[cfg(all(test, target_os = "windows"))]
use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
#[cfg(all(test, target_os = "windows"))]
use windows_sys::Win32::Storage::FileSystem::{
    CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    FILE_SHARE_NONE, OPEN_EXISTING,
};

#[cfg(all(test, target_os = "windows"))]
pub fn hold_exclusive<P: AsRef<Path>>(path: P, hold_ms: u64) -> io::Result<()> {
    let wide: Vec<u16> = path
        .as_ref()
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let handle: HANDLE = unsafe {
        CreateFileW(
            wide.as_ptr(),
            (FILE_GENERIC_READ | FILE_GENERIC_WRITE) as u32,
            FILE_SHARE_NONE,
            std::ptr::null_mut(),
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            std::ptr::null_mut(),
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }

    std::thread::sleep(Duration::from_millis(hold_ms));

    unsafe {
        CloseHandle(handle);
    }

    Ok(())
}

#[cfg(all(test, target_os = "windows"))]
pub fn spawn_av_watcher<P: AsRef<Path> + Send + 'static>(
    watch_dir: P,
    hold_duration_ms: u64,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        use notify::{Event, EventKind, RecursiveMode, Watcher};
        use std::sync::mpsc::channel;

        let (tx, rx) = channel();

        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        })
        .expect("Failed to create file watcher");

        watcher
            .watch(watch_dir.as_ref(), RecursiveMode::Recursive)
            .expect("Failed to watch directory");

        eprintln!(
            "[AV Simulator] Watching {} for file creation events...",
            watch_dir.as_ref().display()
        );

        while let Ok(event) = rx.recv() {
            if matches!(event.kind, EventKind::Create(_)) {
                for path in event.paths {
                    if !path.is_file() {
                        continue;
                    }

                    if let Some(fname) = path.file_name() {
                        let name = fname.to_string_lossy();
                        if name.starts_with(".tmp") || name.ends_with(".lock") {
                            continue;
                        }
                    }

                    let should_lock = if let Some(ext) = path.extension() {
                        let ext_str = ext.to_string_lossy();
                        matches!(ext_str.as_ref(), "store" | "pos" | "term" | "idx" | "fast" | "fieldnorm")
                    } else {
                        false
                    };

                    if should_lock {
                        if let Err(_e) = hold_exclusive(&path, hold_duration_ms) {
                        }
                    }
                }
            }
        }
    })
}

#[cfg(not(all(test, target_os = "windows")))]
pub fn hold_exclusive<P: AsRef<std::path::Path>>(_path: P, _hold_ms: u64) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "av_simulator requires Windows platform",
    ))
}

#[cfg(not(all(test, target_os = "windows")))]
pub fn spawn_av_watcher<P: AsRef<std::path::Path> + Send + 'static>(
    _watch_dir: P,
    _hold_duration_ms: u64,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(|| {
        eprintln!("[AV Simulator] Not available: requires Windows platform");
    })
}
