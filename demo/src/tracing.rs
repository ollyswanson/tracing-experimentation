use layer::compat_layer::CompatLayer;
use layer::fmt::json::JsonFormatter;
use opentelemetry::global;
use tracing::subscriber::set_global_default;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::{EnvFilter, Registry};

pub fn setup_tracing(use_otel: bool) {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("INFO"));
    let subscriber = Registry::default()
        .with(env_filter)
        .with(CompatLayer::new(JsonFormatter::new(), std::io::stdout));

    if use_otel {
        global::set_text_map_propagator(opentelemetry_jaeger::Propagator::new());

        let tracer = opentelemetry_jaeger::new_agent_pipeline()
            .with_service_name("cats")
            .install_simple()
            .expect("Failed to install tracer");

        let otel = tracing_opentelemetry::layer().with_tracer(tracer);
        let subscriber = subscriber.with(otel);
        set_global_default(subscriber).expect("Failed to set subscriber");
    } else {
        set_global_default(subscriber).expect("Failed to set subscriber");
    }
}
