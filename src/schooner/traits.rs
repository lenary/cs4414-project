
use super::events::*;

// This is a trait for all the Raft States: Leader, Candidate,
// Follower. Each will have its own struct, and then inside that
// struct it will have a pointer to an owned "Server" struct that
// contains all information that is shared between these states.
//
// These can all return a new RaftState, if they want to change state.
// If they want to just continue in the same state, return None;
pub trait RaftState {

    fn handle_setup(&mut self) -> Option<~RaftState> {
        None
    }

    fn handle_teardown(&mut self) { }


    fn handle_timeout(&mut self) -> Option<~RaftState> {
        None
    }


    fn handle_append_entries_req(&mut self, req: AppendEntriesReq) -> Option<~RaftState> {
        None
    }

    fn handle_append_entries_res(&mut self, res: AppendEntriesRes) -> Option<~RaftState> {
        None
    }


    fn handle_vote_req(&mut self, req: VoteReq) -> Option<~RaftState> {
        None
    }

    fn handle_vote_res(&mut self, res: VoteRes) -> Option<~RaftState> {
        None
    }


    fn handle_application_req(&mut self, req: ApplicationReq) -> Option<~RaftState> {
        None
    }

    fn handle_handoff_req(&mut self, req: HandoffReq) -> Option<~RaftState> {
        None
    }
}
