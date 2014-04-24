
use super::traits::{RaftState, RaftStateTransition, NextState, Continue};
use super::traits::{RaftNextState, RaftLeader, RaftFollower};

use super::leader::Leader;
use super::follower::Follower;


pub struct Candidate; // TODO: What fields?

impl Candidate {
    pub fn new() -> Candidate {
        Candidate
    }
}

// TODO: Fill this out with real implementations
impl RaftState for Candidate { }
