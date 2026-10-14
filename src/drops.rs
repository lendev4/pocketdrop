use futures_util::TryStreamExt;
use http::{StatusCode, header};
use http_body::Frame;
use http_body_util::StreamBody;
use tokio_util::io::ReaderStream;
use topcoat::{
    Result,
    context::Cx,
    router::{
        Body,
        content::multipart::Multipart,
        error::{not_found, unauthorized},
        path_param,
        request::{headers, uri},
        response::Response,
        route,
    },
};

use crate::{auth, ingest::ingest, store, store::FileMeta};

path_param!(drop_id);

/// Upload text and files. Scripts ask for JSON-ish replies (status plus a
/// plain-text message); a plain form post is redirected back to the page.
#[route(POST)]
async fn create(cx: &Cx, mut form: Multipart) -> Result<Response> {
    auth::require_api(cx)?;
    let from_script = headers(cx)
        .get(header::ACCEPT)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|accept| accept.contains("application/json"));

    let response = match ingest(cx, &mut form, "browser").await {
        Ok(_) if from_script => Response::builder().status(StatusCode::CREATED).body(Body::empty())?,
        Ok(_) => Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, "/")
            .body(Body::empty())?,
        Err(error) => {
            let (status, message) = error.status_and_message();
            Response::builder()
                .status(status)
                .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
                .body(Body::from(message))?
        }
    };
    Ok(response)
}

/// Clear all. Open pages update through the live stream.
#[route(DELETE)]
async fn clear(cx: &Cx) -> Result<()> {
    auth::require_api(cx)?;
    store(cx).clear().await?;
    Ok(())
}

#[route(DELETE "./{drop_id}")]
async fn delete(cx: &Cx) -> Result<()> {
    auth::require_api(cx)?;
    let id: &str = path_param::<DropId>(cx);
    if store(cx).remove(id).await? {
        Ok(())
    } else {
        Err(not_found().into())
    }
}

/// Streams a file drop. Always a download, except `?inline` previews of
/// raster images, which are safe to show in the page.
#[route(GET "./{drop_id}/file")]
async fn file(cx: &Cx) -> Result<Response> {
    if !auth::is_logged_in(cx) {
        return Err(unauthorized().into());
    }
    let id: &str = path_param::<DropId>(cx);
    let (meta, path) = store(cx).file(id).ok_or_else(not_found)?;
    let inline = uri(cx).query().is_some_and(|q| q.split('&').any(|p| p == "inline"))
        && is_previewable(&meta.mime);

    let file = tokio::fs::File::open(path).await?;
    let chunks = ReaderStream::new(file).map_ok(Frame::data);

    Ok(Response::builder()
        .header(header::CONTENT_TYPE, &meta.mime)
        .header(header::CONTENT_LENGTH, meta.size)
        .header(header::CONTENT_DISPOSITION, disposition(&meta, inline))
        .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
        .header(header::CONTENT_SECURITY_POLICY, "default-src 'none'; sandbox")
        .header(header::CACHE_CONTROL, "private, max-age=3600")
        .body(Body::new(StreamBody::new(chunks)))?)
}

/// Image types every browser renders without running anything. SVG is
/// deliberately missing: it can carry scripts.
pub fn is_previewable(mime: &str) -> bool {
    matches!(mime, "image/jpeg" | "image/png" | "image/gif" | "image/webp" | "image/avif")
}

/// `attachment` with the original name: an ASCII fallback plus the exact
/// UTF-8 name for browsers that support RFC 6266.
fn disposition(meta: &FileMeta, inline: bool) -> String {
    let kind = if inline { "inline" } else { "attachment" };
    let ascii: String = meta
        .name
        .chars()
        .map(|c| if c.is_ascii_graphic() && c != '"' && c != '\\' || c == ' ' { c } else { '_' })
        .collect();
    let encoded: String = meta
        .name
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect();
    format!("{kind}; filename=\"{ascii}\"; filename*=UTF-8''{encoded}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disposition_escapes_names() {
        let meta = FileMeta { name: "Grüße \"x\".pdf".to_owned(), mime: "application/pdf".to_owned(), size: 1 };
        assert_eq!(
            disposition(&meta, false),
            "attachment; filename=\"Gr__e _x_.pdf\"; filename*=UTF-8''Gr%C3%BC%C3%9Fe%20%22x%22.pdf"
        );
    }
}
