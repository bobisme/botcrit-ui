//! Shared types for review data and the CritClient trait.

use std::collections::HashMap;

use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Summary of a review for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewSummary {
    pub review_id: String,
    pub title: String,
    pub author: String,
    pub status: String,
    pub thread_count: i64,
    pub open_thread_count: i64,
}

/// Full details of a review.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewDetail {
    pub review_id: String,
    pub jj_change_id: String,
    pub initial_commit: String,
    pub final_commit: Option<String>,
    pub title: String,
    pub description: Option<String>,
    pub author: String,
    pub created_at: String,
    pub status: String,
    pub status_changed_at: Option<String>,
    pub status_changed_by: Option<String>,
    pub abandon_reason: Option<String>,
    pub thread_count: i64,
    pub open_thread_count: i64,
}

/// Summary of a thread for list views.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub thread_id: String,
    pub file_path: String,
    pub selection_start: i64,
    pub selection_end: Option<i64>,
    pub status: String,
    pub comment_count: i64,
}

/// Full details of a thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadDetail {
    pub thread_id: String,
    pub review_id: String,
    pub file_path: String,
    pub selection_type: String,
    pub selection_start: i64,
    pub selection_end: Option<i64>,
    pub commit_hash: String,
    pub author: String,
    pub created_at: String,
    pub status: String,
    pub status_changed_at: Option<String>,
    pub status_changed_by: Option<String>,
    pub resolve_reason: Option<String>,
    pub reopen_reason: Option<String>,
}

/// A single comment in a thread.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Comment {
    pub comment_id: String,
    pub author: String,
    pub body: String,
    pub created_at: String,
}

/// Bundle of review data loaded in one call.
pub struct ReviewData {
    pub detail: ReviewDetail,
    pub threads: Vec<ThreadSummary>,
    pub comments: HashMap<String, Vec<Comment>>,
}

/// Trait for loading review data from any backend.
pub trait CritClient {
    fn list_reviews(&self, status: Option<&str>) -> Result<Vec<ReviewSummary>>;
    fn load_review_data(&self, review_id: &str) -> Result<Option<ReviewData>>;
}
