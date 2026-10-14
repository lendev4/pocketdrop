//! Turns a multipart request into drops. Shared by uploads from the page
//! (`POST /drops`) and the Android Share Target fallback (`POST /share`).
//!
//! Text fields (`title`, `text`, `url`) become one text drop; every file part
//! becomes its own file drop, streamed to disk as it arrives. An `expiry`
//! field applies to the parts after it.

use topcoat::{
    context::Cx,
    router::{content::multipart::Multipart, error::ContentTooLargeError},
};

use crate::{config, store::WriteError};

pub enum IngestError {
    TextTooLarge,
    Write(WriteError),
    Request(topcoat::Error),
}

impl From<WriteError> for IngestError {
    fn from(error: WriteError) -> Self {
        Self::Write(error)
    }
}

impl From<topcoat::Error> for IngestError {
    fn from(error: topcoat::Error) -> Self {
        Self::Request(error)
    }
}

impl IngestError {
    /// Status code and a message fit to show the user.
    pub fn status_and_message(&self) -> (u16, String) {
        match self {
            Self::TextTooLarge => (413, "The text is over the size limit.".to_owned()),
            Self::Write(WriteError::FileTooLarge) => (413, "The file is over the size limit.".to_owned()),
            Self::Write(WriteError::StorageFull) => {
                (507, "Storage is full. Delete some drops and try again.".to_owned())
            }
            Self::Write(WriteError::Io(error)) => {
                eprintln!("storing a drop failed: {error}");
                (500, "Storing the drop failed.".to_owned())
            }
            Self::Request(error) if error.downcast_ref::<ContentTooLargeError>().is_some() => {
                (413, "The upload is over the size limit.".to_owned())
            }
            Self::Request(_) => (400, "The upload was interrupted or malformed.".to_owned()),
        }
    }
}

/// Returns how many drops were created.
pub async fn ingest(cx: &Cx, form: &mut Multipart, source: &str) -> Result<usize, IngestError> {
    let config = config(cx);
    let store = crate::store(cx);
    let mut expiry = config.default_expiry;
    let (mut title, mut text, mut url) = (String::new(), String::new(), String::new());
    let mut created = 0;

    while let Some(mut field) = form.next_field().await? {
        let name = field.name().unwrap_or_default().to_owned();

        if let Some(file_name) = field.file_name().map(str::to_owned) {
            let mime = field
                .content_type()
                .filter(|mime| is_plain_mime(mime))
                .unwrap_or("application/octet-stream")
                .to_owned();
            let mut upload = store.begin_file().await.map_err(WriteError::Io)?;
            loop {
                let chunk = match field.chunk().await {
                    Ok(Some(chunk)) => chunk,
                    Ok(None) => break,
                    Err(error) => {
                        store.discard(upload).await;
                        return Err(error.into());
                    }
                };
                if let Err(error) = store.write_chunk(&mut upload, &chunk).await {
                    store.discard(upload).await;
                    return Err(error.into());
                }
            }
            // Browsers send an empty, nameless part when no file was picked.
            if upload.len() == 0 && file_name.is_empty() {
                store.discard(upload).await;
                continue;
            }
            store
                .commit_file(upload, display_name(&file_name), mime, expiry, source)
                .await?;
            created += 1;
            continue;
        }

        let value = field.text().await?;
        match name.as_str() {
            "expiry" => expiry = config.expiry_or_default(Some(&value)),
            "title" => title = value,
            "text" => text = value,
            "url" => url = value,
            _ => {}
        }
    }

    let content = combine(&title, &text, &url);
    if !content.is_empty() {
        if content.len() > config.max_text_bytes {
            return Err(IngestError::TextTooLarge);
        }
        store.add_text(content, expiry, source).await?;
        created += 1;
    }
    Ok(created)
}

/// Apps fill title/text/url inconsistently: many put the URL inside text.
fn combine(title: &str, text: &str, url: &str) -> String {
    let (title, text, url) = (title.trim(), text.trim(), url.trim());
    let mut parts = Vec::new();
    if !text.is_empty() {
        parts.push(text);
    }
    if !url.is_empty() && !text.contains(url) {
        parts.push(url);
    }
    if parts.is_empty() && !title.is_empty() {
        parts.push(title);
    }
    parts.join("\n")
}

/// The last path segment without control characters, capped in length.
fn display_name(raw: &str) -> String {
    let base = raw.rsplit(['/', '\\']).next().unwrap_or_default();
    let clean: String = base.chars().filter(|c| !c.is_control()).take(200).collect();
    match clean.trim() {
        "" | "." | ".." => "file".to_owned(),
        name => name.to_owned(),
    }
}

/// A `type/subtype` without parameters or odd characters.
fn is_plain_mime(mime: &str) -> bool {
    let mut parts = mime.split('/');
    let valid = |part: Option<&str>| {
        part.is_some_and(|p| {
            !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric() || "!#$&-^_.+".contains(c))
        })
    };
    valid(parts.next()) && valid(parts.next()) && parts.next().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combine_avoids_duplicate_urls() {
        assert_eq!(combine("", "look https://a.b", "https://a.b"), "look https://a.b");
        assert_eq!(combine("", "look", "https://a.b"), "look\nhttps://a.b");
        assert_eq!(combine("Title", "", ""), "Title");
        assert_eq!(combine("", "  ", ""), "");
    }

    #[test]
    fn display_name_strips_paths() {
        assert_eq!(display_name("../../etc/passwd"), "passwd");
        assert_eq!(display_name("C:\\Users\\me\\photo.jpg"), "photo.jpg");
        assert_eq!(display_name("a\u{7}b.txt"), "ab.txt");
        assert_eq!(display_name(".."), "file");
        assert_eq!(display_name(""), "file");
    }

    #[test]
    fn plain_mime_only() {
        assert!(is_plain_mime("image/jpeg"));
        assert!(is_plain_mime("application/vnd.ms-excel"));
        assert!(!is_plain_mime("text/html; charset=utf-8"));
        assert!(!is_plain_mime("image"));
        assert!(!is_plain_mime("a/b/c"));
    }
}
