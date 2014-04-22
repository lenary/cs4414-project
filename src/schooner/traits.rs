
use super::events::*;

// This is a trait for all the Raft States: Leader, Candidate,
// Follower. Each will have its own struct, and then inside that
// struct it will have a pointer to an owned "Server" struct that
// contains all information that is shared between these states.
//
// These can all return a new RaftState, if they want to change state.
// If they want to just continue in the same state, return None;
pub trait RaftState {

    // Called before the loop starts for this RaftState
    fn handle_setup(&mut self) -> Option<~RaftState> {
        None
    }

    // Called when a new RaftState is requested, so that we can clear
    // up any old state
    fn handle_teardown(&mut self) { }

    // A Timer will fire every HEARTBEAT_INTERVAL, this is the
    // callback where it should be handled.
    fn handle_timeout(&mut self) -> Option<~RaftState> {
        None
    }

    // A thing with a default implementation to use all these other
    // methods
    // TODO: expand this or remove it
    fn handle_event(&mut self, event: ~RaftEvent) -> Option<~RaftState> {
        None
    }


    // Another Peer that thinks it's a Leader has sent an Append Entries
    // RPC.
    fn handle_append_entries_req(&mut self, req: AppendEntriesReq) -> Option<~RaftState> {
        None
    }

    // This Peer (which thinks it's the Leader, maybe) has sent an
    // Append Entries RPC, and got a Response from another Peer.
    // Handle the response here.
    fn handle_append_entries_res(&mut self, res: AppendEntriesRes) -> Option<~RaftState> {
        None
    }


    // This Peer has just recieved a Vote Request RPC.
    fn handle_vote_req(&mut self, req: VoteReq) -> Option<~RaftState> {
        None
    }

    // This Peer (which thinks it's a Candidate, maybe) has just
    // sent a Vote Request RPC, and got a Response from another Peer.
    // Handle the response here.
    fn handle_vote_res(&mut self, res: VoteRes) -> Option<~RaftState> {
        None
    }


    // A Peer (that thinks This Peer is the Leader) has forwarded an
    // Application Request to the Current Peer. Handle it here.
    // TODO: Can we abandon this, and just use `handle_application_req`?
    fn handle_handoff_req(&mut self, req: HandoffReq) -> Option<~RaftState> {
        None
    }

    // This Peer forwarded an Application Request to the Peer it
    // thought was the Leader, and has got a Response. Handle the
    // response here.
    fn handle_handoff_res(&mut self, req: HandoffReq) -> Option<~RaftState> {
        None
    }

    // This Peer has just recieved an Application Request. Handle the
    // Request here.
    fn handle_application_req(&mut self, req: ApplicationReq) -> Option<~RaftState> {
        None
    }
}
