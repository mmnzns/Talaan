//! "Summary ready" notifications. Written in W3 (Windows toasts).

pub fn setup(_on_click: impl Fn(String) + Send + Sync + 'static) {}

pub fn post(_id: &str, _title: &str, _body: &str) {}
