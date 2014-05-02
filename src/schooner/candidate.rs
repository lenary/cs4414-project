
use super::events::*;

use super::server::RaftServerState;
use super::server::{RaftStateTransition, NextState, Continue};
use super::server::{RaftNextState, RaftLeader, RaftFollower};

// Rust doesn't support arbitrary methods being defined in another
// file... except with traits.
//
// I don't know how this copes with small private helper functions.
// Hopefully you won't need to declare them in the trait before you
// can define them in the impl Candidate for RaftServerState.

pub trait Candidate {
    fn candidate_setup(&mut self) -> RaftStateTransition;
    fn candidate_teardown(&mut self);

    fn candidate_append_entries_req(&mut self, req: AppendEntriesReq, chan: Sender<AppendEntriesRes>) -> RaftStateTransition;
    fn candidate_append_entries_res(&mut self, res: AppendEntriesRes) -> RaftStateTransition;

    fn candidate_vote_req(&mut self, req: VoteReq, chan: Sender<VoteRes>) -> RaftStateTransition;
    fn candidate_vote_res(&mut self, res: VoteRes) -> RaftStateTransition;
}

// TODO: Real Implementations
impl Candidate for RaftServerState {
    fn candidate_setup(&mut self) -> RaftStateTransition {
        Continue
    }

    fn candidate_teardown(&mut self) {
    }


    fn candidate_append_entries_req(&mut self, req: AppendEntriesReq, chan: Sender<AppendEntriesRes>) -> RaftStateTransition {
        Continue
    }

    fn candidate_append_entries_res(&mut self, res: AppendEntriesRes) -> RaftStateTransition {
        Continue
    }


    fn candidate_vote_req(&mut self, req: VoteReq, chan: Sender<VoteRes>) -> RaftStateTransition {
        Continue
    }

    fn candidate_vote_res(&mut self, res: VoteRes) -> RaftStateTransition  {
        Continue
    }
}

