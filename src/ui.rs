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

fn expires(drop: &Drop, now: u64) -> String {
    if drop.is_once() {
        return "deleted after first use".to_owned();
    }
    match drop.meta.expires_at {
        None => "kept until deleted".to_owned(),
        Some(at) => format!("expires in {}", Duration(at.saturating_sub(now))),
    }
}

/// Everything Datastar replaces on a change, matched by its id.
#[component]
pub async fn drop_list(drops: Vec<Drop>, total_bytes: u64, max_total_bytes: u64) -> Result<impl View> {
    let now = unix_now();
    let usage = format!("{} of {} used", Bytes(total_bytes), Bytes(max_total_bytes));

    Ok(view! {
        <section id="drops">
            if drops.is_empty() {
                <p class="empty">"Nothing here yet. Share something!"</p>
            } else {
                <div class="toolbar">
                    <span class="muted">(usage)</span>
                    <button class="quiet" data-on:click="confirm('Delete all drops?') && @delete('/drops')">
                        "Clear all"
                    </button>
                </div>
            }
            <ul>
                for drop in drops {
                    <li class="drop" if drop.is_once() { data-once=(drop.id.clone()) }>
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
                            (Kind::Link, _) => {
                                <a class="content" href=(drop.text.trim().to_owned()) target="_blank" rel="noopener noreferrer">(drop.text.clone())</a>
                            }
                            _ => {
                                <div class="content">(drop.text.clone())</div>
                            }
                        }
                        <div class="meta">
                            <span>(age(drop.meta.created_at, now))</span>
                            <span>(expires(&drop, now))</span>
                            <span class="actions">
                                if drop.meta.kind == Kind::File {
                                    <a class="button" href=(format!("/drops/{}/file", drop.id))>"Download"</a>
                                } else {
                                    <button class="quiet" data-copy=(drop.text.clone())>"Copy"</button>
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
