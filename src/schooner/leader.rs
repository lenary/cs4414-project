#![feature(globs)]
use super::events::*;
use super::net::*;
use super::net::RaftRpc;
use super::server::RaftServerState;
use super::server::{RaftStateTransition, NextState, Continue};
use super::server::{RaftNextState, RaftCandidate, RaftFollower};
use std::vec::Vec;
use uuid::{Uuid, UuidVersion, Version4Random};

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
        let aer = AppendEntriesReq{
                                term: self.log.term, //TODO get more accurate info from election?
                                prev_log_idx: self.log.idx,
                                prev_log_term: self.log.term,
                                commit_idx: self.log.idx,
                                leader_id: self.id,
                                uuid: Uuid::new(Version4Random).unwrap(),
                                entries: Vec::new()};

        self.peers.msg_all_peers(RpcARQ(aer));

        Continue
    }

    fn leader_append_entries_req(&mut self, req: AppendEntriesReq, chan: Sender<AppendEntriesRes>) -> RaftStateTransition {

        //add entry to local log
        if (self.log.append_entries(&req).is_ok()) {
            //send entry to followers
            self.peers.msg_all_peers(RpcARQ(req));
        }

        Continue
    }

    fn leader_append_entries_res(&mut self, res: AppendEntriesRes) -> RaftStateTransition {

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

