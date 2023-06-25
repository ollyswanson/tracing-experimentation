use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use bytes::Bytes;
use serde::Deserialize;
use thiserror::Error;
use tracing::Span;

use crate::run::AppState;

#[tracing::instrument("get_cat", skip(state))]
pub async fn get_cat(State(state): State<Arc<AppState>>) -> Result<Html<String>, CatError> {
    static API_URL: &str = "https://api.thecatapi.com/v1/images/search";
    let client = &state.client;

    let link = get_link(client, API_URL).await.map_err(|e| {
        tracing::error!(message = "Failed to get a link", error = ?e);
        e
    })?;

    let raw_image = get_image(client, &link).await.map_err(|e| {
        tracing::error!(message = "Failed to download image", error = ?e);
        e
    })?;

    let current_span = Span::current();
    let ascii_cat = tokio::task::spawn_blocking(move || {
        let _enter = current_span.enter();
        asciify(raw_image)
    })
    .await
    .unwrap()
    .map_err(|e| {
        tracing::error!(message = "Failed to process image", error = ?e);
        e
    })?;

    Ok(Html(ascii_cat))
}

#[tracing::instrument("get_cat_link", skip(client))]
async fn get_link(client: &reqwest::Client, url: &str) -> anyhow::Result<String> {
    #[derive(Deserialize)]
    struct CatLink {
        url: String,
    }

    tracing::info!("Fetching link!");

    client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .json::<Vec<CatLink>>()
        .await?
        .pop()
        .map(|image| image.url)
        .ok_or_else(|| anyhow::anyhow!("Missing cat!"))
}

#[tracing::instrument("get_cat_image", skip(client))]
async fn get_image(client: &reqwest::Client, url: &str) -> anyhow::Result<Bytes> {
    tracing::info!("Fetching image!");

    client
        .get(url)
        .send()
        .await?
        .error_for_status()?
        .bytes()
        .await
        .map_err(|e| e.into())
}

#[tracing::instrument("asciifying_cat", skip(bytes))]
fn asciify(bytes: Bytes) -> anyhow::Result<String> {
    let image = image::load_from_memory(&bytes).map_err(|e| anyhow::anyhow!(e))?;

    tracing::info!("Converting image!");

    let ascii = artem::convert(
        image,
        artem::options::OptionBuilder::new()
            .target(artem::options::TargetType::HtmlFile(false, false))
            .build(),
    );

    Ok(ascii)
}

#[derive(Error, Debug)]
#[error(transparent)]
pub struct CatError(#[from] anyhow::Error);

impl IntoResponse for CatError {
    fn into_response(self) -> Response {
        (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong").into_response()
    }
}
