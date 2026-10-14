use topcoat::{
    Result,
    context::Cx,
    router::{
        content::{Html, multipart::Multipart},
        route,
    },
    view::{ViewExt, view},
};

use crate::{auth, ingest::ingest, shell};

/// Android Share Target, used only when the service worker is not active
/// (it normally intercepts this request and shows the send screen instead).
/// Stores the share with the default expiry.
#[route(POST)]
async fn share(cx: &Cx, mut form: Multipart) -> Result<Html<String>> {
    let logged_in = auth::is_logged_in(cx);
    let outcome = if logged_in {
        Some(ingest(cx, &mut form, "share").await)
    } else {
        None
    };

    let (heading, detail) = match &outcome {
        None => ("Not logged in", "The share arrived without a login cookie.".to_owned()),
        Some(Ok(0)) => ("Nothing to send", "The share contained no text, link or file.".to_owned()),
        Some(Ok(_)) => ("Sent ✓", String::new()),
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
    Ok(Html(page.render(cx)))
}
