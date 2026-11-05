//! Login by approval: the login page shows a QR code and a short code; a
//! device that is already logged in scans it (or types the code) and
//! approves, and the waiting page logs itself in.
//!
//! The QR code carries only the request token. Approving creates a separate
//! claim token that is sent only to the waiting page over its stream, so a
//! photo of the QR code is not enough to log in.

use std::{
    collections::HashMap,
    net::IpAddr,
    sync::{Mutex, PoisonError},
    time::Duration,
};

use futures_util::{Stream, StreamExt, stream};
use http::{StatusCode, header};
use tokio::sync::oneshot;
use topcoat::{
    Result,
    context::{Cx, app_context},
    datastar::{ExecuteScript, PatchElements},
    router::{
        Body,
        content::sse::{Event, Sse},
        path_param,
        request::{client_ip, headers, uri},
        response::Response,
        route,
    },
    view::{ViewExt, view},
};

use crate::{auth, pair, store::unix_now};

const VALID_SECS: u64 = 5 * 60;
/// The waiting page has this long to pick up its login after approval.
const CLAIM_SECS: u64 = 60;
/// Anyone can open the login page, so cap what that can make us remember.
const MAX_PENDING: usize = 50;
/// So one address can't take every slot.
const MAX_PER_ADDRESS: usize = 3;
/// Unambiguous characters for the short code: no 0/O, 1/I.
const CODE_ALPHABET: &[u8; 32] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
const CODE_LEN: usize = 6;

path_param!(claim);

#[derive(Default)]
pub struct SignIns {
    state: Mutex<State>,
}

#[derive(Default)]
struct State {
    /// Waiting login pages, by the token in their QR code.
    pending: HashMap<String, Request>,
    /// Approved logins not yet picked up, by claim token.
    claims: HashMap<String, Approved>,
}

struct Request {
    ip: Option<IpAddr>,
    code: String,
    device: String,
    next: String,
    expires_at: u64,
    /// `Some(claim token)` when approved, `None` when declined.
    decided: oneshot::Sender<Option<String>>,
}

struct Approved {
    next: String,
    expires_at: u64,
}

/// What the approving device sees about a waiting login.
pub struct Waiting {
    pub token: String,
    pub code: String,
    pub device: String,
}

/// Resolves with `Some(claim token)` on approval, `None` when declined.
type Decided = oneshot::Receiver<Option<String>>;

pub enum Decision {
    Approved,
    Declined,
}

impl SignIns {
    fn create(
        &self,
        ip: Option<IpAddr>,
        device: String,
        next: String,
    ) -> std::result::Result<Option<(Waiting, Decided)>, getrandom::Error> {
        let token = random_hex()?;
        let now = unix_now();
        let mut state = self.lock();
        state.prune(now);
        let from_address = state.pending.values().filter(|request| request.ip == ip).count();
        if state.pending.len() >= MAX_PENDING || from_address >= MAX_PER_ADDRESS {
            return Ok(None);
        }
        let code = loop {
            let code = random_code()?;
            if !state.pending.values().any(|request| request.code == code) {
                break code;
            }
        };
        let (decided, rx) = oneshot::channel();
        state.pending.insert(
            token.clone(),
            Request { ip, code: code.clone(), device: device.clone(), next, expires_at: now + VALID_SECS, decided },
        );
        Ok(Some((Waiting { token, code, device }, rx)))
    }

    /// Looks up a waiting login by the token from its QR code.
    pub fn by_token(&self, token: &str) -> Option<Waiting> {
        let now = unix_now();
        let mut state = self.lock();
        state.prune(now);
        state.pending.get(token).map(|request| Waiting {
            token: token.to_owned(),
            code: request.code.clone(),
            device: request.device.clone(),
        })
    }

    /// Looks up a waiting login by its short code, as typed by a person.
    pub fn by_code(&self, typed: &str) -> Option<Waiting> {
        let code = normalize_code(typed);
        let now = unix_now();
        let mut state = self.lock();
        state.prune(now);
        state.pending.iter().find(|(_, request)| request.code == code).map(|(token, request)| Waiting {
            token: token.clone(),
            code: request.code.clone(),
            device: request.device.clone(),
        })
    }

