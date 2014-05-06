
use super::events::*;

use super::server::RaftServerState;
use super::server::{RaftStateTransition, NextState, Continue};
use super::server::{RaftNextState, RaftLeader, RaftCandidate};

// This module contains the callback functions for Raft Followers.
//
// Rust doesn't support arbitrary methods being defined in another
// file... except with traits.
//
// I don't know how this copes with small private helper functions.
// Hopefully you won't need to declare them in the trait before you
// can define them in the impl Follower for RaftServerState.

pub trait Follower {
    fn follower_setup(&mut self) -> RaftStateTransition;
    fn follower_teardown(&mut self);

    fn follower_append_entries_req(&mut self, req: AppendEntriesReq, chan: Sender<AppendEntriesRes>) -> RaftStateTransition;
    fn follower_append_entries_res(&mut self, res: AppendEntriesRes) -> RaftStateTransition;

    fn follower_vote_req(&mut self, req: VoteReq, chan: Sender<VoteRes>) -> RaftStateTransition;
    fn follower_vote_res(&mut self, res: VoteRes) -> RaftStateTransition;
}

// TODO: Real Implementations
impl Follower for RaftServerState {
    fn follower_setup(&mut self) -> RaftStateTransition {
        Continue
    }

    fn follower_teardown(&mut self) {
    }


    fn follower_append_entries_req(&mut self, req: AppendEntriesReq, chan: Sender<AppendEntriesRes>) -> RaftStateTransition {
        // redirect client to leader
        Continue
    }

    fn follower_append_entries_res(&mut self, res: AppendEntriesRes) -> RaftStateTransition {
        Continue
    }


    fn follower_vote_req(&mut self, req: VoteReq, chan: Sender<VoteRes>) -> RaftStateTransition {
        Continue
    }

    fn follower_vote_res(&mut self, res: VoteRes) -> RaftStateTransition  {
        Continue
    }
}
