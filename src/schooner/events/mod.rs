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
pub use self::application::{ApplicationReq, ApplicationRes};
pub use self::append_entries::{AppendEntriesReq, AppendEntriesRes};

mod traits; // Annoyingly can't be called "trait" because keyword
pub mod append_entries;
mod vote;
mod handoff;
mod application;

// We have to wrap the RaftEvents in this EventMsg to send them all
// down the same channel.
pub enum RaftPeerMsg {
    ARQ(AppendEntriesReq),
    ARS(AppendEntriesRes),
    VRQ(VoteReq),
    VRS(VoteRes),
    HRQ(HandoffReq),
    HRS(HandoffRes),
    StopReq,
}

pub enum RaftAppMsg {
    APRQ(ApplicationReq),
    APRS(ApplicationRes)
}
