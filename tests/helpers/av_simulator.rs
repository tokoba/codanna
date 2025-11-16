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
//!
//! Usage:
//! - This module is only compiled on Windows
//! - Tests using this should be marked with #[ignore] for manual execution only
//! - Example:
//!   ```rust
//!   #[cfg(all(test, target_os = "windows"))]
//!   use crate::helpers::av_simulator;
//!   ```

// Windows-specific AV simulator
#[cfg(all(test, target_os = "windows"))]
pub mod av_simulator {
    use std::ffi::OsStr;
    use std::io;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use std::time::Duration;
    use windows_sys::Win32::Foundation::{CloseHandle, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{
        CreateFileW, FILE_FLAG_BACKUP_SEMANTICS, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
        FILE_SHARE_NONE, OPEN_EXISTING,
    };

    /// Hold a file or directory exclusively (no sharing) for a specified duration
    ///
    /// This simulates antivirus software locking a file during scanning.
    /// The file is opened with FILE_SHARE_NONE, preventing other processes
    /// from accessing it (read, write, or delete) while held.
    ///
    /// # Arguments
    /// * `path` - File or directory path to lock
    /// * `hold_ms` - Duration to hold the exclusive lock (in milliseconds)
    ///
    /// # Returns
    /// * `Ok(())` - Successfully locked and released
    /// * `Err(io::Error)` - Failed to open the file (e.g., file not found)
    ///
    /// # Example
    /// ```rust,ignore
    /// # use std::path::Path;
    /// # use av_simulator::hold_exclusive;
    /// // Simulate AV scanning a file for 80ms
    /// hold_exclusive(Path::new("test_file.txt"), 80)?;
    /// ```
    pub fn hold_exclusive<P: AsRef<Path>>(path: P, hold_ms: u64) -> io::Result<()> {
        // Convert path to wide string (UTF-16) for Windows API
        let wide: Vec<u16> = path
            .as_ref()
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0)) // Null terminator
            .collect();

        // Open file/directory with exclusive access (no sharing)
        let handle: HANDLE = unsafe {
            CreateFileW(
                wide.as_ptr(),
                (FILE_GENERIC_READ | FILE_GENERIC_WRITE) as u32,
                FILE_SHARE_NONE, // Critical: No sharing allowed
                std::ptr::null_mut(),
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS, // Required to open directories
                0,
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }

        // Hold the lock (simulating AV scan duration)
        std::thread::sleep(Duration::from_millis(hold_ms));

        // Release the handle
        unsafe {
            CloseHandle(handle);
        }

        Ok(())
    }

    /// Spawn a background thread that monitors file creation events and simulates AV scans
    ///
    /// This is a more realistic simulation where the "AV" responds to file creation
    /// events and locks them briefly, similar to real antivirus behavior.
    ///
    /// # Arguments
    /// * `watch_dir` - Directory to monitor for file creation events
    /// * `hold_duration_ms` - How long to hold each file exclusively (milliseconds)
    ///
    /// # Returns
    /// * `JoinHandle` - Handle to the background AV simulation thread
    ///
    /// # Example
    /// ```rust,ignore
    /// # use std::path::Path;
    /// # use av_simulator::spawn_av_watcher;
    /// let av_thread = spawn_av_watcher(Path::new("./test_index"), 80);
    /// // ... perform operations that create files ...
    /// av_thread.join().unwrap();
    /// ```
    pub fn spawn_av_watcher<P: AsRef<Path> + Send + 'static>(
        watch_dir: P,
        hold_duration_ms: u64,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(move || {
            use notify::{Event, EventKind, RecursiveMode, Watcher};
            use std::sync::mpsc::channel;

            let (tx, rx) = channel();

            // Create a watcher
            let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    let _ = tx.send(event);
                }
            })
            .expect("Failed to create file watcher");

            // Start watching the directory
            watcher
                .watch(watch_dir.as_ref(), RecursiveMode::Recursive)
                .expect("Failed to watch directory");

            eprintln!(
                "[AV Simulator] Watching {} for file creation events...",
                watch_dir.as_ref().display()
            );

            // Process file events
            while let Ok(event) = rx.recv() {
                if matches!(event.kind, EventKind::Create(_)) {
                    for path in event.paths {
                        if path.is_file() {
                            eprintln!("[AV Simulator] Detected new file: {}, locking...", path.display());
                            if let Err(e) = hold_exclusive(&path, hold_duration_ms) {
                                eprintln!("[AV Simulator] Failed to lock {}: {}", path.display(), e);
                            } else {
                                eprintln!(
                                    "[AV Simulator] Released lock on {} after {}ms",
                                    path.display(),
                                    hold_duration_ms
                                );
                            }
                        }
                    }
                }
            }
        })
    }
}

// Stub implementation for non-Windows platforms
#[cfg(not(all(test, target_os = "windows")))]
pub mod av_simulator {
    use std::io;
    use std::path::Path;

    /// Stub: AV simulator requires Windows platform
    pub fn hold_exclusive<P: AsRef<Path>>(_path: P, _hold_ms: u64) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "av_simulator requires Windows platform",
        ))
    }

    /// Stub: AV watcher requires Windows platform
    pub fn spawn_av_watcher<P: AsRef<Path> + Send + 'static>(
        _watch_dir: P,
        _hold_duration_ms: u64,
    ) -> std::thread::JoinHandle<()> {
        std::thread::spawn(|| {
            eprintln!("[AV Simulator] Not available: requires Windows platform");
        })
    }
}
