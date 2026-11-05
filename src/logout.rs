use topcoat::{
    Result,
    context::Cx,
    router::{
        error::{SeeOther, see_other},
        route,
    },
};

use crate::auth;

#[route(POST)]
async fn logout(cx: &Cx) -> Result<SeeOther> {
    auth::require_same_origin(cx)?;
    auth::log_out(cx);
    Ok(see_other("/login"))
}

/// Logs out every other device; this one stays logged in.
#[route(POST "./others")]
async fn others(cx: &Cx) -> Result<SeeOther> {
    auth::require_api(cx)?;
    auth::log_out_others(cx).await?;
    Ok(see_other("/?others=out"))
}
