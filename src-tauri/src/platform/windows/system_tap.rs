//! System (call) audio capture. Written in W1 Task 9 (WASAPI process loopback).

use crate::recorder::source::{AudioSource, SharedFeed};

#[derive(Default)]
pub struct SystemTap;

const UNAVAILABLE: &str = "System audio isn't available yet on Windows";

impl AudioSource for SystemTap {
    fn start(&mut self, _feed: SharedFeed) -> Result<String, String> {
        Err(UNAVAILABLE.into())
    }

    fn restart(&mut self) -> Result<String, String> {
        Err(UNAVAILABLE.into())
    }

    fn stop(&mut self) {}
}
