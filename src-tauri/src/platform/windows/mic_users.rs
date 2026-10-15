//! Which apps are using the microphone right now. Written in W2 (CapabilityAccessManager consent store).

/// On Windows `bundle_id` holds the exe name or the packaged app's family name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicUser {
    pub pid: i32,
    pub bundle_id: String,
    pub path: String,
}

pub fn mic_users() -> Vec<MicUser> {
    vec![]
}
