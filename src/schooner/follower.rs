
use super::traits::{RaftState, RaftStateTransition, NextState, Continue};
use super::traits::{RaftNextState, RaftLeader, RaftCandidate};

use super::leader::Leader;
use super::candidate::Candidate;

pub struct Follower; // TODO: What fields?


impl Follower {
    pub fn new() -> Follower {
        Follower
    }
}

// TODO: Fill this out with real implementations
impl RaftState for Follower { }
