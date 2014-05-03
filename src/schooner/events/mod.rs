use serialize::{Decodable, Encodable};

// This is just something easy to import that re-exports all the
// events. Other modules should just have to `use schooner::events::*;`

// Raft-level Messages
// TODO:At some point we may need events for cluster changes (eg joins
// and parts)
pub use self::vote::{VoteReq, VoteRes};

// Application-level Messages
pub use self::client::{ClientCmdReq, ClientCmdRes};
pub use self::append_entries::{AppendEntriesReq, AppendEntriesRes};

pub mod append_entries;
mod vote;
mod client;

// We have to wrap the RaftEvents in this RaftMsg to send them all
// down the same channel.
//
// Messages for sending within Raft, basically. Contain channels for
// communication with the networking modules.
//
// One of these gets sent out from the networking code and into Raft
// mainloop, and then the reply for the associated message goes out
// on the channel if it was a "Request" type message. If it's a
// "Response" type message, and we're receiving it, then we don't
// need to do another reply back.
pub enum RaftMsg {
    ARQ(AppendEntriesReq, Sender<AppendEntriesRes>),
    ARS(AppendEntriesRes),
    VRQ(VoteReq, Sender<VoteRes>),
    VRS(VoteRes),
    StopReq,
}
