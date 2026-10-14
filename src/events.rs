use futures_util::{Stream, stream};
use tokio::sync::broadcast::{self, error::RecvError};
use topcoat::{
    Result,
    context::Cx,
    datastar::PatchElements,
    router::{
        content::sse::{Event, KeepAlive, Sse},
        error::unauthorized,
        route,
    },
    view::{ViewExt, view},
};

use crate::{auth, config, store, ui};

/// Live stream for the main page: sends the full list on connect, then again
/// after every change. Reconnecting therefore always catches up.
#[route(GET)]
async fn events(cx: &Cx) -> Result<Sse<impl Stream<Item = Result<Event>> + use<>>> {
    if !auth::is_logged_in(cx) {
        return Err(unauthorized().into());
    }

    let rx = store(cx).subscribe();
    let cx = cx.clone();

    let events = stream::unfold((rx, cx, true), |(mut rx, cx, first)| async move {
        if !first && !wait_for_change(&mut rx).await {
            return None;
        }
        let event = render_list(&cx).await;
        Some((event, (rx, cx, false)))
    });

    Ok(Sse::new(events).keep_alive(KeepAlive::new()))
}

/// Returns false once the channel is closed.
async fn wait_for_change(rx: &mut broadcast::Receiver<()>) -> bool {
    match rx.recv().await {
        // Lagging only means several changes happened; one re-render covers them.
        Ok(()) | Err(RecvError::Lagged(_)) => true,
        Err(RecvError::Closed) => false,
    }
}

async fn render_list(cx: &Cx) -> Result<Event> {
    let store = store(cx);
    let (list, total_bytes) = (store.list(), store.total_bytes());
    let config = config(cx);
    let (max_total_bytes, pairdrop_url) = (config.max_total_bytes, config.pairdrop_url.clone());
    let html = view! { cx =>
        ui::drop_list(drops: list, total_bytes: total_bytes, max_total_bytes: max_total_bytes, pairdrop_url: pairdrop_url)
    }
        .single()
        .await?;
    Ok(PatchElements::new(html.render(cx)).into())
}
