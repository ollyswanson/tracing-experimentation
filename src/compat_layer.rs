use std::any::TypeId;
use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::marker;
use std::time::{Duration, Instant};

use tracing_core::field::{Field, Visit};
use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{Dispatch, Event, Subscriber};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::json::JsonFormatter;

pub struct CompatLayer<S, W> {
    meta: StaticMetadata,
    get_context: WithContext,
    make_writer: W,
    _registry: marker::PhantomData<S>,
}

pub struct StaticMetadata {
    pub name: String,
    pub pid: u32,
}

// this function "remembers" the types of the subscriber so that we
// can downcast to something aware of them without knowing those
// types at the callsite.
//
// See https://github.com/tokio-rs/tracing/blob/4dad420ee1d4607bad79270c1520673fa6266a3d/tracing-error/src/layer.rs
pub(crate) struct WithContext(
    #[allow(clippy::type_complexity)] fn(&Dispatch, &Id, f: &mut dyn FnMut(&mut Visitor)),
);

impl WithContext {
    // This function allows a function to be called in the context of the
    // "remembered" subscriber.
    pub(crate) fn with_context(
        &self,
        dispatch: &Dispatch,
        id: &Id,
        mut f: impl FnMut(&mut Visitor),
    ) {
        (self.0)(dispatch, id, &mut f)
    }
}

impl<S, W> CompatLayer<S, W>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    W: for<'writer> MakeWriter<'writer> + 'static,
{
    pub fn new(name: String, make_writer: W) -> Self {
        Self {
            meta: StaticMetadata {
                name,
                pid: std::process::id(),
            },
            get_context: WithContext(Self::get_context),
            make_writer,
            _registry: marker::PhantomData,
        }
    }

    fn get_context(dispatch: &Dispatch, id: &Id, f: &mut dyn FnMut(&mut Visitor)) {
        let subscriber = dispatch
            .downcast_ref::<S>()
            .expect("subscriber should downcats to expected type; this is a bug!");
        let span = subscriber
            .span(id)
            .expect("registry should have a span for the current ID");

        let mut extensions = span.extensions_mut();
        if let Some(visitor) = extensions.get_mut::<Visitor>() {
            f(visitor);
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Visitor<'a> {
    fields: BTreeMap<&'a str, serde_json::Value>,
}

impl<'a> Visitor<'a> {
    pub fn fields(&self) -> &BTreeMap<&'a str, serde_json::Value> {
        &self.fields
    }
}

impl Visit for Visitor<'_> {
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.fields
            .insert(field.name(), serde_json::Value::from(value));
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        self.fields
            .insert(field.name(), serde_json::Value::from(value));
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        self.fields
            .insert(field.name(), serde_json::Value::from(value));
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        self.fields
            .insert(field.name(), serde_json::Value::from(value));
    }

    fn record_str(&mut self, field: &Field, value: &str) {
        self.fields
            .insert(field.name(), serde_json::Value::from(value));
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        match field.name() {
            // Skip fields that are actually log metadata that have already been handled
            name if name.starts_with("log.") => (),
            name if name.starts_with("r#") => {
                self.fields
                    .insert(&name[2..], serde_json::Value::from(format!("{:?}", value)));
            }
            name => {
                self.fields
                    .insert(name, serde_json::Value::from(format!("{:?}", value)));
            }
        };
    }
}

/// New type around the instant to avoid interfering with other layers.
pub(crate) struct InstantWrapper(Instant);

pub(crate) enum SpanLifecycle {
    Start,
    End,
}

impl SpanLifecycle {
    pub fn as_str(&self) -> &'static str {
        use SpanLifecycle::*;

        match self {
            Start => "start",
            End => "end",
        }
    }
}

impl<S, W> Layer<S> for CompatLayer<S, W>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    W: for<'writer> MakeWriter<'writer> + 'static,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        // We record the span's attributes for later use, as we won't get another chance to
        // access them.
        let span = ctx.span(id).expect("Span not found, this is a bug");
        let mut visitor: Visitor<'_> = Visitor::default();
        attrs.record(&mut visitor);
        span.extensions_mut().insert(visitor);

        let mut buf = Vec::new();
        let _ = JsonFormatter::format_span(span, &self.meta, SpanLifecycle::Start, &mut buf);
        buf.push(b'\n');
        let _ = self.make_writer.make_writer().write_all(&buf);
    }

    fn on_record(&self, span: &Id, values: &Record<'_>, ctx: Context<'_, S>) {
        let span = ctx.span(span).expect("Span not found, this is a bug");
        let mut extensions = span.extensions_mut();

        // We stored the visitor when we created the span, we *should* be able to access it now.
        let visitor = extensions
            .get_mut::<Visitor>()
            .expect("Visitor not found on 'record', this is a bug");

        values.record(visitor);
    }

    fn on_enter(&self, id: &Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("Span not found, this is a bug");

        let mut extensions = span.extensions_mut();

        // A span can be entered multiple times in an async context, but we only worry about
        // recording the total duration of the span, not the idle + active time, so we insert the
        // instant once, on the first time the span is entered.
        if extensions.get_mut::<InstantWrapper>().is_none() {
            extensions.insert(InstantWrapper(Instant::now()));
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        let mut buf = Vec::new();
        let _ = JsonFormatter::format_event(event, ctx, &self.meta, &mut buf);
        buf.push(b'\n');
        let _ = self.make_writer.make_writer().write_all(&buf);
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        let span = ctx.span(&id).expect("Span not found, this is a bug");
        let mut buf = Vec::new();
        let _ = JsonFormatter::format_span(span, &self.meta, SpanLifecycle::End, &mut buf);
        buf.push(b'\n');
        let _ = self.make_writer.make_writer().write_all(&buf);
    }

    // SAFETY: The pointer returned by downcast_ref is non-null and points to a valid instance of
    // the type with the provided `TypeId`. Additionally the `WithContext` function pointer is
    // valid for the lifetime of `&self`.
    unsafe fn downcast_raw(&self, id: TypeId) -> Option<*const ()> {
        match id {
            id if id == TypeId::of::<Self>() => Some(self as *const _ as *const ()),
            id if id == TypeId::of::<WithContext>() => {
                Some(&self.get_context as *const _ as *const ())
            }
            _ => None,
        }
    }
}
