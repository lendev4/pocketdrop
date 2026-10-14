use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{SeeOther, content_too_large, see_other},
        route,
    },
};

use crate::{auth, config, store};

#[derive(Deserialize)]
struct SendForm {
    text: String,
    expiry: Option<String>,
}

/// The send box on the main page.
#[route(POST)]
async fn send(cx: &Cx, Form(form): Form<SendForm>) -> Result<SeeOther> {
    auth::require(cx)?;
    auth::require_same_origin(cx)?;
    let config = config(cx);
    if form.text.len() > config.max_text_bytes {
        return Err(content_too_large().into());
    }
    if !form.text.trim().is_empty() {
        let expiry = config.expiry_or_default(form.expiry.as_deref());
        store(cx).add_text(form.text, expiry, "browser").await?;
    }
    Ok(see_other("/"))
}
