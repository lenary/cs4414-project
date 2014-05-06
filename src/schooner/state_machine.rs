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
use rand::{task_rng,Rng};

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

// pub enum LockState {
//     Locked,
//     Unlocked
// }

// pub struct Locked {
//     locked: bool,
//     state: Option<int>,
//     id: Option<int>
// }

// pub struct Unlocked {
//     unlocked: bool,
//     state: Option<int>,
//     id: Option<int>
// }

pub struct LockState {
	locked: bool,
	counter: uint
}

// pub struct LockRes {
//     new_state: Option<State>,
//     status: bool,
//     id: Option<int>
// }

// pub struct UnlockRes {
//     new_state: Option<State>,
//     status: bool,
//     id: Option<int>
// }
 
impl LockState {
  pub fn spawn(recv: Receiver<(ClientCmdReq, Sender<ClientCmdRes>)>) {
    spawn(proc() {
      ls = LockState::new();
      
      ls.loop(recv);
    });
  }
  
  fn new() -> LockState {
    return LockState { 
      locked: false,
      counter: 0
    }
  }
  
  fn loop(&mut self, recv: Receiver<(ClientCmdReq, Sender<ClientCmdRes>)>) {
    loop {
      let (cmd, sender) = recv.recv();
      
      //self.lock();
      

      //self.unlock(uint);
      
    }
  }
  
  fn lock(&mut self) -> Option<uint> {
    if !self.locked {
      self.locked = true;
      Some(self.counter)
    } else {
      None
    }
  }
  
  fn unlock(&mut self, lockval: uint) -> bool {
    if self.locked  && self.counter == lockval { 
      self.locked = false;
      self.counter += 1;
      true
    }
    else {
      false
    }
  }
}

// impl LockState {

//     fn lock_request(object: &mut LockState) -> LockRes {
//         if !object.locked { //if unlocked
            
//             //update with some deterministic but RANDOM NUMBER
//             // let mut rand_num = task_rng().gen::<int>();
//             // if rand_num.gen() {
//             // 	println!("int: {}, uint: {}", rng.gen::<int>(), rng.gen::<uint>())
//             // }

//             let result = LockRes {new_state: Some(Locked), status: true, id: Some(rand_num) };         
//             result
//             //- send lock msg on a Chan 
//             //endp_send.send(result);
//             //- send "true" and <id> on Chan
//         }

//         else if object.locked { //if already locked
//             //do nothing
//         }
//     }

//     fn unlock_request(object: &mut Unlocked) -> UnlockRes {
//         if object.locked { //if locked
//             let result = UnlockRes {new_state: Some(Unlocked), status: false, id: None};
//             result
//             }
//         // else if !self.locked {
//         //     Continue //already unlocked do nothing
//         // }
//     }
// }

// impl Locked {

//     fn unlock_request(object: &mut Locked, request_id: Option<int>) -> UnlockRes {
//         if object.locked {
//             match request_id {
//             	Some(id) => { let result = UnlockRes {
//             							new_state: Some(Unlocked), 
//             							status: true, 
//             							id: None};
//             							result 
//             						},
            	
//             	None => {}//do nothing
			
//                 //id sent by request does not match, continue/do nothing
// 				//if unlock request (with same <id>) send unlock msg on Chan + "true" and <id> on Chan
//             }
//         }

//         else if !object.locked {
//             //already unlocked, do nothing
//         }
//     }

//     fn lock_request(object: &mut Locked) -> LockRes {
//         if object.locked {
//             //already locked
//         }

//         // else if !self.locked {
//         //     Continue //already locked do nothing
//         // }
// 	}
// }

