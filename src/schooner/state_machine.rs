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

pub struct LockState {
	locked: bool,
	counter: uint
}
 
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