    /// Approves or declines a waiting login. Returns false if it expired or
    /// the waiting page is gone.
    pub fn decide(&self, token: &str, decision: Decision) -> std::result::Result<bool, getrandom::Error> {
        let now = unix_now();
        let mut state = self.lock();
        state.prune(now);
        let Some(request) = state.pending.remove(token) else {
            return Ok(false);
        };
        match decision {
            Decision::Declined => Ok(request.decided.send(None).is_ok()),
            Decision::Approved => {
                let claim = random_hex()?;
                state.claims.insert(claim.clone(), Approved { next: request.next, expires_at: now + CLAIM_SECS });
                if request.decided.send(Some(claim.clone())).is_ok() {
                    Ok(true)
                } else {
                    state.claims.remove(&claim);
                    Ok(false)
                }
            }
        }
    }

    /// Uses a claim token once; returns where to go next.
    fn claim(&self, claim: &str) -> Option<String> {
        let now = unix_now();
        let mut state = self.lock();
        state.prune(now);
        state.claims.remove(claim).map(|claim| claim.next)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl State {
    fn prune(&mut self, now: u64) {
        // A closed login page drops its receiver; its slot is free again.
        self.pending.retain(|_, request| request.expires_at > now && !request.decided.is_closed());
        self.claims.retain(|_, claim| claim.expires_at > now);
    }
}

/// The login page's panel. The response is a stream: the codes first, then
/// the login (or "declined" / "expired") once that is known.
#[route(GET)]
async fn start(cx: &Cx) -> Result<Sse<impl Stream<Item = Result<Event>> + use<>>> {
    let query = uri(cx).query().unwrap_or_default();
    let next = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("next="))
        .and_then(percent_decode)
        .unwrap_or_default();
    let next = auth::safe_next(Some(&next)).to_owned();
    let user_agent = headers(cx).get(header::USER_AGENT).and_then(|value| value.to_str().ok()).unwrap_or_default();

    let created = app_context::<SignIns>(cx)
        .create(client_ip(cx), describe_device(user_agent), next.clone())
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let restart = format!("@get('/signin?next={}', {{openWhenHidden: true}})", percent_encode(&next));

    let panel = |message: &'static str| {
        let (cx, restart) = (cx.clone(), restart.clone());
        async move {
            let html = view! { cx =>
                <div id="signin" class="pair-panel">
                    <p>(message)</p>
                    <button type="button" class="quiet" data-on:click=(restart)>"Show a new code"</button>
                </div>
            }
            .single()
            .await?
            .render(&cx);
            Ok::<_, topcoat::Error>(html)
        }
    };

    let Some((waiting, decided)) = created else {
        let busy = panel("Too many devices are waiting to log in. Try again in a few minutes.").await?;
        return Ok(Sse::new(stream::once(async move { Ok(PatchElements::new(busy).into()) }).boxed()));
    };

    let url = format!("{}/approve/{}", pair::origin(cx), waiting.token);
    let qr = pair::qr_data_uri(&url)?;
    let code = display_code(&waiting.code);
    let showing = view! { cx =>
        <div id="signin" class="pair-panel">
            <p>"Or log in from a device that's already logged in: scan this with its camera,"</p>
            <img class="qr" src=(qr) alt="QR code for approving this login">
            <p>"or there open " <strong>"⋯ → Enter a login code"</strong> " and type"</p>
            <code class="login-code">(code)</code>
            <p class="muted">"Works once, for 5 minutes."</p>
        </div>
    }
    .single()
    .await?
    .render(cx);
    let logging_in = view! { cx =>
        <div id="signin" class="pair-panel">
            <p class="notice-inline">"Approved ✓ Logging in…"</p>
        </div>
    }
    .single()
    .await?
    .render(cx);
    let declined = panel("The login was declined.").await?;
    let expired = panel("The code expired.").await?;

