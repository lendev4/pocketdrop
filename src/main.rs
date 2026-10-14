mod auth;
mod config;
mod drops;
mod events;
mod healthz;
mod login;
mod logout;
mod send;
mod share;
mod store;
mod ui;

use std::{sync::Arc, time::Duration};

use topcoat::{
    Result,
    context::{Cx, app_context},
    cookie::RouterBuilderCookieExt,
    router::{RouterBuilderDirectoryExt, module_router, page},
    view::{Child, View, component, view},
};

use crate::{auth::Auth, config::Config, store::Store};

#[tokio::main]
async fn main() {
    let auth = Auth::from_env();
    let config = Config::from_env();
    let store = Arc::new(Store::open(&config).await.expect("cannot open data directory"));

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

    let router = module_router!()
        .cookies()
        .app_context(auth.key.clone())
        .app_context(auth)
        .app_context(config)
        .app_context(store)
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
                <meta name="theme-color" content="#4f46e5">
                <title>(title)</title>
                <link rel="manifest" href="/manifest.json">
                <link rel="icon" href="/icons/icon-192.png">
                <link rel="stylesheet" href="/app.css">
                <script type="module" src="/vendor/datastar-1.0.2.js"></script>
                <script src="/app.js" defer="true"></script>
                topcoat::dev::script()
            </head>
            <body>
                <main>(child)</main>
            </body>
        </html>
    })
}

#[page]
async fn home(cx: &Cx) -> Result<impl View> {
    auth::require(cx)?;
    let config = config(cx);
    let options: Vec<(String, String, bool)> = config
        .expiry_options
        .iter()
        .map(|&expiry| (expiry.to_string(), expiry.describe(), expiry == config.default_expiry))
        .collect();

    Ok(view! {
        shell(title: "PocketDrop",
            <header>
                <h1>"PocketDrop"</h1>
                <form method="POST" action="/logout">
                    <button class="quiet">"Log out"</button>
                </form>
            </header>

            <form class="send" method="POST" action="/send">
                <textarea name="text" placeholder="Text or link to drop…" required="true"></textarea>
                <div class="send-row">
                    <label>
                        "Keep for "
                        <select name="expiry">
                            for (value, label, selected) in options {
                                <option value=(value) if selected { selected="true" }>(label)</option>
                            }
                        </select>
                    </label>
                    <button>"Drop it"</button>
                </div>
            </form>

            // Opens the live stream; every change replaces #drops.
            <div data-init="@get('/events')"></div>
            ui::drop_list(drops: store(cx).list())
        )
    })
}
