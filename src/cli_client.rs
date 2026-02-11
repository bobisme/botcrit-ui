//! `CritClient` implementation that shells out to the `crit` CLI with `--format json`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use crate::db::{Comment, CritClient, ReviewData, ReviewDetail, ReviewSummary, ThreadSummary};

/// Client that invokes the `crit` binary as a subprocess.
pub struct CliClient {
    repo_path: PathBuf,
}

impl CliClient {
    pub fn new(repo_path: impl Into<PathBuf>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    /// Run `crit <args> --format json --path <repo>` and return stdout bytes.
    fn run_crit(&self, args: &[&str]) -> Result<Vec<u8>> {
        let output = Command::new("crit")
            .args(args)
            .arg("--format")
            .arg("json")
            .arg("--path")
            .arg(&self.repo_path)
            .output()
            .context("Failed to run `crit` — is it installed and on PATH?")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!(
                "crit {} failed (exit {}): {}",
                args.join(" "),
                output.status,
                stderr.trim()
            );
        }

        Ok(output.stdout)
    }
}

// -- Intermediate serde types for the combined `crit review <id>` endpoint --

#[derive(Deserialize)]
struct CombinedResponse {
    review: CombinedReview,
    threads: Vec<CombinedThread>,
}

/// Review detail from the combined endpoint.
/// Has extra fields (`reviewers`, `votes`) that we ignore.
#[derive(Deserialize)]
struct CombinedReview {
    review_id: String,
    jj_change_id: String,
    initial_commit: String,
    final_commit: Option<String>,
    title: String,
    description: Option<String>,
    author: String,
    created_at: String,
    status: String,
    status_changed_at: Option<String>,
    status_changed_by: Option<String>,
    abandon_reason: Option<String>,
    thread_count: i64,
    open_thread_count: i64,
}

/// Thread from the combined endpoint — carries inline `comments` vec.
#[derive(Deserialize)]
struct CombinedThread {
    thread_id: String,
    file_path: String,
    selection_start: i64,
    selection_end: Option<i64>,
    status: String,
    comments: Vec<CombinedComment>,
}

#[derive(Deserialize)]
struct CombinedComment {
    comment_id: String,
    author: String,
    body: String,
    created_at: String,
}

// -- Conversions --

impl From<CombinedReview> for ReviewDetail {
    fn from(r: CombinedReview) -> Self {
        Self {
            review_id: r.review_id,
            jj_change_id: r.jj_change_id,
            initial_commit: r.initial_commit,
            final_commit: r.final_commit,
            title: r.title,
            description: r.description,
            author: r.author,
            created_at: r.created_at,
            status: r.status,
            status_changed_at: r.status_changed_at,
            status_changed_by: r.status_changed_by,
            abandon_reason: r.abandon_reason,
            thread_count: r.thread_count,
            open_thread_count: r.open_thread_count,
        }
    }
}

impl CritClient for CliClient {
    fn list_reviews(&self, status: Option<&str>) -> Result<Vec<ReviewSummary>> {
        let stdout = self.run_crit(&["reviews", "list"])?;
        let reviews: Vec<ReviewSummary> =
            serde_json::from_slice(&stdout).context("Failed to parse `crit reviews list` JSON")?;

        match status {
            Some(s) => Ok(reviews.into_iter().filter(|r| r.status == s).collect()),
            None => Ok(reviews),
        }
    }

    fn load_review_data(&self, review_id: &str) -> Result<Option<ReviewData>> {
        let stdout = self.run_crit(&["review", review_id])?;
        let resp: CombinedResponse =
            serde_json::from_slice(&stdout).context("Failed to parse `crit review` JSON")?;

        let detail: ReviewDetail = resp.review.into();

        let mut threads = Vec::with_capacity(resp.threads.len());
        let mut comments: HashMap<String, Vec<Comment>> = HashMap::new();

        for t in resp.threads {
            let comment_count = t.comments.len() as i64;
            if !t.comments.is_empty() {
                comments.insert(
                    t.thread_id.clone(),
                    t.comments
                        .into_iter()
                        .map(|c| Comment {
                            comment_id: c.comment_id,
                            author: c.author,
                            body: c.body,
                            created_at: c.created_at,
                        })
                        .collect(),
                );
            }
            threads.push(ThreadSummary {
                thread_id: t.thread_id,
                file_path: t.file_path,
                selection_start: t.selection_start,
                selection_end: t.selection_end,
                status: t.status,
                comment_count,
            });
        }

        Ok(Some(ReviewData {
            detail,
            threads,
            comments,
        }))
    }
}