    let first = stream::once(async move { Ok(PatchElements::new(showing).into()) });
    let outcome = stream::once(async move {
        let events: Vec<Result<Event>> = match tokio::time::timeout(Duration::from_secs(VALID_SECS), decided).await {
            Ok(Ok(Some(claim))) => vec![
                Ok(PatchElements::new(logging_in).into()),
                // The claim is hex, so it is safe inside the string.
                Ok(ExecuteScript::new(format!("location.replace('/signin/{claim}')")).into()),
            ],
            Ok(Ok(None)) => vec![Ok(PatchElements::new(declined).into())],
            _ => vec![Ok(PatchElements::new(expired).into())],
        };
        stream::iter(events)
    })
    .flatten();
    Ok(Sse::new(first.chain(outcome).boxed()))
}

/// Where the waiting page goes after approval: sets the login cookie.
#[route(GET "./{claim}")]
async fn pick_up(cx: &Cx) -> Result<Response> {
    let claim: &str = path_param::<Claim>(cx);
    let Some(next) = app_context::<SignIns>(cx).claim(claim) else {
        return Ok(Response::builder()
            .status(StatusCode::SEE_OTHER)
            .header(header::LOCATION, "/login")
            .body(Body::empty())?);
    };
    auth::start_session(cx);
    Ok(Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, next)
        .body(Body::empty())?)
}

fn random_hex() -> std::result::Result<String, getrandom::Error> {
    let mut bytes = [0u8; 24];
    getrandom::fill(&mut bytes)?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

fn random_code() -> std::result::Result<String, getrandom::Error> {
    let mut bytes = [0u8; CODE_LEN];
    getrandom::fill(&mut bytes)?;
    // 256 is a multiple of 32, so every character is equally likely.
    Ok(bytes.iter().map(|b| CODE_ALPHABET[usize::from(b % 32)] as char).collect())
}

/// `K7F29Q` → `K7F-29Q`, easier to read and type.
pub fn display_code(code: &str) -> String {
    let (left, right) = code.split_at(code.len() / 2);
    format!("{left}-{right}")
}

/// Accepts lower case, spaces and dashes.
fn normalize_code(typed: &str) -> String {
    typed.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_uppercase()).collect()
}

/// A short description of a browser, such as "Chrome on Windows", from its
/// User-Agent, so the approving device can tell what it lets in.
pub fn describe_device(user_agent: &str) -> String {
    let browser = if user_agent.contains("Edg/") {
        "Edge"
    } else if user_agent.contains("OPR/") {
        "Opera"
    } else if user_agent.contains("SamsungBrowser/") {
        "Samsung Internet"
    } else if user_agent.contains("Firefox/") {
        "Firefox"
    } else if user_agent.contains("Chrome/") || user_agent.contains("CriOS/") {
        "Chrome"
    } else if user_agent.contains("Safari/") {
        "Safari"
    } else {
        "A browser"
    };
    let os = if user_agent.contains("Android") {
        Some("Android")
    } else if user_agent.contains("iPhone") || user_agent.contains("iPad") {
        Some("iOS")
    } else if user_agent.contains("Windows") {
        Some("Windows")
    } else if user_agent.contains("CrOS") {
        Some("ChromeOS")
    } else if user_agent.contains("Mac OS X") {
        Some("macOS")
    } else if user_agent.contains("Linux") {
        Some("Linux")
    } else {
        None
    };
    match os {
        Some(os) => format!("{browser} on {os}"),
        None => browser.to_owned(),
    }
}

