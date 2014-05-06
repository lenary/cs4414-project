use uuid::{Uuid, UuidVersion, Version4Random};

use super::events::*;
use super::server::RaftServerState;
use super::server::{RaftStateTransition, NextState, Continue};
use super::server::{RaftNextState, RaftLeader, RaftCandidate};

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
        if req.commit_idx > self.last_applied {
            self.last_applied += 1;
            // TODO:
            // self.to_app_sm.send(log(lastApplied))
        }
        if req.term > self.current_term {
            // TODO: maybe state transition to follower (do setup/teardown again)?
            // Raft paper:
            //     If RPC request or response contains term T > currentTerm:
            //     set currentTerm = T, convert to follower
        }
        else if req.term < self.current_term {
            chan.send(AppendEntriesRes {
                success: false,
                term: self.current_term,
                uuid: Uuid::new(Version4Random).unwrap(),
            });
        }
        // TODO: (pseudo-haskell for being concise ...)
        // if len $ filter (self.log[previousLogIdx].term ==) (map (.term) req.entries) then reply false
        
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
