use crate::tracing::setup_tracing;

mod cats;
mod run;
mod tracing;

#[tokio::main]
async fn main() {
    let use_otel = std::env::var("USE_OTEL").is_ok();
    let show_spans = std::env::var("SHOW_SPANS").is_ok();
    setup_tracing(use_otel, show_spans);
    run::run().await.unwrap();
}