/// Encodes everything except unreserved characters and `/`, so the result
/// is safe inside a URL and inside a quoted script string.
pub fn percent_encode(text: &str) -> String {
    text.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Decodes `%XX`; `None` for malformed input or control characters, which
/// must not end up in a redirect.
fn percent_decode(text: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(text.len());
    let mut input = text.bytes();
    while let Some(b) = input.next() {
        if b == b'%' {
            let hex = [input.next()?, input.next()?];
            bytes.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            bytes.push(b);
        }
    }
    let decoded = String::from_utf8(bytes).ok()?;
    (!decoded.chars().any(char::is_control)).then_some(decoded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_login_can_be_claimed_once() {
        let sign_ins = SignIns::default();
        let (waiting, mut rx) = sign_ins.create(None, "Chrome on Linux".into(), "/send".into()).unwrap().unwrap();
        assert_eq!(sign_ins.by_code(&display_code(&waiting.code).to_lowercase()).unwrap().token, waiting.token);
        assert!(sign_ins.decide(&waiting.token, Decision::Approved).unwrap());
        let claim = rx.try_recv().unwrap().unwrap();
        assert_ne!(claim, waiting.token);
        assert!(sign_ins.by_token(&waiting.token).is_none());
        assert_eq!(sign_ins.claim(&claim).as_deref(), Some("/send"));
        assert!(sign_ins.claim(&claim).is_none());
    }

    #[test]
    fn approving_after_the_page_left_fails() {
        let sign_ins = SignIns::default();
        let (waiting, rx) = sign_ins.create(None, "Firefox".into(), "/".into()).unwrap().unwrap();
        drop(rx);
        assert!(sign_ins.by_token(&waiting.token).is_none());
        assert!(!sign_ins.decide(&waiting.token, Decision::Approved).unwrap());
        assert!(sign_ins.lock().claims.is_empty());
    }

    #[test]
    fn declining_sends_none() {
        let sign_ins = SignIns::default();
        let (waiting, mut rx) = sign_ins.create(None, "Firefox".into(), "/".into()).unwrap().unwrap();
        assert!(sign_ins.decide(&waiting.token, Decision::Declined).unwrap());
        assert_eq!(rx.try_recv().unwrap(), None);
    }

    #[test]
    fn pending_requests_are_capped_in_total_and_per_address() {
        let sign_ins = SignIns::default();
        let ip = |n: u8| Some(IpAddr::from([203, 0, 113, n]));
        let mut receivers: Vec<_> = (0..MAX_PER_ADDRESS)
            .map(|_| sign_ins.create(ip(1), "x".into(), "/".into()).unwrap().unwrap().1)
            .collect();
        assert!(sign_ins.create(ip(1), "x".into(), "/".into()).unwrap().is_none());
        for n in 0..u8::try_from(MAX_PENDING - MAX_PER_ADDRESS).unwrap() {
            receivers.push(sign_ins.create(ip(100 + n), "x".into(), "/".into()).unwrap().unwrap().1);
        }
        assert!(sign_ins.create(ip(2), "x".into(), "/".into()).unwrap().is_none());
        // Closing a waiting page frees its slot.
        receivers.pop();
        assert!(sign_ins.create(ip(2), "x".into(), "/".into()).unwrap().is_some());
    }

    #[test]
    fn describes_common_browsers() {
        let chrome_windows = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
        assert_eq!(describe_device(chrome_windows), "Chrome on Windows");
        let firefox_linux = "Mozilla/5.0 (X11; Linux x86_64; rv:132.0) Gecko/20100101 Firefox/132.0";
        assert_eq!(describe_device(firefox_linux), "Firefox on Linux");
        let edge = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36 Edg/130.0.0.0";
        assert_eq!(describe_device(edge), "Edge on Windows");
        let android = "Mozilla/5.0 (Linux; Android 14; K) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Mobile Safari/537.36";
        assert_eq!(describe_device(android), "Chrome on Android");
        let safari_mac = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/18.0 Safari/605.1.15";
        assert_eq!(describe_device(safari_mac), "Safari on macOS");
        assert_eq!(describe_device("curl/8.0"), "A browser");
    }

    #[test]
    fn next_round_trips_and_rejects_control_characters() {
        let next = "/send?x='a b'";
        assert_eq!(percent_decode(&percent_encode(next)).as_deref(), Some(next));
        assert!(!percent_encode(next).contains('\''));
        assert_eq!(percent_decode("/a%0D%0Ab"), None);
        assert_eq!(percent_decode("/a%2"), None);
    }
}
