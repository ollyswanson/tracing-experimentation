use std::fmt;
use std::time::Duration;

use serde::ser::{SerializeMap, Serializer as _};
use serde_json::ser::Serializer;
use tracing_core::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::registry::SpanRef;

use crate::compat_layer::SpanLifecycle;
use crate::compat_layer::StaticMetadata;
use crate::compat_layer::Visitor;

pub struct JsonFormatter;

impl JsonFormatter {
    pub(crate) fn format_event<S>(
        event: &Event<'_>,
        ctx: Context<'_, S>,
        meta: &StaticMetadata,
        buffer: &mut Vec<u8>,
    ) -> Result<(), fmt::Error>
    where
        S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        let visit = || {
            let mut serializer = Serializer::new(buffer);
            let mut serializer = serializer.serialize_map(None)?;
            let mut visitor = Visitor::default();
            event.record(&mut visitor);
            let metadata = event.metadata();

            let current_span = event
                .parent()
                .and_then(|id| ctx.span(id))
                .or_else(|| ctx.lookup_current());

            serializer.serialize_entry("level", metadata.level().as_str())?;
            serializer.serialize_entry(
                "title",
                visitor
                    .fields()
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or(metadata.name()),
            )?;
            serializer.serialize_entry("type", "event")?;

            if let Some(span) = &current_span {
                serializer.serialize_entry("parent", span.metadata().name())?;
            }

            serializer.serialize_entry("source.filename", &event.metadata().file())?;
            serializer.serialize_entry("source.line", &event.metadata().line())?;
            serializer.serialize_entry("source.target", &event.metadata().target())?;
            serializer.serialize_entry("source.pid", &meta.pid.to_string())?;

            if let Some(current_span) = current_span {
                Self::spans(&mut serializer, current_span)?;
            }

            for (k, v) in visitor.fields().iter().filter(|&(k, _)| *k != "message") {
                serializer.serialize_entry(k, v)?;
            }

            serializer.end()
        };

        visit().map_err(|_| fmt::Error)
    }

    pub(crate) fn format_span<S>(
        span: SpanRef<'_, S>,
        meta: &StaticMetadata,
        lifecycle: SpanLifecycle,
        buffer: &mut Vec<u8>,
    ) -> Result<(), fmt::Error>
    where
        S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        let visit = || {
            let mut serializer = Serializer::new(buffer);
            let mut serializer = serializer.serialize_map(None)?;

            serializer.serialize_entry("level", span.metadata().level().as_str())?;
            serializer.serialize_entry("title", span.metadata().name())?;
            serializer.serialize_entry("type", lifecycle.as_str())?;
            serializer.serialize_entry("source.filename", &span.metadata().file())?;
            serializer.serialize_entry("source.line", &span.metadata().line())?;
            serializer.serialize_entry("source.target", &span.metadata().target())?;
            serializer.serialize_entry("source.pid", &meta.pid.to_string())?;

            Self::spans(&mut serializer, span)?;

            serializer.end()
        };

        visit().map_err(|_| fmt::Error)?;
        Ok(())
    }

    fn spans<M, S>(serializer: &mut M, span: SpanRef<'_, S>) -> Result<(), M::Error>
    where
        M: SerializeMap,
        S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        for span in span.scope().from_root() {
            let extensions = span.extensions();
            let visitor = extensions
                .get::<Visitor>()
                .expect("Extensions should contain visitor, this is a bug");

            for (key, val) in visitor.fields() {
                serializer.serialize_entry(key, val)?;
            }
        }
        Ok(())
    }
}

pub fn format_duration(duration: Duration) -> String {
    let secs_part = match duration.as_secs().checked_mul(1_000_000_000) {
        Some(v) => v,
        None => return format!("{}s", duration.as_secs()),
    };

    let duration_in_ns = secs_part + u64::from(duration.subsec_nanos());

    fn format_fraction(value: f64, suffix: &str) -> String {
        if value < 10.0 {
            format!("{:.3}{}", value, suffix)
        } else if value < 100.0 {
            format!("{:.2}{}", value, suffix)
        } else if value < 1000.0 {
            format!("{:.1}{}", value, suffix)
        } else {
            format!("{:.0}{}", value, suffix)
        }
    }

    if duration_in_ns < 1_000 {
        format!("{}ns", duration_in_ns)
    } else if duration_in_ns < 1_000_000 {
        format_fraction(duration_in_ns as f64 / 1_000.0, "us")
    } else if duration_in_ns < 1_000_000_000 {
        format_fraction(duration_in_ns as f64 / 1_000_000.0, "ms")
    } else {
        format_fraction(duration_in_ns as f64 / 1_000_000_000.0, "s")
    }
}
