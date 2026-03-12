//! SQLite-backed persistence layer.

use common::error::AppError;
use sha2::{Digest, Sha256};
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use std::{fs, path::PathBuf, str::FromStr};
use uuid::Uuid;

/// Data access facade for authentication/session/contact storage.
#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    /// Opens the SQLite database and initializes schema if missing.
    ///
    /// If the backing file is missing, unreadable, or corrupt, this function attempts
    /// to recreate it once for file-backed SQLite URLs.
    pub async fn connect(url: &str) -> Result<Self, AppError> {
        if let Some(path) = sqlite_file_path(url) {
            ensure_parent_dir(&path)?;
            if path.exists() && !is_valid_sqlite_file(&path)? {
                reset_sqlite_file(&path)?;
            }
        }

        match Self::connect_and_migrate(url).await {
            Ok(db) => Ok(db),
            Err(first_err) => {
                if let Some(path) = sqlite_file_path(url) {
                    reset_sqlite_file(&path)?;
                    Self::connect_and_migrate(url).await.map_err(|_| first_err)
                } else {
                    Err(first_err)
                }
            }
        }
    }

    async fn connect_and_migrate(url: &str) -> Result<Self, AppError> {
        let options = SqliteConnectOptions::from_str(url)?.create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
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

    pub async fn upsert_contact(&self, owner: &str, contact: &str) -> Result<(), AppError> {
        sqlx::query("INSERT OR IGNORE INTO contacts (owner, contact) VALUES (?, ?)")
            .bind(owner)
            .bind(contact)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn list_contacts(&self, owner: &str) -> Result<Vec<String>, AppError> {
        let rows: Vec<(String,)> =
            sqlx::query_as("SELECT contact FROM contacts WHERE owner = ? ORDER BY contact")
                .bind(owner)
                .fetch_all(&self.pool)
                .await?;
        Ok(rows.into_iter().map(|(contact,)| contact).collect())
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

fn sqlite_file_path(url: &str) -> Option<PathBuf> {
    let raw = if let Some(raw) = url.strip_prefix("sqlite://") {
        raw
    } else if let Some(raw) = url.strip_prefix("sqlite:") {
        raw
    } else {
        return None;
    };

    if raw == ":memory:" {
        return None;
    }

    let path_part = raw.split('?').next().unwrap_or(raw);
    if path_part.is_empty() {
        None
    } else {
        Some(PathBuf::from(path_part))
    }
}

fn ensure_parent_dir(path: &PathBuf) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    Ok(())
}

fn is_valid_sqlite_file(path: &PathBuf) -> Result<bool, AppError> {
    let bytes = fs::read(path)?;
    if bytes.is_empty() {
        return Ok(false);
    }

    const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";
    if bytes.len() < SQLITE_MAGIC.len() {
        return Ok(false);
    }

    Ok(&bytes[..SQLITE_MAGIC.len()] == SQLITE_MAGIC)
}

fn reset_sqlite_file(path: &PathBuf) -> Result<(), AppError> {
    ensure_parent_dir(path)?;

    if path.exists() {
        fs::remove_file(path)?;
    }

    Ok(())
}

fn hash_password(password: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(password.as_bytes());
    hex::encode(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::Database;

    #[tokio::test]
    async fn auth_and_contacts_roundtrip() {
        let db = Database::connect("sqlite::memory:").await.expect("db");
        db.register_user("alice", "secret")
            .await
            .expect("register alice");
        db.register_user("bob", "secret")
            .await
            .expect("register bob");

        let ok = db.authenticate("alice", "secret").await.expect("auth");
        assert!(ok);

        let token = db.create_session("alice").await.expect("session");
        let owner = db
            .session_owner(&token.to_string())
            .await
            .expect("owner query")
            .expect("owner present");
        assert_eq!(owner, "alice");

        db.upsert_contact("alice", "bob")
            .await
            .expect("contact insert");
        let contacts = db.list_contacts("alice").await.expect("contact list");
        assert_eq!(contacts, vec!["bob".to_string()]);
    }

    #[tokio::test]
    async fn recreates_invalid_file_backed_database() {
        let tmp =
            std::env::temp_dir().join(format!("skype-rs-invalid-{}.db", uuid::Uuid::new_v4()));
        std::fs::write(&tmp, b"this is not sqlite").expect("seed invalid file");

        let url = format!("sqlite://{}", tmp.display());
        let db = Database::connect(&url)
            .await
            .expect("connect should recover");
        db.register_user("carol", "secret")
            .await
            .expect("db usable after recovery");

        let _ = std::fs::remove_file(&tmp);
    }

    #[tokio::test]
    async fn creates_missing_parent_directory_for_database_file() {
        let base = std::env::temp_dir().join(format!("skype-rs-db-dir-{}", uuid::Uuid::new_v4()));
        let db_path = base.join("nested").join("node.db");
        let url = format!("sqlite:{}", db_path.display());

        let db = Database::connect(&url)
            .await
            .expect("connect should create parent directories and db file");
        db.register_user("dave", "secret")
            .await
            .expect("db usable after create");

        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_dir_all(&base);
    }
}
