//! Drops on disk, one folder each:
//!
//! ```text
//! <DATA_DIR>/drops/<id>/meta.json
//! <DATA_DIR>/drops/<id>/content
//! <DATA_DIR>/tmp/            ← drops being written, renamed into drops/ when complete
//! ```
//!
//! Ids sort by creation time. Text drops are small, so all of them are also
//! kept in memory; the disk copy is what survives restarts.

use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicU32, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tokio::{fs, sync::broadcast};

use crate::config::{Config, Expiry};

pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Text,
    Link,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Meta {
    pub kind: Kind,
    pub created_at: u64,
    /// The rule chosen when sending, e.g. `24h` or `never`.
    pub expiry: String,
    pub expires_at: Option<u64>,
    pub source: String,
}

#[derive(Clone)]
pub struct Drop {
    pub id: String,
    pub meta: Meta,
    pub text: String,
}

impl Drop {
    pub fn is_expired(&self, now: u64) -> bool {
        self.meta.expires_at.is_some_and(|at| at <= now)
    }
}

pub struct Store {
    drops_dir: PathBuf,
    tmp_dir: PathBuf,
    max_drops: usize,
    /// Oldest first.
    drops: Mutex<Vec<Drop>>,
    changed: broadcast::Sender<()>,
    counter: AtomicU32,
}

impl Store {
    pub async fn open(config: &Config) -> io::Result<Self> {
        let drops_dir = config.data_dir.join("drops");
        let tmp_dir = config.data_dir.join("tmp");
        fs::create_dir_all(&drops_dir).await?;
        // Leftovers in tmp/ are interrupted writes.
        if fs::try_exists(&tmp_dir).await? {
            fs::remove_dir_all(&tmp_dir).await?;
        }
        fs::create_dir_all(&tmp_dir).await?;

        let mut drops = Vec::new();
        let mut entries = fs::read_dir(&drops_dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let id = entry.file_name().to_string_lossy().into_owned();
            match load(&entry.path(), id.clone()).await {
                Ok(drop) => drops.push(drop),
                Err(error) => eprintln!("skipping unreadable drop {id}: {error}"),
            }
        }
        drops.sort_by(|a, b| a.id.cmp(&b.id));

        Ok(Self {
            drops_dir,
            tmp_dir,
            max_drops: config.max_drops,
            drops: Mutex::new(drops),
            changed: broadcast::channel(16).0,
            counter: AtomicU32::new(0),
        })
    }

    pub async fn add_text(&self, text: String, expiry: Expiry, source: &str) -> io::Result<()> {
        let now = unix_now();
        let trimmed = text.trim();
        let is_link = (trimmed.starts_with("https://") || trimmed.starts_with("http://"))
            && !trimmed.contains(char::is_whitespace);
        let meta = Meta {
            kind: if is_link { Kind::Link } else { Kind::Text },
            created_at: now,
            expiry: expiry.to_string(),
            expires_at: expiry.seconds().map(|secs| now + secs),
            source: source.to_owned(),
        };

        let id = self.new_id();
        let staging = self.tmp_dir.join(&id);
        fs::create_dir(&staging).await?;
        fs::write(staging.join("content"), &text).await?;
        fs::write(staging.join("meta.json"), serde_json::to_vec_pretty(&meta)?).await?;
        fs::rename(&staging, self.drops_dir.join(&id)).await?;

        self.lock().push(Drop { id, meta, text });
        self.cleanup().await;
        self.notify();
        Ok(())
    }

    /// Current drops, newest first, without expired ones.
    pub fn list(&self) -> Vec<Drop> {
        let now = unix_now();
        self.lock()
            .iter()
            .rev()
            .filter(|drop| !drop.is_expired(now))
            .cloned()
            .collect()
    }

    /// Returns false if no drop has this id.
    pub async fn remove(&self, id: &str) -> io::Result<bool> {
        let found = self.lock().iter().any(|drop| drop.id == id);
        if found {
            self.delete(&[id.to_owned()]).await?;
            self.notify();
        }
        Ok(found)
    }

    pub async fn clear(&self) -> io::Result<()> {
        let ids: Vec<String> = self.lock().iter().map(|drop| drop.id.clone()).collect();
        self.delete(&ids).await?;
        self.notify();
        Ok(())
    }

    /// Deletes expired drops and the oldest ones above the limit, keeping
    /// drops marked "until deleted". Errors are logged; the next run retries.
    pub async fn cleanup(&self) -> bool {
        let now = unix_now();
        let doomed: Vec<String> = {
            let drops = self.lock();
            let expired = drops.iter().filter(|drop| drop.is_expired(now));
            let live: Vec<&Drop> = drops.iter().filter(|drop| !drop.is_expired(now)).collect();
            let excess = live.len().saturating_sub(self.max_drops);
            let overflow = live
                .into_iter()
                .filter(|drop| drop.meta.expires_at.is_some())
                .take(excess);
            expired.chain(overflow).map(|drop| drop.id.clone()).collect()
        };
        if doomed.is_empty() {
            return false;
        }
        if let Err(error) = self.delete(&doomed).await {
            eprintln!("cleanup failed: {error}");
        }
        true
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }

    /// Wakes every open live stream.
    pub fn notify(&self) {
        // No receivers just means no page is open.
        let _ = self.changed.send(());
    }

    async fn delete(&self, ids: &[String]) -> io::Result<()> {
        for id in ids {
            match fs::remove_dir_all(self.drops_dir.join(id)).await {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => self.lock().retain(|drop| &drop.id != id),
            }
        }
        Ok(())
    }

    /// Time-sortable and unique within this process: milliseconds plus a counter.
    fn new_id(&self) -> String {
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let n = self.counter.fetch_add(1, Ordering::Relaxed) % 10_000;
        format!("{millis:013}-{n:04}")
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Vec<Drop>> {
        self.drops.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

async fn load(dir: &Path, id: String) -> io::Result<Drop> {
    let meta = serde_json::from_slice(&fs::read(dir.join("meta.json")).await?)?;
    let text = fs::read_to_string(dir.join("content")).await?;
    Ok(Drop { id, meta, text })
}
