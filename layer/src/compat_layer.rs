use std::any::TypeId;
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::marker;
use std::time::Instant;

use tracing_core::field::{Field, Visit};
use tracing_core::span::{Attributes, Id, Record};
use tracing_core::{Dispatch, Event, Subscriber};
use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::fmt::Format;

pub struct CompatLayer<S, F, W> {
    formatter: F,
    get_context: WithContext,
    make_writer: W,
    with_spans: bool,
    _registry: marker::PhantomData<S>,
}

// this function "remembers" the types of the subscriber so that we
// can downcast to something aware of them without knowing those
// types at the callsite.
//
// See https://github.com/tokio-rs/tracing/blob/4dad420ee1d4607bad79270c1520673fa6266a3d/tracing-error/src/layer.rs
pub(crate) struct WithContext(
    #[allow(clippy::type_complexity)] fn(&Dispatch, &Id, f: &mut dyn FnMut(&Visitor) -> bool),
);

impl WithContext {
    // This function allows a function to be called in the context of the
    // "remembered" subscriber.
    pub(crate) fn with_context(
        &self,
        dispatch: &Dispatch,
        id: &Id,
        mut f: impl FnMut(&Visitor) -> bool,
    ) {
        (self.0)(dispatch, id, &mut f)
    }
}

impl<S, F, W> CompatLayer<S, F, W>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    F: Format<S>,
    W: for<'writer> MakeWriter<'writer> + 'static,
{
    pub fn new(formatter: F, make_writer: W) -> Self {
        Self {
            formatter,
            get_context: WithContext(Self::get_context),
            make_writer,
            with_spans: false,
            _registry: marker::PhantomData,
        }
    }

    pub fn with_spans(mut self, with_spans: bool) -> Self {
        self.with_spans = with_spans;
        self
    }

    fn get_context(dispatch: &Dispatch, id: &Id, f: &mut dyn FnMut(&Visitor) -> bool) {
        let subscriber = dispatch
            .downcast_ref::<S>()
            .expect("subscriber should downcast to expected type; this is a bug!");
        let span = subscriber
            .span(id)
            .expect("registry should have a span for the current ID");

        for span in span.scope().from_root() {
            let mut extensions = span.extensions_mut();
            if let Some(visitor) = extensions.get_mut::<Visitor>() {
                if f(visitor) {
                    return;
                }
            }
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

    pub fn fields_mut(&mut self) -> &mut BTreeMap<&'a str, serde_json::Value> {
        &mut self.fields
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

macro_rules! with_event_from_span {
    ($id:ident, $span:ident, $($field:literal = $value:expr),*, |$event:ident| $code:block) => {
        let meta = $span.metadata();
        let cs = meta.callsite();
        let fs = tracing_core::field::FieldSet::new(&[$($field),*], cs);
        #[allow(unused)]
        let mut iter = fs.iter();
        let v = [$(
            (&iter.next().unwrap(), Some(&$value as &dyn tracing_core::field::Value)),
        )*];
        let vs = fs.value_set(&v);
        let $event = tracing_core::Event::new_child_of($id, meta, &vs);
        $code
    };
}

/// New type around the instant to avoid interfering with other layers.
pub(crate) struct InstantWrapper(Instant);

impl<S, F, W> Layer<S> for CompatLayer<S, F, W>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    F: Format<S> + 'static,
    W: for<'writer> MakeWriter<'writer> + 'static,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, id: &Id, ctx: Context<'_, S>) {
        // We record the span's attributes for later use as we won't get another chance to access
        // them.
        let span = ctx.span(id).expect("Span not found, this is a bug");
        let mut visitor: Visitor<'_> = Visitor::default();
        attrs.record(&mut visitor);
        span.extensions_mut().insert(visitor);
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
        let first_entry = extensions.get_mut::<InstantWrapper>().is_none();
        if first_entry {
            extensions.insert(InstantWrapper(Instant::now()));

            if self.with_spans {
                // We also make use of the first span entry to "log" the start.
                with_event_from_span!(id, span, "message" = "start", |event| {
                    drop(extensions);
                    drop(span);
                    self.on_event(&event, ctx);
                });
            }
        }
    }

    fn on_close(&self, id: Id, ctx: Context<'_, S>) {
        if self.with_spans {
            let span = ctx.span(&id).expect("Span not found, this is a bug");
            let start = span
                .extensions()
                .get::<InstantWrapper>()
                .map(|i| i.0)
                .expect("Start not found, this is a bug");

            let elapsed = crate::fmt::format_duration(start.elapsed());

            with_event_from_span!(id, span, "message" = "end", "elapsed" = elapsed, |event| {
                drop(span);
                self.on_event(&event, ctx);
            });
        }
    }

    fn on_event(&self, event: &Event<'_>, ctx: Context<'_, S>) {
        // We can avoid extra allocations by using a thread local here.
        thread_local! {
            static BUF: RefCell<String> = RefCell::new(String::new());
        }

        BUF.with(|buf| {
            let borrow = buf.try_borrow_mut();
            let mut a;
            let mut b;
            let buf = match borrow {
                Ok(buf) => {
                    a = buf;
                    &mut *a
                }
                _ => {
                    b = String::new();
                    &mut b
                }
            };

            let _ = self.formatter.format_event(event, ctx, &mut *buf);
            let _ = self.make_writer.make_writer().write_all(buf.as_bytes());
            buf.clear();
        })
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
