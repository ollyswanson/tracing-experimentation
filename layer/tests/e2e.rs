mod mock_writer;

use std::sync::{Arc, Mutex};

use layer::compat_layer::CompatLayer;
use layer::fmt::json::JsonFormatter;
use serde_json::Value;
use tracing::{info, span};
use tracing_core::Level;
use tracing_subscriber::layer::SubscriberExt;

use crate::mock_writer::MockWriter;

// Run a closure and collect the output emitted by the tracing instrumentation using an in-memory
// buffer.
fn run_and_get_raw_output<F: Fn()>(action: F) -> String {
    let buffer = Arc::new(Mutex::new(vec![]));
    let make_writer = {
        let buffer = buffer.clone();
        move || MockWriter::new(buffer.clone())
    };

    let subscriber =
        tracing_subscriber::registry().with(CompatLayer::new(JsonFormatter::new(), make_writer));
    tracing::subscriber::with_default(subscriber, action);

    let buffer_guard = buffer.lock().unwrap();
    let output = buffer_guard.to_vec();
    String::from_utf8(output).unwrap()
}

fn run_and_get_output<F: Fn()>(action: F) -> Vec<Value> {
    run_and_get_raw_output(action)
        .lines()
        .filter(|&l| !l.trim().is_empty())
        .inspect(|l| println!("{}", l))
        .map(|line| serde_json::from_str::<Value>(line).unwrap())
        .collect()
}

fn test_action() {
    let a = 2;
    let span = span!(Level::DEBUG, "shaving_yaks", a);
    let _enter = span.enter();

    info!(message = "pre-shaving yaks");
    let b = 3;
    let skipped = false;
    let new_span = span!(Level::DEBUG, "inner shaving", b, skipped);
    let _enter2 = new_span.enter();

    info!("shaving yaks");
}

#[test]
fn each_line_is_valid_json() {
    let tracing_output = run_and_get_raw_output(test_action);

    // Each line is valid JSON
    for line in tracing_output.lines().filter(|&l| !l.is_empty()) {
        assert!(serde_json::from_str::<Value>(line).is_ok());
    }
}

#[test]
fn see_output() {
    let _output = run_and_get_output(test_action);
}
