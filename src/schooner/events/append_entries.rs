// Events for the AppendEntries RPC

use uuid::Uuid;
use std::vec::Vec;
use serialize::{json, Decodable};

use super::super::consistent_log::LogEntry;

#[deriving(Decodable, Encodable, Eq, Clone, Show)]
pub struct AppendEntriesReq {
    // Raft information
    pub term: u64,          // current term of leader
    pub prev_log_idx: u64,  // idx of leader's log entry immediately before first entry in this AER
    pub prev_log_term: u64, // term of leader's log entry immediately before first entry in this AER
    pub commit_idx: u64,    // last idx of log committed to leader's state machine
    pub leader_id: u64,
    pub entries: Vec<LogEntry>, // entries to log; may be empty (hearbeat msg)
    pub uuid: Uuid,
}

#[deriving(Decodable, Encodable, Eq, Clone, Show)]
pub struct AppendEntriesRes {
    pub success: bool,
    pub term: u64,
    pub uuid: Uuid,
    pub id: uint
}

impl AppendEntriesReq {
    // Anything extra we need to have, like figuring out whether we were successful.
}

impl AppendEntriesRes {
}
