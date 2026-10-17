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

use crate::{
    auth::{self, LoginResult},
    shell,
};

#[page]
async fn page(cx: &Cx) -> Result<impl View> {
    let query = uri(cx).query().unwrap_or_default();
    let failed = query.split('&').any(|pair| pair == "error=1");
    let blocked = query.split('&').any(|pair| pair == "error=wait");
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
                if blocked {
                    <p class="error">"Too many attempts. Wait a minute and try again."</p>
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
    let error = match auth::log_in(cx, &form.passphrase) {
        LoginResult::LoggedIn => return Ok(see_other(next)),
        LoginResult::WrongPassphrase => "1",
        LoginResult::TooManyAttempts => "wait",
    };
    if next == "/" {
        Ok(see_other(format!("/login?error={error}")))
    } else {
        Ok(see_other(format!("/login?error={error}&next={next}")))
    }
}
