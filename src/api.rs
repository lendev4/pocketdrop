//! Token-authenticated API for scripts and the terminal. Enabled by setting
//! `API_TOKEN`; requests send `Authorization: Bearer <API_TOKEN>`.

use http::{StatusCode, header};
use topcoat::{
    Result,
    context::Cx,
    router::{Body, content::multipart::Multipart, response::Response, route},
};

use crate::{auth, drops::file_response, ingest::ingest, store, store::Kind};

/// Same fields as the page: `text`, `files` (repeatable) and `expiry`.
#[route(POST "./drops")]
async fn create(cx: &Cx, mut form: Multipart) -> Result<Response> {
    auth::require_token(cx)?;
    match ingest(cx, &mut form, "api").await {
        Ok(0) => plain(StatusCode::BAD_REQUEST, "Nothing to drop: send a text or files field.\n"),
        Ok(count) => plain(StatusCode::CREATED, &format!("Dropped {count}.\n")),
        Err(error) => {
            let (status, message) = error.status_and_message();
            plain(StatusCode::from_u16(status)?, &format!("{message}\n"))
        }
    }
}

/// The newest drop: text is printed, a file is downloaded. Uses up `once` drops.
#[route(GET "./latest")]
async fn latest(cx: &Cx) -> Result<Response> {
    auth::require_token(cx)?;
    let store = store(cx);
    let Some(drop) = store.list().into_iter().next() else {
        return plain(StatusCode::NOT_FOUND, "No drops.\n");
    };

    let response = match (drop.meta.kind, store.file(&drop.id)) {
        (Kind::File, Some((meta, path))) => file_response(tokio::fs::File::open(path).await?, &meta, false)?,
        _ => plain(StatusCode::OK, &drop.text)?,
    };
    store.consume(&drop.id).await?;
    Ok(response)
}

fn plain(status: StatusCode, text: &str) -> Result<Response> {
    Ok(Response::builder()
        .status(status)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(text.to_owned()))?)
}
