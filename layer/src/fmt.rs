pub mod json;

use std::fmt;
use std::io;
use std::time::Duration;

use tracing_core::{Event, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

pub trait Format<S>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn format_event<W: fmt::Write>(
        &self,
        event: &Event<'_>,
        ctx: Context<'_, S>,
        writer: W,
    ) -> fmt::Result;
}

struct WriteAdaptor<'a, W>(&'a mut W)
where
    W: fmt::Write;

impl<'a, W> io::Write for WriteAdaptor<'a, W>
where
    W: fmt::Write,
{
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let s =
            std::str::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.0
            .write_str(s)
            .map_err(|e| io::Error::new(io::ErrorKind::Other, e))?;

        Ok(s.as_bytes().len())
    }

    fn flush(&mut self) -> io::Result<()> {
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
