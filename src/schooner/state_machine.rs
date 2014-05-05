#![allow(dead_code)]

use super::events::*;
use super::events::append_entries::{AppendEntriesReq, AppendEntriesRes};
use super::events::{VoteReq, VoteRes};
use super::follower::Follower;   // A trait with impl for RaftServerState
use super::candidate::Candidate; // A trait with impl for RaftServerState
use super::leader::Leader;       // A trait with impl for RaftServerState

use std::comm::*;
use std::io::timer::Timer;
use std::vec::Vec;

//Not sure yet if needed
mod events;
mod consistent_log;
mod net;

mod server;
mod leader;
mod candidate;
mod follower;


// Some ideas of how to use with Application Tasks:
//
// let (to_app_send, to_app_rec) = channel();
// let (from_app_send, from_app_rec) = channel();
//
// spawn_app_task(to_app_rec, from_app_send);
//
// let (from_endpoint_send, from_endpoint_rec) = channel();
//
// spawn_main_app_endpoint_task(from_endpoint_send);
//
// rs = RaftServer::new(...opts...); // TODO
// rs.add_peer(...peercfg...); // TODO
// RaftServer::spawn(rs, to_app_send, from_app_rec, from_endpoint_rec);

// STATE MACHINE
/* NOTES
    pass as <~str>
    LockReq -> LockResp(bool, id)  //using Option<int>
    UnlockReq(<id>) -> UnlockResp(bool)

loop {
next_msg = Rec(Msg).recv();
let mut state = Option<id>;
match state {
    None => state = Unlocked;
    Some<id> => state = Locked(id: ---)
}
*/

pub enum State {
    Locked,
    Unlocked
}
pub struct Locked {
    locked: bool,
    state: Option<int>,
    id: Option<int>
}

pub struct Unlocked {
    unlocked: bool,
    state: Option<int>,
    id: Option<int>
}

pub struct LockRes {
    new_state: Option<State>,
    status: bool,
    id: Option<int>
}

pub struct UnlockRes {
    new_state: Option<State>,
    status: bool,
    id: Option<int>
}

/*
impl Unlocked {

    fn lock_request(&mut self) -> LockRes {
        if self.unlocked {
            //update with some deterministic but RANDOM NUMBER
            let result = LockRes { new_state: Some(Locked), status: true, id: Some(1) };         
            //- send lock msg on a Chan 
            //- send "true" and <id> on Chan
        }

        else if !self.unlocked {
            //already locked do nothing
        }
    }

    fn unlock_request(&mut self) -> UnlockRes {
        if self.unlocked {
            //already unlocked
            }
        // else if !self.locked {
        //     Continue //already unlocked do nothing
        // }
    }
}

impl Locked {

    fn unlock_request(&mut self) -> UnlockRes {
        if self.locked {
            match self.id {
                //if unlock request (but different lock <id>) continue / do nothing
                //if unlock request (with same <id>) send unlock msg on Chan + "true" and <id> on Chan
                None => Unlocked,
                Some(id) => Locked,

            }

            let result = UnlockRes{new_state: Some(Unlocked), status: true, id: None};
            result
        }

        else if !self.locked {
            //already unlocked do nothing
        }
    }

    fn lock_request(&mut self) -> LockRes {
        if self.locked {
            //already locked
        }

        // else if !self.locked {
        //     Continue //already locked do nothing
        // }
    }
}
*/