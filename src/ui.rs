use topcoat::{
    Result,
    view::{View, component, view},
};

use crate::{
    config::Duration,
    store::{Drop, Kind, unix_now},
};

fn age(created_at: u64, now: u64) -> String {
    match now.saturating_sub(created_at) {
        0..60 => "just now".to_owned(),
        secs => format!("{} ago", Duration(secs)),
    }
}

fn expires(drop: &Drop, now: u64) -> String {
    match drop.meta.expires_at {
        None => "kept until deleted".to_owned(),
        Some(at) => format!("expires in {}", Duration(at.saturating_sub(now))),
    }
}

/// Everything Datastar replaces on a change, matched by its id.
#[component]
pub async fn drop_list(drops: Vec<Drop>) -> Result<impl View> {
    let now = unix_now();

    Ok(view! {
        <section id="drops">
            if drops.is_empty() {
                <p class="empty">"Nothing here yet. Share something!"</p>
            } else {
                <div class="toolbar">
                    <button class="quiet" data-on:click="confirm('Delete all drops?') && @delete('/drops')">
                        "Clear all"
                    </button>
                </div>
            }
            <ul>
                for drop in drops {
                    <li class="drop">
                        if drop.meta.kind == Kind::Link {
                            <a class="content" href=(drop.text.trim().to_owned()) target="_blank" rel="noopener noreferrer">(drop.text.clone())</a>
                        } else {
                            <div class="content">(drop.text.clone())</div>
                        }
                        <div class="meta">
                            <span>(age(drop.meta.created_at, now))</span>
                            <span>(expires(&drop, now))</span>
                            <span class="actions">
                                <button class="quiet" data-copy=(drop.text.clone())>"Copy"</button>
                                <button class="quiet" data-on:click=(format!("@delete('/drops/{}')", drop.id))>"Delete"</button>
                            </span>
                        </div>
                    </li>
                }
            </ul>
        </section>
    })
}
