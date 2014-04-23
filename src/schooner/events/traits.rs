
// A trait so we can send all events from the network layer down the
// same channel. 
pub trait RaftEvent: Send {

    // I'd imagine that each "request" type has a channel inside it
    // that we can send the response down, so that it can get to the
    // network layer and then the other cluster members.
    // The bool is for success/failure
    fn respond(&self, _response: ~RaftEvent) -> bool {
        false
    }

    // TODO: anything else?
}
