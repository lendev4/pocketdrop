//! M0: drops live in memory only. Disk storage on the volume arrives in M1.

use std::{
    sync::{Mutex, PoisonError},
    time::{Duration, SystemTime},
};

use tokio::sync::broadcast;
use topcoat::{
    Result,
    context::{Cx, app_context},
    view::{View, component, view},
};

const MAX_DROPS: usize = 50;

#[derive(Clone)]
pub struct Drop {
    pub created: SystemTime,
    pub text: String,
    pub source: &'static str,
}

impl Drop {
    fn is_link(&self) -> bool {
        let text = self.text.trim();
        (text.starts_with("https://") || text.starts_with("http://"))
            && !text.contains(char::is_whitespace)
    }
}

pub struct Drops {
    state: Mutex<State>,
    changed: broadcast::Sender<()>,
}

#[derive(Default)]
struct State {
    items: Vec<Drop>,
}

impl Default for Drops {
    fn default() -> Self {
        Self {
            state: Mutex::default(),
            changed: broadcast::channel(16).0,
        }
    }
}

impl Drops {
    pub fn add(&self, text: String, source: &'static str) {
        {
            let mut state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
            state.items.push(Drop { created: SystemTime::now(), text, source });
            if state.items.len() > MAX_DROPS {
                state.items.remove(0);
            }
        }
        // No receivers just means no page is open.
        let _ = self.changed.send(());
    }

    /// Newest first.
    pub fn list(&self) -> Vec<Drop> {
        let state = self.state.lock().unwrap_or_else(PoisonError::into_inner);
        state.items.iter().rev().cloned().collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<()> {
        self.changed.subscribe()
    }
}

pub fn drops(cx: &Cx) -> &Drops {
    app_context::<Drops>(cx)
}

fn age(created: SystemTime) -> String {
    let secs = created.elapsed().unwrap_or(Duration::ZERO).as_secs();
    match secs {
        0..60 => "just now".to_owned(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86_400 => format!("{} h ago", secs / 3600),
        _ => format!("{} d ago", secs / 86_400),
    }
}

/// The list Datastar replaces on every change, matched by its id.
#[component]
pub async fn drop_list(drops: Vec<Drop>) -> Result<impl View> {
    Ok(view! {
        <ul id="drops">
            if drops.is_empty() {
                <li class="empty">"Nothing here yet. Share something!"</li>
            }
            for drop in drops {
                <li class="drop">
                    if drop.is_link() {
                        <a class="content" href=(drop.text.trim().to_owned()) target="_blank" rel="noopener noreferrer">(drop.text.clone())</a>
                    } else {
                        <div class="content">(drop.text.clone())</div>
                    }
                    <div class="meta">
                        <span>(age(drop.created))</span>
                        <span>"from " (drop.source)</span>
                        <span class="actions">
                            <button class="quiet" data-copy=(drop.text.clone())>"Copy"</button>
                        </span>
                    </div>
                </li>
            }
        </ul>
    })
}
