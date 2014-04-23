
use super::events::*;
use super::machine::{RaftStateTransition,Continue};

// This is a trait for all the Raft States: Leader, Candidate,
// Follower. Each will have its own struct, and then inside that
// struct it will have a pointer to an owned "Server" struct that
// contains all information that is shared between these states.
//
// These can all return a new RaftState, if they want to change state.
// If they want to just continue in the same state, return None;
pub trait RaftState {

    // Called before the loop starts for this RaftState
    fn handle_setup(&mut self) -> RaftStateTransition {
        Continue
    }

    // Called when a new RaftState is requested, so that we can clear
    // up any old state
    fn handle_teardown(&mut self) { }

    // A Timer will fire every HEARTBEAT_INTERVAL, this is the
    // callback where it should be handled.
    fn handle_timeout(&mut self) -> RaftStateTransition {
        Continue
    }


    // Another Peer that thinks it's a Leader has sent an Append Entries
    // RPC.
    fn handle_append_entries_req(&mut self, _req: AppendEntriesReq) -> RaftStateTransition {
        Continue
    }

    // This Peer (which thinks it's the Leader, maybe) has sent an
    // Append Entries RPC, and got a Response from another Peer.
    // Handle the response here.
    fn handle_append_entries_res(&mut self, _res: AppendEntriesRes) -> RaftStateTransition {
        Continue
    }


    // This Peer has just recieved a Vote Request RPC.
    fn handle_vote_req(&mut self, _req: VoteReq) -> RaftStateTransition {
        Continue
    }

    // This Peer (which thinks it's a Candidate, maybe) has just
    // sent a Vote Request RPC, and got a Response from another Peer.
    // Handle the response here.
    fn handle_vote_res(&mut self, _res: VoteRes) -> RaftStateTransition {
        Continue
    }


    // A Peer (that thinks This Peer is the Leader) has forwarded an
    // Application Request to the Current Peer. Handle it here.
    // TODO: Can we abandon this, and just use `handle_application_req`?
    fn handle_handoff_req(&mut self, _req: HandoffReq) -> RaftStateTransition {
        Continue
    }

    // This Peer forwarded an Application Request to the Peer it
    // thought was the Leader, and has got a Response. Handle the
    // response here.
    fn handle_handoff_res(&mut self, _req: HandoffRes) -> RaftStateTransition {
        Continue
    }

    // This Peer has just recieved an Application Request. Handle the
    // Request here.
    fn handle_application_req(&mut self, _req: ApplicationReq) -> RaftStateTransition {
        Continue
    }

    fn handle_application_res(&mut self, _res: ApplicationRes) -> RaftStateTransition {
        Continue
    }
}
