use std::sync::Arc;

use axum::routing::get;
use axum::Router;

use crate::cats::get_cat;

pub struct AppState {
    pub client: reqwest::Client,
}

pub async fn run() -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let app_state = Arc::new(AppState { client });

    let app = Router::new().route("/", get(get_cat)).with_state(app_state);

    let addr = "127.0.0.1:8080".parse().unwrap();
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
