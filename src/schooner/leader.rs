
use super::traits::RaftState;
use super::Server;

pub struct Leader {
    server: ~Server,
    // TODO: What other fields?
}

impl Leader {
    fn new(server: ~Server) -> Leader {
        Leader {
            server: server,
        }
    }
}

// TODO: Fill this out with real implementations
impl RaftState for Leader { }
