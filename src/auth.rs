//! Single-user login: one passphrase, then a signed cookie holding the time
//! it was issued. Nothing is stored server-side.

use std::env;

use topcoat::{
    Result,
    context::{Cx, app_context},
    cookie::{Cookie, Cookies, Key, SameSite, signed_cookies, time::Duration},
    router::{
        error::{forbidden, redirect, unauthorized},
        request::{headers, uri},
    },
};

use crate::store::unix_now;

const COOKIE: &str = "pd_session";

pub struct Auth {
    pub key: Key,
    passphrase: String,
    session_days: u64,
}

impl Auth {
    pub fn from_env() -> Self {
        let passphrase = env::var("PASSPHRASE").expect("PASSPHRASE must be set");
        assert!(!passphrase.is_empty(), "PASSPHRASE must not be empty");

        let key = match env::var("SESSION_SECRET") {
            Ok(secret) => Key::try_from(secret.as_bytes())
                .expect("SESSION_SECRET must be at least 64 bytes"),
            Err(_) => {
                eprintln!("SESSION_SECRET not set: using a random key, sessions end on restart");
                Key::generate()
            }
        };

        let session_days = env::var("SESSION_DAYS")
            .ok()
            .and_then(|days| days.parse().ok())
            .unwrap_or(365);

        Self { key, passphrase, session_days }
    }
}

pub fn is_logged_in(cx: &Cx) -> bool {
    let auth = app_context::<Auth>(cx);
    signed_cookies(cx)
        .get(COOKIE)
        .and_then(|cookie| cookie.value().parse::<u64>().ok())
        .is_some_and(|issued| unix_now().saturating_sub(issued) < auth.session_days * 86_400)
}

/// Redirects to the login page unless the request is logged in; after
/// logging in, the user comes back to this page.
pub fn require(cx: &Cx) -> Result<()> {
    if is_logged_in(cx) {
        return Ok(());
    }
    match uri(cx).path() {
        "/" => Err(redirect("/login").into()),
        path => Err(redirect(format!("/login?next={path}")).into()),
    }
}

/// A local path to return to after login; anything else falls back to `/`.
pub fn safe_next(next: Option<&str>) -> &str {
    match next {
        Some(path) if path.starts_with('/') && !path.starts_with("//") && !path.contains('\\') => path,
        _ => "/",
    }
}

/// For requests made by page scripts: rejects instead of redirecting.
pub fn require_api(cx: &Cx) -> Result<()> {
    if !is_logged_in(cx) {
        return Err(unauthorized().into());
    }
    require_same_origin(cx)
}

/// Rejects state-changing requests sent from another site. Browsers set
/// `Origin` on such requests; a missing header is left to `SameSite`.
pub fn require_same_origin(cx: &Cx) -> Result<()> {
    let headers = headers(cx);
    let Some(origin) = headers.get("origin").and_then(|value| value.to_str().ok()) else {
        return Ok(());
    };
    let host = headers
        .get("x-forwarded-host")
        .or_else(|| headers.get("host"))
        .and_then(|value| value.to_str().ok());
    let origin_host = origin.split_once("://").map(|(_, host)| host);

    match (origin_host, host) {
        (Some(origin_host), Some(host)) if origin_host == host => Ok(()),
        _ => Err(forbidden().into()),
    }
}

/// Checks the passphrase and, if it matches, sets the session cookie.
pub fn log_in(cx: &Cx, passphrase: &str) -> bool {
    let auth = app_context::<Auth>(cx);
    if !constant_time_eq(passphrase.as_bytes(), auth.passphrase.as_bytes()) {
        return false;
    }

    let days = i64::try_from(auth.session_days).unwrap_or(365);
    signed_cookies(cx).add(
        Cookie::build((COOKIE, unix_now().to_string()))
            .path("/")
            .http_only(true)
            .secure(true)
            .same_site(SameSite::Lax)
            .max_age(Duration::days(days))
            .build(),
    );
    true
}

pub fn log_out(cx: &Cx) {
    signed_cookies(cx).remove(Cookie::build((COOKIE, "")).path("/").build());
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}
