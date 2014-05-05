#![feature(globs)]
use super::events::*;

use super::server::RaftServerState;
use super::server::{RaftStateTransition, NextState, Continue};
use super::server::{RaftNextState, RaftCandidate, RaftFollower};

// Rust doesn't support arbitrary methods being defined in another
// file... except with traits.
//
// I don't know how this copes with small private helper functions.
// Hopefully you won't need to declare them in the trait before you
// can define them in the impl Leader for RaftServerState.

pub trait Leader {
    fn leader_setup(&mut self) -> RaftStateTransition;
    fn leader_teardown(&mut self);
    fn leader_heartbeat(&mut self) -> RaftStateTransition;

    fn leader_append_entries_req(&mut self, req: AppendEntriesReq, chan: Sender<AppendEntriesRes>) -> RaftStateTransition;
    fn leader_append_entries_res(&mut self, res: AppendEntriesRes) -> RaftStateTransition;

    fn leader_vote_req(&mut self, req: VoteReq, chan: Sender<VoteRes>) -> RaftStateTransition;
    fn leader_vote_res(&mut self, res: VoteRes) -> RaftStateTransition;
}

// TODO: Real Implementations
impl Leader for RaftServerState {
    fn leader_setup(&mut self) -> RaftStateTransition {
        //send initial heartbeat to all followers
        self.leader_heartbeat();
        Continue
    }

    fn leader_teardown(&mut self) {
        self.new_state(RaftFollower);
    }

    fn leader_heartbeat(&mut self) -> RaftStateTransition {
        //make an empty append entry request using self.leader_append_entries_req()
        Continue
    }

    fn leader_append_entries_req(&mut self, req: AppendEntriesReq, chan: Sender<AppendEntriesRes>) -> RaftStateTransition {
        //TODO
        //add entry to local log

        //add entry to consistent log
        //send req to followers

        Continue
    }

    fn leader_append_entries_res(&mut self, res: AppendEntriesRes) -> RaftStateTransition {
        //TODO
        //listen for a conform (majority of peers) of the log
        //pass the log into application state machine
        //pass the log to client using RaftAppMsg<AppMsg>
        Continue
    }


    fn leader_vote_req(&mut self, req: VoteReq, chan: Sender<VoteRes>) -> RaftStateTransition {
        Continue
    }

    fn leader_vote_res(&mut self, res: VoteRes) -> RaftStateTransition  {
        Continue
    }
}

