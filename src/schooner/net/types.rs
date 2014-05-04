use std::io::net::ip::SocketAddr;
use super::super::events::*;

// Bare RPC types. This is the incoming type before we set up channels
// to make Raft messages. Might not be necessary, provided we can setup
// those channels in the functions where the RPCs are built out of network
// bytes.
#[deriving(Decodable, Encodable, Eq, Clone, Show)]
pub enum RaftRpc {
    RpcARQ(AppendEntriesReq),
    RpcARS(AppendEntriesRes),
    RpcVRQ(VoteReq),
    RpcVRS(VoteRes),
    RpcStopReq,
}

#[deriving(Clone, Hash, Eq, TotalEq, Show)]
pub struct NetPeerConfig {
    pub id: uint,
    // Peer's Raft listening address, but not necessarily the port we will
    // get our request from. Hence the peer should send its id when it
    // makes its first connection to us.
    pub address: SocketAddr,
    // The port for this field is the peer's client listening port.
    pub client_addr: SocketAddr,
}
