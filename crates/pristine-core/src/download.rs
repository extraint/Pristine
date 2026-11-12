use crate::error::{Error, Result};
use futures::{stream, StreamExt};
use serde::de::DeserializeOwned;
use sha1::{Digest, Sha1};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone)]
pub struct Progress {
    pub stage: &'static str,
    pub done: usize,
    pub total: usize,
}

pub type ProgressFn = Arc<dyn Fn(Progress) + Send + Sync>;

#[derive(Debug, Clone)]
pub struct Job {
    pub url: String,
    pub dest: PathBuf,
    pub sha1: Option<String>,
}

pub async fn fetch_json<T: DeserializeOwned>(client: &reqwest::Client, url: &str) -> Result<T> {
    Ok(client.get(url).send().await?.error_for_status()?.json().await?)
}

fn sha1_file(path: &Path) -> Result<String> {
    use std::io::Read;
    let mut f = std::fs::File::open(path)?;
    let mut h = Sha1::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 { break; }
        h.update(&buf[..n]);
    }
    Ok(hex::encode(h.finalize()))
}

/// Already on disk and (if a hash is known) matching it?
fn is_valid(job: &Job) -> bool {
    if !job.dest.is_file() { return false; }
    match &job.sha1 {
        Some(want) => sha1_file(&job.dest).map(|got| got.eq_ignore_ascii_case(want)).unwrap_or(false),
        None => true,
    }
}

async fn download_once(client: &reqwest::Client, job: &Job) -> Result<()> {
    if let Some(parent) = job.dest.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let tmp = job.dest.with_extension("part");
    let mut resp = client.get(&job.url).send().await?.error_for_status()?;
    let mut file = tokio::fs::File::create(&tmp).await?;
    let mut hasher = Sha1::new();
    while let Some(chunk) = resp.chunk().await? {
        hasher.update(&chunk);
        file.write_all(&chunk).await?;
    }
    file.flush().await?;
    drop(file);
    if let Some(want) = &job.sha1 {
        let got = hex::encode(hasher.finalize());
        if !got.eq_ignore_ascii_case(want) {
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(Error::HashMismatch { path: job.dest.clone(), expected: want.clone(), actual: got });
        }
    }
    tokio::fs::rename(&tmp, &job.dest).await?;
    Ok(())
}

pub async fn download(client: &reqwest::Client, job: &Job) -> Result<()> {
    if is_valid(job) { return Ok(()); }
    let mut last = None;
    for attempt in 0..3u64 {
        match download_once(client, job).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                tracing::warn!("download {} failed (attempt {}): {e}", job.url, attempt + 1);
                last = Some(e);
                tokio::time::sleep(std::time::Duration::from_millis(400 * (attempt + 1))).await;
            }
        }
    }
    Err(last.unwrap())
}

/// Download many files concurrently, reporting progress.
pub async fn download_all(
    client: &reqwest::Client,
    jobs: Vec<Job>,
    stage: &'static str,
    progress: &ProgressFn,
) -> Result<()> {
    let total = jobs.len();
    let done = Arc::new(AtomicUsize::new(0));
    progress(Progress { stage, done: 0, total });
    let mut results = stream::iter(jobs)
        .map(|job| {
            let client = client.clone();
            let done = done.clone();
            let progress = progress.clone();
            async move {
                download(&client, &job).await?;
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                progress(Progress { stage, done: n, total });
                Ok::<(), Error>(())
            }
        })
        .buffer_unordered(16);
    while let Some(r) = results.next().await {
        r?;
    }
    Ok(())
}
