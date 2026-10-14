use topcoat::{
    Result,
    context::Cx,
    router::{error::not_found, path_param, route},
};

use crate::{auth, store};

path_param!(drop_id);

/// Clear all. Open pages update through the live stream.
#[route(DELETE)]
async fn clear(cx: &Cx) -> Result<()> {
    auth::require_api(cx)?;
    store(cx).clear().await?;
    Ok(())
}

#[route(DELETE "./{drop_id}")]
async fn delete(cx: &Cx) -> Result<()> {
    auth::require_api(cx)?;
    let id: &str = path_param::<DropId>(cx);
    if store(cx).remove(id).await? {
        Ok(())
    } else {
        Err(not_found().into())
    }
}
