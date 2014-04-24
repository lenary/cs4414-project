
use super::traits::{RaftState, RaftStateTransition, NextState, Continue};
use super::traits::{RaftNextState, RaftLeader, RaftFollower};

use super::follower::Follower;


pub struct Leader; // TODO: What Fields

impl Leader {
    pub fn new() -> Leader {
        Leader
    }
}

// TODO: Fill this out with real implementations
impl RaftState for Leader { }
