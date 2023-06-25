use crate::tracing::setup_tracing;

mod cats;
mod run;
mod tracing;

#[tokio::main]
async fn main() {
    setup_tracing();
    run::run().await.unwrap();
}
