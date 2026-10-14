//! Drops on disk, one folder each:
//!
//! ```text
//! <DATA_DIR>/drops/<id>/meta.json
//! <DATA_DIR>/drops/<id>/content     ← the text, or the file's bytes
//! <DATA_DIR>/tmp/<id>/              ← drops being written, renamed into drops/ when complete
//! ```
//!
//! Ids sort by creation time. Text is small, so text drops are also kept in
//! memory; file contents stay on disk. The disk copy is what survives restarts.

use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicU32, AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use serde::{Deserialize, Serialize};
use tokio::{
    fs::{self, File},
    io::AsyncWriteExt,
    sync::broadcast,
};

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
    File,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Meta {
    pub kind: Kind,
    pub created_at: u64,
    /// The rule chosen when sending, e.g. `24h` or `never`.
    pub expiry: String,
    pub expires_at: Option<u64>,
    pub source: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<FileMeta>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct FileMeta {
    /// Display name only; the bytes always live at `content`.
    pub name: String,
    pub mime: String,
    pub size: u64,
}

#[derive(Clone)]
pub struct Drop {
    pub id: String,
    pub meta: Meta,
    /// Empty for file drops.
    pub text: String,
}

impl Drop {
    pub fn is_expired(&self, now: u64) -> bool {
        self.meta.expires_at.is_some_and(|at| at <= now)
    }

    fn size(&self) -> u64 {
        self.meta.file.as_ref().map_or(self.text.len() as u64, |file| file.size)
    }
}

#[derive(Debug)]
pub enum WriteError {
    /// The file is larger than the per-file limit.
    FileTooLarge,
    /// Storing it would exceed the total storage cap.
    StorageFull,
    Io(io::Error),
}

impl From<io::Error> for WriteError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// A file drop being written. Dropping it without [`Store::commit_file`]
/// leaves its folder in `tmp/`, which [`Store::discard`] or the next start removes.
pub struct Upload {
    id: String,
    dir: PathBuf,
    file: File,
    written: u64,
}

impl Upload {
    pub fn len(&self) -> u64 {
        self.written
    }
}

pub struct Store {
    drops_dir: PathBuf,
    tmp_dir: PathBuf,
    max_drops: usize,
    max_file_bytes: u64,
    max_total_bytes: u64,
    /// Oldest first.
    drops: Mutex<Vec<Drop>>,
    /// Bytes held by stored drops plus uploads in progress.
    total_bytes: AtomicU64,
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
        let total_bytes = drops.iter().map(Drop::size).sum();

        Ok(Self {
            drops_dir,
            tmp_dir,
            max_drops: config.max_drops,
            max_file_bytes: config.max_file_bytes,
            max_total_bytes: config.max_total_bytes,
            drops: Mutex::new(drops),
            total_bytes: AtomicU64::new(total_bytes),
            changed: broadcast::channel(16).0,
            counter: AtomicU32::new(0),
        })
    }

    pub async fn add_text(&self, text: String, expiry: Expiry, source: &str) -> Result<(), WriteError> {
        let size = text.len() as u64;
        self.reserve(size)?;

        let trimmed = text.trim();
        let is_link = (trimmed.starts_with("https://") || trimmed.starts_with("http://"))
            && !trimmed.contains(char::is_whitespace);
        let meta = new_meta(if is_link { Kind::Link } else { Kind::Text }, expiry, source, None);

        let id = self.new_id();
        let result = async {
            let staging = self.tmp_dir.join(&id);
            fs::create_dir(&staging).await?;
            fs::write(staging.join("content"), &text).await?;
            self.publish(&staging, &id, &meta).await
        }
        .await;
        if let Err(error) = result {
            self.release(size);
            return Err(error.into());
        }

        self.lock().push(Drop { id, meta, text });
        self.after_add().await;
        Ok(())
    }

    pub async fn begin_file(&self) -> io::Result<Upload> {
        let id = self.new_id();
        let dir = self.tmp_dir.join(&id);
        fs::create_dir(&dir).await?;
        let file = File::create(dir.join("content")).await?;
        Ok(Upload { id, dir, file, written: 0 })
    }

    /// Appends a chunk, enforcing the per-file and total limits as bytes
    /// arrive. On error, call [`discard`](Self::discard).
    pub async fn write_chunk(&self, upload: &mut Upload, chunk: &[u8]) -> Result<(), WriteError> {
        let len = chunk.len() as u64;
        if upload.written + len > self.max_file_bytes {
            return Err(WriteError::FileTooLarge);
        }
        self.reserve(len)?;
        upload.written += len;
        upload.file.write_all(chunk).await?;
        Ok(())
    }

    pub async fn commit_file(
        &self,
        mut upload: Upload,
        name: String,
        mime: String,
        expiry: Expiry,
        source: &str,
    ) -> Result<(), WriteError> {
        upload.file.flush().await?;
        upload.file.sync_all().await?;
        let file = FileMeta { name, mime, size: upload.written };
        let meta = new_meta(Kind::File, expiry, source, Some(file));
        if let Err(error) = self.publish(&upload.dir, &upload.id, &meta).await {
            self.discard(upload).await;
            return Err(error.into());
        }

        self.lock().push(Drop { id: upload.id, meta, text: String::new() });
        self.after_add().await;
        Ok(())
    }

    /// Throws away an unfinished upload and gives back its reserved space.
    pub async fn discard(&self, upload: Upload) {
        self.release(upload.written);
        drop(upload.file);
        if let Err(error) = fs::remove_dir_all(&upload.dir).await {
            eprintln!("cannot remove unfinished upload {}: {error}", upload.id);
        }
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

    /// A live file drop and the path of its bytes.
    pub fn file(&self, id: &str) -> Option<(FileMeta, PathBuf)> {
        let now = unix_now();
        self.lock()
            .iter()
            .find(|drop| drop.id == id && !drop.is_expired(now))
            .and_then(|drop| drop.meta.file.clone())
            .map(|file| (file, self.drops_dir.join(id).join("content")))
    }

    pub fn total_bytes(&self) -> u64 {
        self.total_bytes.load(Ordering::Relaxed)
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

    async fn after_add(&self) {
        self.cleanup().await;
        self.notify();
    }

    /// Writes the metadata and moves a finished drop from tmp/ into drops/.
    async fn publish(&self, staging: &Path, id: &str, meta: &Meta) -> io::Result<()> {
        fs::write(staging.join("meta.json"), serde_json::to_vec_pretty(meta)?).await?;
        fs::rename(staging, self.drops_dir.join(id)).await
    }

    /// Claims space for incoming bytes, refusing if the total cap would be passed.
    fn reserve(&self, bytes: u64) -> Result<(), WriteError> {
        self.total_bytes
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |total| {
                (total + bytes <= self.max_total_bytes).then_some(total + bytes)
            })
            .map(|_| ())
            .map_err(|_| WriteError::StorageFull)
    }

    fn release(&self, bytes: u64) {
        self.total_bytes.fetch_sub(bytes, Ordering::Relaxed);
    }

    async fn delete(&self, ids: &[String]) -> io::Result<()> {
        for id in ids {
            match fs::remove_dir_all(self.drops_dir.join(id)).await {
                Err(error) if error.kind() != io::ErrorKind::NotFound => return Err(error),
                _ => {
                    let mut drops = self.lock();
                    if let Some(index) = drops.iter().position(|drop| &drop.id == id) {
                        let removed = drops.remove(index);
                        self.release(removed.size());
                    }
                }
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

fn new_meta(kind: Kind, expiry: Expiry, source: &str, file: Option<FileMeta>) -> Meta {
    let now = unix_now();
    Meta {
        kind,
        created_at: now,
        expiry: expiry.to_string(),
        expires_at: expiry.seconds().map(|secs| now + secs),
        source: source.to_owned(),
        file,
    }
}

async fn load(dir: &Path, id: String) -> io::Result<Drop> {
    let meta: Meta = serde_json::from_slice(&fs::read(dir.join("meta.json")).await?)?;
    let text = match meta.kind {
        Kind::File => String::new(),
        Kind::Text | Kind::Link => fs::read_to_string(dir.join("content")).await?,
    };
    Ok(Drop { id, meta, text })
}
