use super::events::RaftEvent;
use std::vec::Vec;
use std::io::IoError;
use serialize::Encodable;
use serialize::Decodable;
use serialize::json::{Encoder,Error};

pub use self::peer::{Peer,parse_config};

pub mod peer;
pub mod messages;
pub mod handlers;

// TODO: put entry functions in here, like:
// - start_peer_helper (which returns a Peer struct, including fields
// with channels to message with)

// TODO: This may need more, or may not be needed at all, I don't
// know.
// This is essentially to do serialisation/deserialisation
// there should be only one of these per RaftEvent right now

#[deriving(Encodable)]
pub trait RaftNetEvent<RaftEvent> {
    fn deserialize(s: &str) -> Result<~RaftEvent, Error>;
    fn serialize(&self) -> ~str;
}

// TODO: fill this out further.
struct PeerConfig;

// TODO: Peer will contain identity info and channels for
// communicating with this peer task
pub fn start_peer_task(cfg: ~PeerConfig) -> Option<~Peer> {
    None
}
