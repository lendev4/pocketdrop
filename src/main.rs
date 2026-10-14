mod auth;
mod drops;
mod events;
mod healthz;
mod login;
mod logout;
mod send;
mod share;

use topcoat::{
    Result,
    context::Cx,
    cookie::RouterBuilderCookieExt,
    router::{RouterBuilderDirectoryExt, module_router, page},
    view::{Child, View, component, view},
};

use crate::{auth::Auth, drops::Drops};

#[tokio::main]
async fn main() {
    let auth = Auth::from_env();

    let router = module_router!()
        .cookies()
        .app_context(auth.key.clone())
        .app_context(auth)
        .app_context(Drops::default())
        .public_dir("public")
        .build();

    topcoat::start(router).await.unwrap();
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
                <button>"Drop it"</button>
            </form>

            // Opens the live stream; every change replaces #drops.
            <div data-init="@get('/events')"></div>
            drops::drop_list(drops: drops::drops(cx).list())
        )
    })
}
