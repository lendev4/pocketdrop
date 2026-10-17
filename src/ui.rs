use topcoat::{
    Result,
    view::{View, component, view},
};

use crate::{
    config::{Bytes, Duration},
    drops::is_previewable,
    store::{Drop, Kind, unix_now},
};

fn age(created_at: u64, now: u64) -> String {
    match now.saturating_sub(created_at) {
        0..60 => "just now".to_owned(),
        secs => format!("{} ago", Duration(secs)),
    }
}

/// Text for drops whose expiry is not a countdown; `None` means show one.
fn fixed_expiry(drop: &Drop) -> Option<&'static str> {
    if drop.meta.kind == Kind::Handoff {
        Some("gone once opened")
    } else if drop.is_once() {
        Some("deleted after first use")
    } else if drop.meta.expires_at.is_none() {
        Some("kept until deleted")
    } else {
        None
    }
}

/// Storage line for the header menu; patched alongside the list.
#[component]
pub async fn usage(total_bytes: u64, max_total_bytes: u64) -> Result<impl View> {
    Ok(view! {
        <p id="usage" class="muted">(format!("{} of {} used", Bytes(total_bytes), Bytes(max_total_bytes)))</p>
    })
}

/// Everything Datastar replaces on a change, matched by its id. Times carry
/// their timestamps so app.js can keep "5 min ago" current between changes.
#[component]
pub async fn drop_list(drops: Vec<Drop>, pairdrop_url: String) -> Result<impl View> {
    let now = unix_now();

    Ok(view! {
        <section id="drops">
            if drops.is_empty() {
                <div class="empty">
                    <p class="empty-title">"Nothing here yet"</p>
                    <ul>
                        <li>"On your phone: Share → PocketDrop from any app"</li>
                        <li>"Here: type above, paste with Ctrl+V or drag files onto the page"</li>
                    </ul>
                </div>
            } else {
                <div class="toolbar">
                    <button class="quiet small" data-clear-all="true">"Clear all"</button>
                </div>
            }
            <ul>
                for drop in drops {
                    <li class="drop" id=(format!("drop-{}", drop.id)) if drop.is_once() { data-once=(drop.id.clone()) }>
                        match (&drop.meta.kind, &drop.meta.file) {
                            (Kind::File, Some(file)) => {
                                // A preview would count as the one download.
                                if is_previewable(&file.mime) && !drop.is_once() {
                                    <a href=(format!("/drops/{}/file", drop.id))>
                                        <img class="preview" src=(format!("/drops/{}/file?inline", drop.id)) alt=(file.name.clone()) loading="lazy">
                                    </a>
                                }
                                <div class="file-row">
                                    <span class="file-name">(file.name.clone())</span>
                                    <span class="muted">(Bytes(file.size).to_string())</span>
                                </div>
                            }
                            (Kind::Handoff, Some(file)) => {
                                <p class="handoff-label">"Incoming via PairDrop"</p>
                                <div class="file-row">
                                    <span class="file-name">(file.name.clone())</span>
                                    <span class="muted">(Bytes(file.size).to_string())</span>
                                </div>
                            }
                            // Long content is clamped; app.js reveals "Show more" when needed.
                            (Kind::Link, _) => {
                                <a class="content clamp link" href=(drop.text.trim().to_owned()) target="_blank" rel="noopener noreferrer">(drop.text.clone())</a>
                                <button class="more" data-more="true" hidden="true">"Show more"</button>
                            }
                            _ => {
                                <div class="content clamp">(drop.text.clone())</div>
                                <button class="more" data-more="true" hidden="true">"Show more"</button>
                            }
                        }
                        <div class="meta">
                            <span data-age=(drop.meta.created_at.to_string())>(age(drop.meta.created_at, now))</span>
                            match (fixed_expiry(&drop), drop.meta.expires_at) {
                                (Some(text), _) => { <span>(text)</span> }
                                (None, Some(at)) => {
                                    <span data-expires=(at.to_string())>(format!("expires in {}", Duration(at.saturating_sub(now))))</span>
                                }
                                (None, None) => {}
                            }
                            <span class="actions">
                                match (&drop.meta.kind, &drop.meta.file) {
                                    (Kind::File, Some(file)) => {
                                        if is_previewable(&file.mime) {
                                            <button class="quiet" data-copy-image=(format!("/drops/{}/file?inline", drop.id))>"Copy"</button>
                                        }
                                        <a class="button" href=(format!("/drops/{}/file", drop.id))>"Download"</a>
                                    }
                                    (Kind::Handoff, _) => {
                                        <a class="button" href=(pairdrop_url.clone()) target="_blank" rel="noopener noreferrer" data-consume="true">"Open PairDrop"</a>
                                    }
                                    _ => {
                                        <button class="quiet" data-copy=(drop.text.clone())>"Copy"</button>
                                    }
                                }
                                <button class="quiet" data-on:click=(format!("@delete('/drops/{}')", drop.id))>"Delete"</button>
                            </span>
                        </div>
                    </li>
                }
            </ul>
        </section>
    })
}
