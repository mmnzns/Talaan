//! A browser's tab URLs. Written in W4 (UI Automation on the address bar; the active tab only).

use crate::calls::tabs::{Family, Tabs};

pub fn tab_urls(_app: &str, _family: Family) -> Tabs {
    Tabs::Unknown
}
