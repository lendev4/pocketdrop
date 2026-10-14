use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::{View, view},
};

use crate::{auth, config, expiry_select, shell};

/// Send screen for Android shares. The service worker keeps the shared
/// content on the phone and redirects here; send.js shows it and uploads it.
#[page]
async fn page(cx: &Cx) -> Result<impl View> {
    auth::require(cx)?;
    let config = config(cx);
    let max_file = config.max_file_bytes.to_string();

    Ok(view! {
        shell(title: "Send · PocketDrop",
            <section id="share" class="send" data-max-file=(max_file) data-pairdrop=(config.pairdrop_url.clone())>
                <h1>"Send to PocketDrop"</h1>
                <div id="share-preview"><p class="muted">"Loading the shared content…"</p></div>
                <ul id="uploads"></ul>
                <p id="share-status" class="notice-inline"></p>
                <div class="share-controls">
                    expiry_select(id: "share-expiry")
                    <label class="check">
                        <input type="checkbox" id="share-auto">
                        " Send right away next time"
                    </label>
                    <button id="share-send" class="send-button wide" disabled="true">"Send"</button>
                    <a class="share-home" href="/">"Open PocketDrop"</a>
                </div>
            </section>
            <script src="/send.js" defer="true"></script>
        )
    })
}
