// Events for Follower-to-Leader Handoff of Requests

use super::traits::RaftEvent;

// TODO: fill this out
pub struct HandoffReq;
pub struct HandoffRes;

impl RaftEvent for HandoffReq {
    // TODO: implement respond
}

impl RaftEvent for HandoffRes {}
