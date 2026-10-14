use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{SeeOther, see_other},
        route,
    },
};

use crate::{auth, drops};

#[derive(Deserialize)]
struct SendForm {
    text: String,
}

/// The send box on the main page.
#[route(POST)]
async fn send(cx: &Cx, Form(form): Form<SendForm>) -> Result<SeeOther> {
    auth::require(cx)?;
    if !form.text.trim().is_empty() {
        drops::drops(cx).add(form.text, "browser");
    }
    Ok(see_other("/"))
}
