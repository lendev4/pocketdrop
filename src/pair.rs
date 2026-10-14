//! QR pairing: a logged-in device shows a one-time link as a QR code;
//! opening it on another device logs that device in.

use std::{
    collections::HashMap,
    sync::{Mutex, PoisonError},
    time::Duration,
};

use futures_util::{Stream, StreamExt, stream};
use http::{StatusCode, header};
use qrcode::{QrCode, render::svg};
use tokio::sync::oneshot;
use topcoat::{
    Result,
    context::{Cx, app_context},
    datastar::PatchElements,
    router::{
        Body,
        content::sse::{Event, Sse},
        path_param,
        request::headers,
        response::Response,
        route,
    },
    view::{ViewExt, view},
};

use crate::{auth, shell, store::unix_now};

const VALID_SECS: u64 = 5 * 60;

path_param!(token);

#[derive(Default)]
pub struct Pairing {
    pending: Mutex<HashMap<String, Pending>>,
}

struct Pending {
    expires_at: u64,
    /// Tells the showing device that the link was used.
    used: oneshot::Sender<()>,
}

impl Pairing {
    fn create(&self) -> std::result::Result<(String, oneshot::Receiver<()>), getrandom::Error> {
        let mut bytes = [0u8; 24];
        getrandom::fill(&mut bytes)?;
        let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        let (used, rx) = oneshot::channel();

        let now = unix_now();
        let mut pending = self.lock();
        pending.retain(|_, p| p.expires_at > now);
        pending.insert(token.clone(), Pending { expires_at: now + VALID_SECS, used });
        Ok((token, rx))
    }

    /// Removes the token; returns its notifier if it was still valid.
    fn take(&self, token: &str) -> Option<oneshot::Sender<()>> {
        let pending = self.lock().remove(token)?;
        (pending.expires_at > unix_now()).then_some(pending.used)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, Pending>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Opens the pairing panel. The response is a stream: the QR code first,
/// then "paired" or "expired" once that is known.
#[route(POST)]
async fn start(cx: &Cx) -> Result<Sse<impl Stream<Item = Result<Event>> + use<>>> {
    auth::require_api(cx)?;
    let (token, used) = app_context::<Pairing>(cx)
        .create()
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let url = format!("{}/pair/{token}", origin(cx));
    let qr = qr_data_uri(&url)?;

    let showing = view! { cx =>
        <div id="pair" class="pair-panel">
            <p>"Scan this with the other device, or open the link there:"</p>
            <img class="qr" src=(qr) alt="QR code for the pairing link">
            <code class="pair-url">(url.clone())</code>
            <p class="muted">"Works once, for 5 minutes."</p>
            <button class="quiet" data-close-pair="true">"Close"</button>
        </div>
    }
    .single()
    .await?
    .render(cx);
    let finished = |message: &'static str| {
        let cx = cx.clone();
        async move {
            let html = view! { cx =>
                <div id="pair" class="pair-panel">
                    <p class="notice-inline">(message)</p>
                    <button class="quiet" data-close-pair="true">"Close"</button>
                </div>
            }
            .single()
            .await?
            .render(&cx);
            Ok::<_, topcoat::Error>(html)
        }
    };
    let paired = finished("Device paired ✓").await?;
    let expired = finished("The pairing link expired. Start again to pair another device.").await?;

    let first = stream::once(async move { Ok(PatchElements::new(showing).into()) });
    let outcome = stream::once(async move {
        let html = match tokio::time::timeout(Duration::from_secs(VALID_SECS), used).await {
            Ok(Ok(())) => paired,
            _ => expired,
        };
        Ok(PatchElements::new(html).into())
    });
    Ok(Sse::new(first.chain(outcome)))
}

/// The link from the QR code: logs this device in.
#[route(GET "./{token}")]
async fn accept(cx: &Cx) -> Result<Response> {
    let token: &str = path_param::<Token>(cx);
    if let Some(used) = app_context::<Pairing>(cx).take(token) {
        auth::start_session(cx);
        // The showing device may have closed its panel already.
        let _ = used.send(());
        return Ok(Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, "/")
            .body(Body::empty())?);
    }

    let page = view! { cx =>
        shell(title: "PocketDrop",
            <div class="notice">
                <h1>"Link not valid"</h1>
                <p>"This pairing link has expired or was already used."</p>
                <a href="/login">"Log in with the passphrase"</a>
            </div>
        )
    }
    .single()
    .await?
    .render(cx);
    Ok(Response::builder()
        .status(StatusCode::GONE)
        .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
        .body(Body::from(page))?)
}

/// This site's origin as the browser sees it, also behind Railway's proxy.
fn origin(cx: &Cx) -> String {
    let headers = headers(cx);
    let get = |name: &str| headers.get(name).and_then(|value| value.to_str().ok());
    let proto = get("x-forwarded-proto").unwrap_or("http");
    let host = get("x-forwarded-host").or_else(|| get("host")).unwrap_or("localhost");
    format!("{proto}://{host}")
}

fn qr_data_uri(text: &str) -> Result<String> {
    let code = QrCode::new(text.as_bytes()).map_err(|error| std::io::Error::other(error.to_string()))?;
    let svg = code
        .render::<svg::Color<'_>>()
        .min_dimensions(240, 240)
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();
    let encoded: String = svg
        .bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b' ' | b'=' | b':' | b'/' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect();
    Ok(format!("data:image/svg+xml,{encoded}"))
}
