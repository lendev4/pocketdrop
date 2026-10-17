use topcoat::{
    Result,
    context::Cx,
    router::page,
    view::{View, view},
};

use crate::{assets, auth, shell};

/// Target of the desktop "Open with PocketDrop" file handler. open.js takes
/// the files from the launch and continues on the send screen.
#[page]
async fn page(cx: &Cx) -> Result<impl View> {
    auth::require(cx)?;
    Ok(view! {
        shell(title: "Open · PocketDrop",
            <div class="notice">
                <p id="open-status" class="muted">"Opening files…"</p>
                <a href="/">"Open PocketDrop"</a>
            </div>
            <script src=(assets::url("/open.js")) defer="true"></script>
        )
    })
}
