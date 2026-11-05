//! Single-user login: one passphrase, then a signed cookie holding the time
//! it was issued. The only server-side state is one time in
//! `<DATA_DIR>/logged-out-before`: cookies issued before it are rejected
//! ("Log out other devices").

use std::{
    collections::HashMap,
    env,
    io,
    net::IpAddr,
    path::{Path, PathBuf},
    sync::{
        Mutex, PoisonError,
        atomic::{AtomicU64, Ordering},
    },
};

use topcoat::{
    Result,
    context::{Cx, app_context},
    cookie::{Cookie, Cookies, Key, SameSite, signed_cookies, time::Duration},
    router::{
        error::{forbidden, not_found, redirect, too_many_requests, unauthorized},
        request::{client_ip, headers, uri},
    },
};

use crate::store::unix_now;

const COOKIE: &str = "pd_session";
/// At most one cookie re-issue per device and day.
const REFRESH_SECS: u64 = 86_400;

pub struct Auth {
    pub key: Key,
    passphrase: String,
    session_days: u64,
    /// Enables the `/api` routes when set.
    api_token: Option<String>,
    limiter: Limiter,
    /// Cookies issued at or before this Unix time are rejected.
    logged_out_before: AtomicU64,
    logged_out_file: PathBuf,
}

/// Failed passphrase or API token attempts allowed per address and window.
const MAX_FAILURES: u32 = 5;
const WINDOW_SECS: u64 = 60;

/// Counts failed logins per client address, so a weak passphrase can't be
/// guessed quickly. In memory only; a restart forgets the counts.
#[derive(Default)]
struct Limiter {
    failures: Mutex<HashMap<Option<IpAddr>, (u64, u32)>>,
}

impl Limiter {
    fn is_blocked(&self, ip: Option<IpAddr>, now: u64) -> bool {
        self.lock()
            .get(&ip)
            .is_some_and(|&(start, count)| now < start + WINDOW_SECS && count >= MAX_FAILURES)
    }

    fn record_failure(&self, ip: Option<IpAddr>, now: u64) {
        let mut failures = self.lock();
        failures.retain(|_, &mut (start, _)| now < start + WINDOW_SECS);
        let entry = failures.entry(ip).or_insert((now, 0));
        entry.1 += 1;
    }

