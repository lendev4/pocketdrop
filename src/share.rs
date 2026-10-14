use topcoat::{
    Result,
    context::Cx,
    router::{
        content::{Html, multipart::Multipart},
        route,
    },
    view::{ViewExt, view},
};

use crate::{auth, config, shell, store};

/// Android Share Target. M0 posts here directly; the service worker takes
/// over in M2.
#[route(POST)]
async fn share(cx: &Cx, mut form: Multipart) -> Result<Html<String>> {
    let logged_in = auth::is_logged_in(cx);

    let (mut title, mut text, mut url) = (String::new(), String::new(), String::new());
    while let Some(field) = form.next_field().await? {
        let name = field.name().unwrap_or_default().to_owned();
        let value = field.text().await?;
        match name.as_str() {
            "title" => title = value,
            "text" => text = value,
            "url" => url = value,
            _ => {}
        }
    }

    let content = combine(&title, &text, &url);
    let config = config(cx);
    let too_large = content.len() > config.max_text_bytes;
    let sent = logged_in && !content.is_empty() && !too_large;
    if sent {
        store(cx).add_text(content, config.default_expiry, "share").await?;
    }

    let page = view! { cx =>
        shell(title: "PocketDrop",
            <div class="notice">
                if sent {
                    <h1>"Sent ✓"</h1>
                } else if too_large {
                    <h1>"Too large"</h1>
                    <p>"The shared text is over the size limit."</p>
                } else if !logged_in {
                    <h1>"Not logged in"</h1>
                    <p>"The share arrived without a login cookie."</p>
                    <a href="/login">"Log in"</a>
                } else {
                    <h1>"Nothing to send"</h1>
                    <p>"The share contained no text or link."</p>
                }
                <a href="/">"Open PocketDrop"</a>
            </div>
        )
    }
    .single()
    .await?;
    Ok(Html(page.render(cx)))
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
