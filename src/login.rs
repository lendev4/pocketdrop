use serde::Deserialize;
use topcoat::{
    Result,
    context::Cx,
    router::{
        content::Form,
        error::{SeeOther, see_other},
        page, route,
        request::uri,
    },
    view::{View, view},
};

use crate::{auth, shell};

#[page]
async fn page(cx: &Cx) -> Result<impl View> {
    let failed = uri(cx).query().is_some_and(|q| q.contains("error"));

    Ok(view! {
        shell(title: "Log in · PocketDrop",
            <h1>"PocketDrop"</h1>
            <form class="send" method="POST" action="/login">
                <input type="password" name="passphrase" placeholder="Passphrase" autocomplete="current-password" required="true" autofocus="true">
                if failed {
                    <p class="error">"Wrong passphrase."</p>
                }
                <button>"Log in"</button>
            </form>
        )
    })
}

#[derive(Deserialize)]
struct LoginForm {
    passphrase: String,
}

#[route(POST)]
async fn login(cx: &Cx, Form(form): Form<LoginForm>) -> Result<SeeOther> {
    if auth::log_in(cx, &form.passphrase) {
        Ok(see_other("/"))
    } else {
        Ok(see_other("/login?error=1"))
    }
}
