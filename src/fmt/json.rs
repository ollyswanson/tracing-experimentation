use std::fmt;
use std::marker;

use serde::ser::{SerializeMap, Serializer as _};
use serde_json::ser::Serializer;
use tracing_core::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::registry::SpanRef;

use crate::compat_layer::Visitor;
use crate::fmt::WriteAdaptor;

use super::Format;

pub struct JsonFormatter<S> {
    // Store as string to avoid reformatting each time it's needed.
    pid: String,
    _registry: marker::PhantomData<S>,
}

impl<S> JsonFormatter<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    pub fn new() -> Self {
        Self {
            pid: std::process::id().to_string(),
            _registry: marker::PhantomData,
        }
    }

    fn spans<M>(serializer: &mut M, span: SpanRef<'_, S>) -> Result<(), M::Error>
    where
        M: SerializeMap,
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

impl<S> Format<S> for JsonFormatter<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn format_event<W: fmt::Write>(
        &self,
        event: &Event<'_>,
        ctx: Context<'_, S>,
        mut writer: W,
    ) -> Result<(), fmt::Error>
    where
        W: fmt::Write,
    {
        let mut visit = || {
            let mut serializer = Serializer::new(WriteAdaptor(&mut writer));
            let mut serializer = serializer.serialize_map(None)?;
            let mut visitor = Visitor::default();
            event.record(&mut visitor);
            let metadata = event.metadata();

            let current_span = event
                .parent()
                .and_then(|id| ctx.span(id))
                .or_else(|| ctx.lookup_current());

            serializer.serialize_entry("level", metadata.level().as_str())?;
            let message = visitor.fields_mut().remove("message");

            serializer.serialize_entry(
                "title",
                message
                    .as_ref()
                    .and_then(|m| m.as_str())
                    .unwrap_or(metadata.name()),
            )?;

            if let Some(span) = &current_span {
                serializer.serialize_entry("span", span.metadata().name())?;
            }

            serializer.serialize_entry("source.filename", &event.metadata().file())?;
            serializer.serialize_entry("source.line", &event.metadata().line())?;
            serializer.serialize_entry("source.target", &event.metadata().target())?;
            serializer.serialize_entry("source.pid", &self.pid)?;

            if let Some(current_span) = current_span {
                Self::spans(&mut serializer, current_span)?;
            }

            for (k, v) in visitor.fields() {
                serializer.serialize_entry(k, v)?;
            }

            serializer.end()
        };

        visit().map_err(|_| fmt::Error)?;
        writeln!(writer)
    }
}
