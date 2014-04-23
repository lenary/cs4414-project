
use super::traits::RaftState;
use super::Server;

pub struct Follower; //{
    // server: ~Server,
    // TODO: What other fields?
//}

impl Follower {
    pub fn new() -> Follower {
        Follower
    }
}

// TODO: Fill this out with real implementations
impl RaftState for Follower {
}
