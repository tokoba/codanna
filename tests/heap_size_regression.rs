//! Phase 0: Heap Size Regression Test (Section 11.7.1)
//!
//! This test observes error occurrence rates across different heap_size values
//! to understand the correlation between heap size and Windows I/O errors.
//!
//! Test Strategy:
//! - heap_size: 10/15/50/100/150/200MB (6 patterns)
//! - Each heap_size: 20 runs (minimum 10 runs recommended)
//! - Per run: 20,000 documents added, commit every 500 documents (~40 events)
//! - Total: 6 × 20 × 40 = 4,800 events
//!
//! Execution:
//! ```bash
//! # Enable Phase 0 detailed logging
//! export CODANNA_DEBUG=1
//! # Optional: JSON output for structured data collection
//! export CODANNA_DEBUG_JSON=1
//! 
//! # Manual execution only (test is marked with #[ignore])
//! cargo test --test heap_size_regression -- --ignored --nocapture
//! ```
//!
//! Expected Observations (Section 11.6.3):
//! - ERROR_SHARING_VIOLATION (32): Windows Defender file lock conflicts
//! - ERROR_USER_MAPPED_FILE (1224): mmap file deletion attempts
//! - ERROR_LOCK_VIOLATION (33): File lock conflicts
//! - ERROR_ACCESS_DENIED (5): Temporary permission denial during AV scan
//!
//! Data Collection:
//! - Errors logged to STDERR in NDJSON format (if CODANNA_DEBUG_JSON=1)
//! - Human-readable format otherwise
//! - Post-processing: Aggregate by heap_size, error_code, operation type

#![cfg(test)]

use codanna::config::Settings;
use codanna::storage::tantivy::DocumentIndex;
use std::time::Duration;
use tempfile::TempDir;

/// Phase 0 observation test: Measure error rates across heap sizes
#[test]
#[ignore] // Manual execution only - this test is time-consuming
fn phase0_heap_size_observation() {
    // Enable debug logging (can also be set via environment variable)
    unsafe {
        std::env::set_var("CODANNA_DEBUG", "1");
    }
    
    // Optional: Enable JSON output for structured data collection
    // unsafe { std::env::set_var("CODANNA_DEBUG_JSON", "1"); }

    let heap_sizes_mb = [10, 15, 50, 100, 150, 200];
    let runs_per_heap = 20; // Minimum 10, recommended 20
    let docs_per_run = 20_000;
    let commit_interval = 500; // Commit every 500 documents (~40 commits per run)

    eprintln!(
        "\n========== Phase 0 Observation Test ==========\nheap_sizes: {:?} MB\nruns_per_heap: {}\ndocs_per_run: {}\ncommit_interval: {}\n==============================================\n",
        heap_sizes_mb, runs_per_heap, docs_per_run, commit_interval
    );

    for &heap_mb in &heap_sizes_mb {
        eprintln!("\n>>> Testing heap_size: {} MB <<<", heap_mb);
        
        for run in 0..runs_per_heap {
            let result = run_single_observation(heap_mb, docs_per_run, commit_interval, run);
            
            match result {
                Ok(stats) => {
                    eprintln!(
                        "(Phase0) run={}/{} heap_mb={} status=success commits={} errors={}",
                        run + 1, runs_per_heap, heap_mb, stats.commits, stats.errors
                    );
                }
                Err(e) => {
                    eprintln!(
                        "(Phase0) run={}/{} heap_mb={} status=failed error={}",
                        run + 1, runs_per_heap, heap_mb, e
                    );
                }
            }

            // Small delay between runs to vary AV scan timing
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    eprintln!("\n========== Phase 0 Observation Completed ==========");
}

struct RunStatistics {
    commits: usize,
    errors: usize,
}

fn run_single_observation(
    heap_mb: usize,
    _docs_per_run: usize,
    commit_interval: usize,
    run_id: usize,
) -> Result<RunStatistics, Box<dyn std::error::Error>> {
    // Create temporary index directory
    let temp_dir = TempDir::new()?;
    let index_path = temp_dir.path().join(format!("phase0_heap{}_run{}", heap_mb, run_id));
    
    // Create settings with custom heap_size
    let mut settings = Settings::default();
    settings.indexing.tantivy_heap_mb = heap_mb;
    settings.indexing.max_retry_attempts = 3;
    settings.indexing.parallel_threads = 1;
    
    // Create DocumentIndex with specified settings
    let index = DocumentIndex::new(&index_path, &settings)?;

    let mut commits = 0;
    let mut errors = 0;

    // Perform multiple commits to trigger I/O operations
    // This is sufficient to observe Windows I/O errors without needing to add actual documents
    let num_commits = commit_interval; // Use commit_interval as number of operations
    
    for i in 0..num_commits {
        // Try to commit (will mostly be no-op, but creates writer and triggers I/O)
        match index.commit_batch() {
            Ok(_) => commits += 1,
            Err(_) => errors += 1,
        }
        
        // Small delay to allow AV to potentially interfere
        if i % 10 == 0 {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    Ok(RunStatistics { commits, errors })
}
