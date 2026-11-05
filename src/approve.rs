//! The logged-in side of login by approval (see `signin`): the page the QR
//! code opens, and a form for typing the short code instead.

use http::header;
use serde::Deserialize;
use topcoat::{
    Result,
    context::{Cx, app_context},
    router::{
        content::Form,
        error::{SeeOther, see_other},
        Body, page, path_param, route,
        request::uri,
        response::Response,
    },
    view::{View, ViewExt, view},
};

use crate::{
    auth, shell,
    signin::{self, Decision, SignIns},
};

path_param!(token);

/// Type the code shown on the device that wants to log in.
#[page]
async fn code_form(cx: &Cx) -> Result<impl View> {
    auth::require(cx)?;
    let unknown = uri(cx).query().unwrap_or_default().split('&').any(|pair| pair == "error=1");

    Ok(view! {
        shell(title: "Enter a login code · PocketDrop",
            <h1>"Enter a login code"</h1>
            <form class="send" method="POST" action="/approve">
                <p class="muted">"Open PocketDrop on the device you want to log in. Its login page shows a code."</p>
                <input type="text" name="code" placeholder="ABC-123" autocomplete="off" autocapitalize="characters" spellcheck="false" required="true" autofocus="true" class="login-code-input">
                if unknown {
                    <p class="error">"No device is waiting with that code. Check it, or show a new code on the other device."</p>
                }
                <button>"Continue"</button>
            </form>
            <p class="share-home"><a href="/">"Back"</a></p>
        )
    })
}

#[derive(Deserialize)]
struct CodeForm {
    code: String,
}

#[route(POST)]
async fn enter(cx: &Cx, Form(form): Form<CodeForm>) -> Result<SeeOther> {
    auth::require_api(cx)?;
    match app_context::<SignIns>(cx).by_code(&form.code) {
        Some(waiting) => Ok(see_other(format!("/approve/{}", waiting.token))),
        None => Ok(see_other("/approve?error=1")),
    }
}

/// Opened from the QR code: asks before letting the device in.
#[route(GET "./{token}")]
async fn confirm(cx: &Cx) -> Result<Response> {
    auth::require(cx)?;
    let token: &str = path_param::<Token>(cx);
    let waiting = app_context::<SignIns>(cx).by_token(token);

    let rendered = view! { cx =>
        shell(title: "Log in a device? · PocketDrop",
            if let Some(waiting) = waiting {
                <div class="notice">
                    <h1>"Log in this device?"</h1>
                    <p><strong>(waiting.device.clone())</strong></p>
                    <p>"It shows the code"</p>
                    <code class="login-code">(signin::display_code(&waiting.code))</code>
                    <p class="muted">"Approve only if this is your device and it shows the same code."</p>
                    <form method="POST" action=(format!("/approve/{}", waiting.token)) class="approve-buttons">
                        <button name="decision" value="approve" class="send-button">"Approve"</button>
                        <button name="decision" value="decline" class="quiet">"Decline"</button>
                    </form>
                </div>
            } else {
                not_waiting()
            }
        )
    }
    .single()
    .await?
    .render(cx);
    html(rendered)
}

#[derive(Deserialize)]
struct DecisionForm {
    decision: String,
}

#[route(POST "./{token}")]
async fn decide(cx: &Cx, Form(form): Form<DecisionForm>) -> Result<Response> {
    auth::require_api(cx)?;
    let token: &str = path_param::<Token>(cx);
    let sign_ins = app_context::<SignIns>(cx);
    let device = sign_ins.by_token(token).map(|waiting| waiting.device);
    let approve = form.decision == "approve";
    let decision = if approve { Decision::Approved } else { Decision::Declined };
    let done = sign_ins.decide(token, decision).map_err(|error| std::io::Error::other(error.to_string()))?;

    let rendered = view! { cx =>
        shell(title: "PocketDrop",
            if !done {
                not_waiting()
            } else {
                <div class="notice">
                    if approve {
                        <h1>"Approved ✓"</h1>
                        <p>(device.unwrap_or_default()) " is logging in now."</p>
                    } else {
                        <h1>"Declined"</h1>
                        <p>"That device was not logged in."</p>
                    }
                    <a href="/">"Back to PocketDrop"</a>
                </div>
            }
        )
    }
    .single()
    .await?
    .render(cx);
    html(rendered)
}

fn html(rendered: String) -> Result<Response> {
    Ok(Response::builder()
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(rendered))?)
}

#[topcoat::view::component]
async fn not_waiting() -> Result<impl View> {
    Ok(view! {
        <div class="notice">
            <h1>"Nothing to approve"</h1>
            <p>"This login request expired, was already answered, or the other device closed its login page. Show a new code there and try again."</p>
            <a href="/">"Back to PocketDrop"</a>
        </div>
    })
}
