// This is just something easy to import that re-exports all the
// events. Other modules should just have to `use schooner::events::*;`

// the RaftEvent Trait, which allows all the events to be sent down
// the same channel.
pub use self::traits::RaftEvent;

// Raft-level Messages
// TODO:At some point we may need events for cluster changes (eg joins
// and parts)
pub use self::vote::{VoteReq, VoteRes};
pub use self::handoff::{HandoffReq, HandoffRes};

// Application-level Messages
pub use self::client::{ClientCmdReq, ClientCmdRes};
pub use self::append_entries::{AppendEntriesReq, AppendEntriesRes};

pub mod traits; // Annoyingly can't be called "trait" because keyword
pub mod append_entries;
mod vote;
mod handoff;
mod client;

// We have to wrap the RaftEvents in this EventMsg to send them all
// down the same channel.
// Messages for sending within Raft, basically. Contain channels for
// communication with the networking modules.
pub enum RaftMsg {
    ARQ(AppendEntriesReq, Sender<AppendEntriesRes>),
    ARS(AppendEntriesRes),
    VRQ(VoteReq, Sender<VoteRes>),
    VRS(VoteRes),
    StopReq,
}

// Bare RPC types. Only used when we don't need associated channels for
// network stuff.
pub enum RaftRpc {
    RpcARQ(AppendEntriesReq),
    RpcARS(AppendEntriesRes),
    RpcVRQ(VoteReq),
    RpcVRS(VoteRes),
    RpcStopReq,
}

pub enum ClientCmd {
    CMDRQ(ClientCmdReq, Sender<ClientCmdRes>),
    CMDRS(ClientCmdRes)
}
