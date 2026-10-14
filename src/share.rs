use http::{StatusCode, header};
use topcoat::{
    Result,
    context::Cx,
    router::{Body, content::multipart::Multipart, response::Response, route},
    view::{ViewExt, view},
};

use crate::{auth, ingest::ingest, shell};

/// Android Share Target, used only when the service worker is not active
/// (it normally intercepts this request and shows the send screen instead).
/// Stores the share with the default expiry and goes to the main page;
/// only problems get a page of their own.
#[route(POST)]
async fn share(cx: &Cx, mut form: Multipart) -> Result<Response> {
    let logged_in = auth::is_logged_in(cx);
    let outcome = if logged_in {
        Some(ingest(cx, &mut form, "share").await)
    } else {
        None
    };

    let (heading, detail) = match &outcome {
        Some(Ok(sent)) if *sent > 0 => {
            return Ok(Response::builder()
                .status(StatusCode::SEE_OTHER)
                .header(header::LOCATION, "/?sent")
                .body(Body::empty())?);
        }
        None => ("Not logged in", "The share arrived without a login cookie.".to_owned()),
        Some(Ok(_)) => ("Nothing to send", "The share contained no text, link or file.".to_owned()),
        Some(Err(error)) => ("Not sent", error.status_and_message().1),
    };

    let page = view! { cx =>
        shell(title: "PocketDrop",
            <div class="notice">
                <h1>(heading)</h1>
                if !detail.is_empty() {
                    <p>(detail)</p>
                }
                if !logged_in {
                    <a href="/login">"Log in"</a>
                }
                <a href="/">"Open PocketDrop"</a>
            </div>
        )
    }
    .single()
    .await?;
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(page.render(cx)))?)
}
