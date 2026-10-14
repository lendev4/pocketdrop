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
    let query = uri(cx).query().unwrap_or_default();
    let failed = query.split('&').any(|pair| pair == "error=1");
    let next = auth::safe_next(
        query.split('&').find_map(|pair| pair.strip_prefix("next=")),
    )
    .to_owned();

    Ok(view! {
        shell(title: "Log in · PocketDrop",
            <h1>"PocketDrop"</h1>
            <form class="send" method="POST" action="/login">
                <input type="hidden" name="next" value=(next.clone())>
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
    next: Option<String>,
}

#[route(POST)]
async fn login(cx: &Cx, Form(form): Form<LoginForm>) -> Result<SeeOther> {
    let next = auth::safe_next(form.next.as_deref());
    if auth::log_in(cx, &form.passphrase) {
        Ok(see_other(next))
    } else if next == "/" {
        Ok(see_other("/login?error=1"))
    } else {
        Ok(see_other(format!("/login?error=1&next={next}")))
    }
}