    fn clear(&self, ip: Option<IpAddr>) {
        self.lock().remove(&ip);
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<Option<IpAddr>, (u64, u32)>> {
        self.failures.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

pub enum LoginResult {
    LoggedIn,
    WrongPassphrase,
    TooManyAttempts,
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

        let api_token = env::var("API_TOKEN").ok().filter(|token| !token.is_empty());

        Self {
            key,
            passphrase,
            session_days,
            api_token,
            limiter: Limiter::default(),
            logged_out_before: AtomicU64::new(0),
            logged_out_file: PathBuf::new(),
        }
    }

    /// Reads the "logged out before" time kept in the data directory.
    pub fn load_logged_out_before(&mut self, data_dir: &Path) {
        self.logged_out_file = data_dir.join("logged-out-before");
        let before = std::fs::read_to_string(&self.logged_out_file)
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(0);
        self.logged_out_before = AtomicU64::new(before);
    }
}

/// Checks the login cookie. Logins roll: a valid cookie older than
/// [`REFRESH_SECS`] is re-issued, so a device stays logged in while it is
/// used and expires after `SESSION_DAYS` without use.
pub fn is_logged_in(cx: &Cx) -> bool {
    let auth = app_context::<Auth>(cx);
    let Some(issued) = signed_cookies(cx).get(COOKIE).and_then(|cookie| cookie.value().parse::<u64>().ok()) else {
        return false;
    };
    if issued <= auth.logged_out_before.load(Ordering::Relaxed) {
        return false;
    }
    let age = unix_now().saturating_sub(issued);
    if age >= auth.session_days * 86_400 {
        return false;
    }
    if age >= REFRESH_SECS {
        start_session(cx);
    }
    true
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

/// For `/api`: requires `Authorization: Bearer <API_TOKEN>`. Without a
/// configured token the API does not exist.
pub fn require_token(cx: &Cx) -> Result<()> {
    let auth = app_context::<Auth>(cx);
    let Some(expected) = &auth.api_token else {
        return Err(not_found().into());
    };
    let (ip, now) = (client_ip(cx), unix_now());
    if auth.limiter.is_blocked(ip, now) {
        return Err(too_many_requests(WINDOW_SECS).into());
    }
    let sent = headers(cx)
        .get("authorization")
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    if constant_time_eq(sent.as_bytes(), expected.as_bytes()) {
        Ok(())
    } else {
        auth.limiter.record_failure(ip, now);
        Err(unauthorized().into())
    }
}

/// Checks the passphrase and, if it matches, sets the session cookie.
pub fn log_in(cx: &Cx, passphrase: &str) -> LoginResult {
    let auth = app_context::<Auth>(cx);
    let (ip, now) = (client_ip(cx), unix_now());
    if auth.limiter.is_blocked(ip, now) {
        return LoginResult::TooManyAttempts;
    }
    if !constant_time_eq(passphrase.as_bytes(), auth.passphrase.as_bytes()) {
        auth.limiter.record_failure(ip, now);
        return LoginResult::WrongPassphrase;
    }
    auth.limiter.clear(ip);
    start_session(cx);
    LoginResult::LoggedIn
}

/// Sets the session cookie on this device.
pub fn start_session(cx: &Cx) {
    issue_cookie(cx, unix_now());
}

fn issue_cookie(cx: &Cx, issued: u64) {
    let auth = app_context::<Auth>(cx);
    let days = i64::try_from(auth.session_days).unwrap_or(365);
    signed_cookies(cx).add(
        Cookie::build((COOKIE, issued.to_string()))
            .path("/")
            .http_only(true)
            .secure(true)
            .same_site(SameSite::Lax)
            .max_age(Duration::days(days))
            .build(),
    );
}

/// Logs out every device except this one: cookies issued up to now stop
/// working, and this device gets one dated a second later. Saved first, so
/// a failed write changes nothing.
pub async fn log_out_others(cx: &Cx) -> io::Result<()> {
    let auth = app_context::<Auth>(cx);
    let now = unix_now();
    let staging = auth.logged_out_file.with_extension("tmp");
    tokio::fs::write(&staging, now.to_string()).await?;
    tokio::fs::rename(&staging, &auth.logged_out_file).await?;
    auth.logged_out_before.fetch_max(now, Ordering::Relaxed);
    issue_cookie(cx, now + 1);
    Ok(())
}

pub fn log_out(cx: &Cx) {
    signed_cookies(cx).remove(Cookie::build((COOKIE, "")).path("/").build());
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |diff, (x, y)| diff | (x ^ y)) == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limiter_blocks_after_max_failures_within_window() {
        let limiter = Limiter::default();
        let ip = Some(IpAddr::from([203, 0, 113, 7]));
        for _ in 0..MAX_FAILURES {
            assert!(!limiter.is_blocked(ip, 1000));
            limiter.record_failure(ip, 1000);
        }
        assert!(limiter.is_blocked(ip, 1000));
        // Other addresses are unaffected; the block ends with the window.
        assert!(!limiter.is_blocked(Some(IpAddr::from([203, 0, 113, 8])), 1000));
        assert!(!limiter.is_blocked(ip, 1000 + WINDOW_SECS));
    }

    #[test]
    fn success_clears_failures() {
        let limiter = Limiter::default();
        let ip = Some(IpAddr::from([203, 0, 113, 7]));
        for _ in 0..MAX_FAILURES - 1 {
            limiter.record_failure(ip, 1000);
        }
        limiter.clear(ip);
        limiter.record_failure(ip, 1000);
        assert!(!limiter.is_blocked(ip, 1000));
    }
}
