use super::events::RaftEvent;

pub use self::peer::{Peer,parse_config};

mod peer;

// TODO: put entry functions in here, like:
// - start_peer_helper (which returns a Peer struct, including fields
// with channels to message with)

// TODO: This may need more, or may not be needed at all, I don't
// know.
// It may just be easier to serialize and deserialize in the other
// RaftEvent trait itself.
pub trait RaftNetEvent {
    fn build() -> ~RaftEvent;
}

struct PeerConfig;

// TODO: Peer will contain identity info and channels for
// communicating with this peer task
pub fn start_peer_task(cfg: ~PeerConfig) -> Option<~Peer> {
    None
}
