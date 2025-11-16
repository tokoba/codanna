//! Phase 0: Heap Size Regression Test (Improved)
//!
//! 変更点:
//! - commit前に必ず実データを追加（空commitを禁止）
//! - heap_sizeに応じてドキュメントサイズをスケール
//! - Windowsのみ、AVシミュレータを親ディレクトリに1つだけ起動（各heapで使い回し）
//!
//! 実行例:
//!   set CODANNA_DEBUG=1
//!   cargo test --test heap_size_regression -- --ignored --nocapture
//!
//! 期待:
//! - 各heap_sizeでセグメント生成・rename/deleteが多数発生
//! - WindowsでAVシミュレーションON時、ERROR_SHARING_VIOLATION(32)等の発生確率が上昇
//! - heap_sizeが大きいほどセグメントが大きくなり、AVスキャン時間が伸び、エラー発生率も上がる傾向

#![cfg(test)]

use std::path::PathBuf;
use std::time::Duration;
use tempfile::TempDir;

use tantivy::{
    doc,
    schema::{Schema, SchemaBuilder, STORED, TEXT},
    Index, IndexWriter,
};

#[cfg(all(test, target_os = "windows"))]
#[path = "helpers/mod.rs"]
mod helpers;

#[test]
#[ignore]
fn phase0_heap_size_observation_real_io() {
    unsafe {
        std::env::set_var("CODANNA_DEBUG", "1");
    }

    let heap_sizes_mb = [10, 15, 50, 100, 150, 200];
    let runs_per_heap = 20;
    let commits_per_run = 40;

    let temp_root = TempDir::new().expect("temp root");

    #[cfg(all(test, target_os = "windows"))]
    let _av_join = {
        let hold_ms = std::env::var("CODANNA_AV_HOLD_MS")
            .ok()
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(80);
        let watch_dir = temp_root.path().to_path_buf();
        Some(helpers::av_simulator::spawn_av_watcher(watch_dir, hold_ms))
    };

    eprintln!(
        "\n========== Phase 0 (Improved) ==========\nheap_sizes: {:?} MB\nruns_per_heap: {}\ncommits_per_run: {}\n=========================================\n",
        heap_sizes_mb, runs_per_heap, commits_per_run
    );

    for &heap_mb in &heap_sizes_mb {
        eprintln!("\n>>> heap_size = {} MB <<<", heap_mb);

        let heap_dir = temp_root
            .path()
            .join(format!("heap_{:03}mb", heap_mb));
        std::fs::create_dir_all(&heap_dir).expect("make heap dir");

        for run in 0..runs_per_heap {
            let run_dir = heap_dir.join(format!("run_{:03}", run));
            std::fs::create_dir_all(&run_dir).expect("make run dir");

            let stats = run_single_observation_real_io(&run_dir, heap_mb, commits_per_run);
            match stats {
                Ok(s) => eprintln!(
                    "(Phase0) run={}/{} heap={}MB commits={} commit_errors={} add_doc_errors={}",
                    run + 1,
                    runs_per_heap,
                    heap_mb,
                    s.commits,
                    s.commit_errors,
                    s.add_doc_errors,
                ),
                Err(e) => eprintln!(
                    "(Phase0) run={}/{} heap={}MB status=failed error={}",
                    run + 1,
                    runs_per_heap,
                    heap_mb,
                    e
                ),
            }

            std::thread::sleep(Duration::from_millis(100));
        }
    }

    eprintln!("\n========== Phase 0 (Improved) Completed ==========");
}

struct RunStatistics {
    commits: usize,
    commit_errors: usize,
    add_doc_errors: usize,
}

fn run_single_observation_real_io(
    index_path: &PathBuf,
    heap_mb: usize,
    commits_per_run: usize,
) -> Result<RunStatistics, Box<dyn std::error::Error>> {
    let mut sb = SchemaBuilder::default();
    let f_path = sb.add_text_field("path", STORED);
    let f_body = sb.add_text_field("body", TEXT | STORED);
    let schema: Schema = sb.build();

    let index = Index::create_in_dir(index_path, schema.clone())?;

    let heap_bytes = heap_mb * 1_000_000;
    let mut writer: IndexWriter = index.writer(heap_bytes)?;

    let scale = (heap_mb as f64 / 15.0).max(1.0);
    let body_size_bytes = (4096.0 * scale) as usize;
    let body_blob = "x".repeat(body_size_bytes);

    let docs_per_commit = 200;

    let mut stats = RunStatistics {
        commits: 0,
        commit_errors: 0,
        add_doc_errors: 0,
    };

    for c in 0..commits_per_run {
        for i in 0..docs_per_commit {
            let doc = doc!(
                f_path => format!("heap{:03}/seg{:03}/doc{:06}.txt", heap_mb, c, i),
                f_body => body_blob.as_str(),
            );
            if let Err(e) = writer.add_document(doc) {
                stats.add_doc_errors += 1;
                eprintln!("[add_document error] heap={}MB: {}", heap_mb, e);
            }
        }

        if c % 4 == 0 {
            std::thread::sleep(Duration::from_millis(5));
        }

        match writer.commit() {
            Ok(_) => {
                stats.commits += 1;
            }
            Err(e) => {
                stats.commit_errors += 1;
                eprintln!(
                    "[commit error] heap={}MB c={}: {}",
                    heap_mb,
                    c,
                    format_tantivy_error_basic(&e)
                );
            }
        }

        if c % 8 == 0 {
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    Ok(stats)
}

fn format_tantivy_error_basic(err: &tantivy::TantivyError) -> String {
    use std::error::Error;
    let mut out = format!("TantivyError: {}", err);
    let mut src = err.source();
    let mut depth = 0usize;
    while let Some(e) = src {
        out.push_str(&format!("\n  cause[{}]: {}", depth, e));
        if let Some(ioe) = e.downcast_ref::<std::io::Error>() {
            out.push_str(&format!("\n    io::ErrorKind: {:?}", ioe.kind()));
            if let Some(code) = ioe.raw_os_error() {
                out.push_str(&format!("\n    raw_os_error: {}", code));
            }
        }
        depth += 1;
        src = e.source();
    }
    out
}
