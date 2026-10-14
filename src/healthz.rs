use topcoat::{Result, router::route};

#[route(GET)]
async fn healthz() -> Result<&'static str> {
    Ok("ok")
}
