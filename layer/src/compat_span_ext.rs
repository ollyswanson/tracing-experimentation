use tracing::Span;

use crate::compat_layer::WithContext;

pub trait CompatSpanExt {
    /// Get the correlation_id from the current span, if it exists.
    fn get_stored<T: From<serde_json::Value>>(&self, key: &str) -> Option<T>;
}

impl CompatSpanExt for Span {
    fn get_stored<T: From<serde_json::Value>>(&self, key: &str) -> Option<T> {
        let mut val = None;

        self.with_subscriber(|(id, dispatch)| {
            if let Some(get_context) = dispatch.downcast_ref::<WithContext>() {
                get_context.with_context(dispatch, id, |storage| {
                    val = storage.fields().get(key).map(|v| T::from(v.clone()));
                    val.is_some()
                })
            }
        });

        val
    }
}
