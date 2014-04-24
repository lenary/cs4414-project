
use super::events::*;
use super::leader::Leader;
use super::candidate::Candidate;
use super::follower::Follower;

pub enum RaftNextState {
    RaftLeader(Leader),
    RaftCandidate(Candidate),
    RaftFollower(Follower)
}

pub enum RaftStateTransition {
    NextState(RaftNextState),
    Continue,
    Stop
}

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


    // This Peer has just recieved an Application Request. Handle the
    // Request here.
    fn handle_application_req(&mut self, _req: ApplicationReq) -> RaftStateTransition {
        Continue
    }

    fn handle_application_res(&mut self, _res: ApplicationRes) -> RaftStateTransition {
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
}

// Save some time by using a macro
// Macro Guide: http://static.rust-lang.org/doc/master/guide-macros.html
macro_rules! transition_proxy(
    ($meth:ident) => (
        match *self {
            RaftLeader(ref mut leader)       => leader.$meth(),
            RaftCandidate(ref mut candidate) => candidate.$meth(),
            RaftFollower(ref mut follower)   => follower.$meth()
        }
        );
    ($meth:ident, $arg:ident) => (
        match *self {
            RaftLeader(ref mut leader)       => leader.$meth($arg),
            RaftCandidate(ref mut candidate) => candidate.$meth($arg),
            RaftFollower(ref mut follower)   => follower.$meth($arg)
        }
    )
)

impl RaftState for RaftNextState {
    fn handle_setup(&mut self) -> RaftStateTransition {
        transition_proxy!(handle_setup)
    }

    fn handle_teardown(&mut self) {
        transition_proxy!(handle_teardown)
    }

    fn handle_timeout(&mut self) -> RaftStateTransition {
        transition_proxy!(handle_timeout)
    }

    fn handle_append_entries_req(&mut self, req: AppendEntriesReq) -> RaftStateTransition {
        transition_proxy!(handle_append_entries_req, req)
    }

    fn handle_append_entries_res(&mut self, res: AppendEntriesRes) -> RaftStateTransition {
        transition_proxy!(handle_append_entries_res, res)
    }

    fn handle_vote_req(&mut self, req: VoteReq) -> RaftStateTransition {
        transition_proxy!(handle_vote_req, req)
    }

    fn handle_vote_res(&mut self, res: VoteRes) -> RaftStateTransition {
        transition_proxy!(handle_vote_res, res)
    }

    fn handle_application_req(&mut self, req: ApplicationReq) -> RaftStateTransition {
        transition_proxy!(handle_application_req, req)
    }

    fn handle_application_res(&mut self, res: ApplicationRes) -> RaftStateTransition {
        transition_proxy!(handle_application_res, res)
    }

    fn handle_handoff_req(&mut self, req: HandoffReq) -> RaftStateTransition {
        transition_proxy!(handle_handoff_req, req)
    }

    fn handle_handoff_res(&mut self, res: HandoffRes) -> RaftStateTransition {
        transition_proxy!(handle_handoff_res, res)
    }
}
