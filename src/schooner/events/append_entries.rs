// Events for the AppendEntries RPC

use super::traits::RaftEvent;

// TODO: fill this out. Probably from the other append_entries.rs
pub struct AppendEntriesReq;
pub struct AppendEntriesRes;

impl RaftEvent for AppendEntriesReq {
    // TODO: implement respond()
}


impl RaftEvent for AppendEntriesRes {
}
