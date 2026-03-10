//! SQLite-backed persistence layer.

use common::error::AppError;
use sha2::{Digest, Sha256};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use uuid::Uuid;

/// Data access facade for authentication/session/contact storage.
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Opens the SQLite database and initializes schema if missing.
    pub async fn connect(url: &str) -> Result<Self, AppError> {
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect(url)
            .await?;
        let db = Self { pool };
        db.migrate().await?;
        Ok(db)
    }

    async fn migrate(&self) -> Result<(), AppError> {
        sqlx::query("CREATE TABLE IF NOT EXISTS users (username TEXT PRIMARY KEY, password_hash TEXT NOT NULL)")
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "CREATE TABLE IF NOT EXISTS sessions (token TEXT PRIMARY KEY, username TEXT NOT NULL)",
        )
        .execute(&self.pool)
        .await?;
        sqlx::query("CREATE TABLE IF NOT EXISTS contacts (owner TEXT NOT NULL, contact TEXT NOT NULL, PRIMARY KEY(owner, contact))")
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn register_user(&self, username: &str, password: &str) -> Result<(), AppError> {
        let hash = hash_password(password);
        sqlx::query("INSERT INTO users (username, password_hash) VALUES (?, ?)")
            .bind(username)
            .bind(hash)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn authenticate(&self, username: &str, password: &str) -> Result<bool, AppError> {
        let hash = hash_password(password);
        let row: Option<(String,)> =
            sqlx::query_as("SELECT password_hash FROM users WHERE username = ?")
                .bind(username)
                .fetch_optional(&self.pool)
                .await?;
        Ok(matches!(row, Some((stored,)) if stored == hash))
    }

    pub async fn create_session(&self, username: &str) -> Result<Uuid, AppError> {
        let token = Uuid::new_v4();
        sqlx::query("INSERT INTO sessions (token, username) VALUES (?, ?)")
            .bind(token.to_string())
            .bind(username)
            .execute(&self.pool)
            .await?;
        Ok(token)
    }

    pub async fn session_owner(&self, token: &str) -> Result<Option<String>, AppError> {
        let row: Option<(String,)> =
            sqlx::query_as("SELECT username FROM sessions WHERE token = ?")
                .bind(token)
                .fetch_optional(&self.pool)
                .await?;
        Ok(row.map(|(u,)| u))
    }
}

fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}
