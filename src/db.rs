//! Direct database access to botcrit's SQLite projection database.
//!
//! These types mirror botcrit's projection::query types but are defined here
//! to avoid a direct dependency on the botcrit crate.

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::path::Path;

/// Summary of a review for list views.
#[derive(Debug, Clone, Serialize)]
pub struct ReviewSummary {
    pub review_id: String,
    pub title: String,
    pub author: String,
    pub status: String,
    pub thread_count: i64,
    pub open_thread_count: i64,
}

/// Full details of a review.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct ThreadSummary {
    pub thread_id: String,
    pub file_path: String,
    pub selection_start: i64,
    pub selection_end: Option<i64>,
    pub status: String,
    pub comment_count: i64,
}

/// Full details of a thread.
#[derive(Debug, Clone, Serialize)]
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
#[derive(Debug, Clone, Serialize)]
pub struct Comment {
    pub comment_id: String,
    pub author: String,
    pub body: String,
    pub created_at: String,
}

/// Database handle for querying botcrit data.
pub struct Db {
    conn: Connection,
}

impl Db {
    /// Open an existing botcrit database.
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("Failed to open database: {}", path.display()))?;
        Ok(Self { conn })
    }

    /// List all reviews, optionally filtered by status.
    pub fn list_reviews(&self, status: Option<&str>) -> Result<Vec<ReviewSummary>> {
        let sql = match status {
            Some(_) => {
                "SELECT review_id, title, author, status, thread_count, open_thread_count
                 FROM v_reviews_summary WHERE status = ? ORDER BY created_at DESC"
            }
            None => {
                "SELECT review_id, title, author, status, thread_count, open_thread_count
                 FROM v_reviews_summary ORDER BY created_at DESC"
            }
        };

        let mut stmt = self.conn.prepare(sql)?;

        let rows = if let Some(s) = status {
            stmt.query_map(params![s], Self::map_review_summary)?
        } else {
            stmt.query_map([], Self::map_review_summary)?
        };

        rows.collect::<Result<Vec<_>, _>>()
            .context("Failed to list reviews")
    }

    fn map_review_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReviewSummary> {
        Ok(ReviewSummary {
            review_id: row.get(0)?,
            title: row.get(1)?,
            author: row.get(2)?,
            status: row.get(3)?,
            thread_count: row.get(4)?,
            open_thread_count: row.get(5)?,
        })
    }

    /// Get full details of a review.
    pub fn get_review(&self, review_id: &str) -> Result<Option<ReviewDetail>> {
        let sql = "SELECT 
            r.review_id, r.jj_change_id, r.initial_commit, r.final_commit,
            r.title, r.description, r.author, r.created_at, r.status,
            r.status_changed_at, r.status_changed_by, r.abandon_reason,
            COALESCE(s.thread_count, 0), COALESCE(s.open_thread_count, 0)
            FROM reviews r
            LEFT JOIN v_reviews_summary s ON s.review_id = r.review_id
            WHERE r.review_id = ?";

        let mut stmt = self.conn.prepare(sql)?;
        let result = stmt
            .query_row(params![review_id], |row| {
                Ok(ReviewDetail {
                    review_id: row.get(0)?,
                    jj_change_id: row.get(1)?,
                    initial_commit: row.get(2)?,
                    final_commit: row.get(3)?,
                    title: row.get(4)?,
                    description: row.get(5)?,
                    author: row.get(6)?,
                    created_at: row.get(7)?,
                    status: row.get(8)?,
                    status_changed_at: row.get(9)?,
                    status_changed_by: row.get(10)?,
                    abandon_reason: row.get(11)?,
                    thread_count: row.get(12)?,
                    open_thread_count: row.get(13)?,
                })
            })
            .optional()?;

        Ok(result)
    }

    /// List threads for a review, optionally filtered by status or file.
    pub fn list_threads(
        &self,
        review_id: &str,
        status: Option<&str>,
        file: Option<&str>,
    ) -> Result<Vec<ThreadSummary>> {
        let mut sql = String::from(
            "SELECT t.thread_id, t.file_path, t.selection_start, t.selection_end, 
                    t.status, COUNT(c.comment_id) as comment_count
             FROM threads t
             LEFT JOIN comments c ON c.thread_id = t.thread_id
             WHERE t.review_id = ?",
        );

        if status.is_some() {
            sql.push_str(" AND t.status = ?");
        }
        if file.is_some() {
            sql.push_str(" AND t.file_path = ?");
        }
        sql.push_str(" GROUP BY t.thread_id ORDER BY t.file_path, t.selection_start");

        let mut stmt = self.conn.prepare(&sql)?;

        let rows: Vec<ThreadSummary> = match (status, file) {
            (Some(s), Some(f)) => stmt
                .query_map(params![review_id, s, f], Self::map_thread_summary)?
                .collect::<Result<Vec<_>, _>>()?,
            (Some(s), None) => stmt
                .query_map(params![review_id, s], Self::map_thread_summary)?
                .collect::<Result<Vec<_>, _>>()?,
            (None, Some(f)) => stmt
                .query_map(params![review_id, f], Self::map_thread_summary)?
                .collect::<Result<Vec<_>, _>>()?,
            (None, None) => stmt
                .query_map(params![review_id], Self::map_thread_summary)?
                .collect::<Result<Vec<_>, _>>()?,
        };

        Ok(rows)
    }

    fn map_thread_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<ThreadSummary> {
        Ok(ThreadSummary {
            thread_id: row.get(0)?,
            file_path: row.get(1)?,
            selection_start: row.get(2)?,
            selection_end: row.get(3)?,
            status: row.get(4)?,
            comment_count: row.get(5)?,
        })
    }

    /// Get full details of a thread.
    pub fn get_thread(&self, thread_id: &str) -> Result<Option<ThreadDetail>> {
        let sql = "SELECT 
            thread_id, review_id, file_path, selection_type,
            selection_start, selection_end, commit_hash, author,
            created_at, status, status_changed_at, status_changed_by,
            resolve_reason, reopen_reason
            FROM threads WHERE thread_id = ?";

        let mut stmt = self.conn.prepare(sql)?;
        let result = stmt
            .query_row(params![thread_id], |row| {
                Ok(ThreadDetail {
                    thread_id: row.get(0)?,
                    review_id: row.get(1)?,
                    file_path: row.get(2)?,
                    selection_type: row.get(3)?,
                    selection_start: row.get(4)?,
                    selection_end: row.get(5)?,
                    commit_hash: row.get(6)?,
                    author: row.get(7)?,
                    created_at: row.get(8)?,
                    status: row.get(9)?,
                    status_changed_at: row.get(10)?,
                    status_changed_by: row.get(11)?,
                    resolve_reason: row.get(12)?,
                    reopen_reason: row.get(13)?,
                })
            })
            .optional()?;

        Ok(result)
    }

    /// List comments for a thread.
    pub fn list_comments(&self, thread_id: &str) -> Result<Vec<Comment>> {
        let sql = "SELECT comment_id, author, body, created_at
                   FROM comments WHERE thread_id = ? ORDER BY created_at";

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt
            .query_map(params![thread_id], |row| {
                Ok(Comment {
                    comment_id: row.get(0)?,
                    author: row.get(1)?,
                    body: row.get(2)?,
                    created_at: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }
}

// Make optional() available on query_row results
trait OptionalExt<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExt<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
