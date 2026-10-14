//! Single-user login: one passphrase, then a signed cookie holding the time
//! it was issued. Nothing is stored server-side.

use std::{
    env,
    time::{SystemTime, UNIX_EPOCH},
};

use topcoat::{
    Result,
    context::{Cx, app_context},
    cookie::{Cookie, Cookies, Key, SameSite, signed_cookies, time::Duration},
    router::error::redirect,
};

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

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn is_logged_in(cx: &Cx) -> bool {
    let auth = app_context::<Auth>(cx);
    signed_cookies(cx)
        .get(COOKIE)
        .and_then(|cookie| cookie.value().parse::<u64>().ok())
        .is_some_and(|issued| now().saturating_sub(issued) < auth.session_days * 86_400)
}

/// Redirects to the login page unless the request is logged in.
pub fn require(cx: &Cx) -> Result<()> {
    if is_logged_in(cx) {
        Ok(())
    } else {
        Err(redirect("/login").into())
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
        Cookie::build((COOKIE, now().to_string()))
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
