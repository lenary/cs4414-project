use super::events::RaftEvent;
use std::vec::Vec;
use std::io::IoError;
use serialize::Encodable;
use serialize::Decodable;
use serialize::json::{Encoder,Error};

pub use self::peer::{NetPeer, NetPeerConfig};
pub use self::rpc::{RpcAppendEntriesReq, RpcAppendEntriesRes, RpcVoteReq, RpcVoteRes};

pub mod peer;
pub mod rpc;

// TODO: put entry functions in here, like:
// - start_peer_helper (which returns a Peer struct, including fields
// with channels to message with)
// TODO: Peer will contain identity info and channels for
// communicating with this peer task

pub fn start_peer_task(cfg: ~NetPeerConfig) -> Option<~NetPeer> {
    None
}
