use layer::compat_layer::CompatLayer;
use layer::fmt::json::JsonFormatter;
use tracing::subscriber::set_global_default;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

pub fn setup_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("INFO"));
    let subscriber = Registry::default()
        .with(env_filter)
        .with(CompatLayer::new(JsonFormatter::new(), std::io::stdout));

    set_global_default(subscriber).expect("Failed to set subscriber");
}
