mod api;
mod assets;
mod auth;
mod config;
mod drops;
mod events;
mod healthz;
mod ingest;
mod login;
mod logout;
mod pair;
mod send;
mod share;
mod store;
mod ui;

use std::{path::Path, sync::Arc, time::Duration};

use http::{HeaderValue, header};
use tower_http::set_header::SetResponseHeaderLayer;

use topcoat::{
    Result,
    context::{Cx, app_context},
    cookie::RouterBuilderCookieExt,
    router::{BodyLimit, RouterBuilderDirectoryExt, module_router, page, tower::TowerLayer},
    view::{Child, View, component, view},
};

use crate::{auth::Auth, config::Config, pair::Pairing, store::Store};

#[tokio::main]
async fn main() {
    let auth = Auth::from_env();
    let config = Config::from_env();
    let store = Arc::new(Store::open(&config).await.expect("cannot open data directory"));
    assets::init(Path::new("public"));

    // Expired drops vanish from open pages when this deletes them.
    let cleaner = Arc::clone(&store);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            if cleaner.cleanup().await {
                cleaner.notify();
            }
        }
    });

    // One file per request, plus room for the other form fields.
    let upload_limit = usize::try_from(config.max_file_bytes).unwrap_or(usize::MAX)
        .saturating_add(config.max_text_bytes)
        .saturating_add(64 * 1024);

    // Browsers must check back before reusing a cached response; unchanged
    // files then cost a 304. Downloads set their own Cache-Control.
    let revalidate = SetResponseHeaderLayer::if_not_present(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));

    let router = module_router!()
        .layer(TowerLayer::new(revalidate))
        .cookies()
        .layer(BodyLimit::max(upload_limit).at("/drops"))
        .layer(BodyLimit::max(upload_limit).at("/share"))
        .layer(BodyLimit::max(upload_limit).at("/api"))
        .app_context(auth.key.clone())
        .app_context(auth)
        .app_context(config)
        .app_context(store)
        .app_context(Pairing::default())
        .public_dir("public")
        .build();

    topcoat::start(router).await.unwrap();
}

pub fn store(cx: &Cx) -> &Store {
    app_context::<Arc<Store>>(cx)
}

pub fn config(cx: &Cx) -> &Config {
    app_context::<Config>(cx)
}

/// Full HTML document around a page's content.
#[component]
pub async fn shell(title: &str, #[default] child: Child<'_>) -> Result<impl View> {
    Ok(view! {
        <!DOCTYPE html>
        <html lang="en">
            <head>
                <meta charset="utf-8">
                <meta name="viewport" content="width=device-width, initial-scale=1">
                // System bars match the page background in both themes.
                <meta name="theme-color" content="#f6f6f9" media="(prefers-color-scheme: light)">
                <meta name="theme-color" content="#131218" media="(prefers-color-scheme: dark)">
                <title>(title)</title>
                <link rel="manifest" href=(assets::url("/manifest.json"))>
                <link rel="icon" href="/icons/icon-192.png">
                <link rel="stylesheet" href=(assets::url("/app.css"))>
                <script type="module" src="/vendor/datastar-1.0.2.js"></script>
                <script src=(assets::url("/share-store.js")) defer="true"></script>
                <script src=(assets::url("/upload.js")) defer="true"></script>
                <script src=(assets::url("/app.js")) defer="true"></script>
                topcoat::dev::script()
            </head>
            <body>
                <main>(child)</main>
            </body>
        </html>
    })
}

/// The "keep for" choice, preselecting the configured default.
#[component]
pub async fn expiry_select(cx: &Cx, id: &str) -> Result<impl View> {
    let config = config(cx);
    let options: Vec<(String, String, bool)> = config
        .expiry_options
        .iter()
        .map(|&expiry| (expiry.to_string(), expiry.describe(), expiry == config.default_expiry))
        .collect();

    Ok(view! {
        <label class="expiry">
            "Keep for "
            <select name="expiry" id=(id)>
                for (value, label, selected) in options {
                    <option value=(value) if selected { selected="true" }>(label)</option>
                }
            </select>
        </label>
    })
}

#[page]
async fn home(cx: &Cx) -> Result<impl View> {
    auth::require(cx)?;
    let config = config(cx);
    let max_file = config.max_file_bytes.to_string();

    Ok(view! {
        shell(title: "PocketDrop",
            <header>
                <h1>"PocketDrop"</h1>
                <details class="menu">
                    <summary aria-label="Menu">"⋯"</summary>
                    <div class="menu-panel">
                        ui::usage(total_bytes: store(cx).total_bytes(), max_total_bytes: config.max_total_bytes)
                        <button class="quiet" data-on:click="@post('/pair')" data-close-menu="true">"Pair a device"</button>
                        <form method="POST" action="/logout">
                            <button class="quiet">"Log out"</button>
                        </form>
                    </div>
                </details>
            </header>
            <div id="pair"></div>

            <p id="pending-share" class="banner" hidden="true">
                "A share from your phone hasn't been sent yet. "
                <a href="/send">"Review and send"</a>
            </p>

            // Works as a plain form; upload.js adds progress, drag-and-drop and paste.
            <form id="send-form" class="send" method="POST" action="/drops" enctype="multipart/form-data" data-max-file=(max_file) data-pairdrop=(config.pairdrop_url.clone())>
                <textarea name="text" rows="1" placeholder="Text or link to drop…"></textarea>
                <div class="send-row">
                    <label class="button quiet file-pick">
                        "Add files"
                        <input type="file" name="files" multiple="true">
                    </label>
                    expiry_select(id: "expiry")
                    <button class="send-button">"Drop it"</button>
                </div>
            </form>
            <ul id="uploads"></ul>

            // Opens the live stream; every change replaces #drops.
            <div data-init="@get('/events')"></div>
            ui::drop_list(drops: store(cx).list(), pairdrop_url: config.pairdrop_url.clone())
        )
    })
}
