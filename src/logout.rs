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
