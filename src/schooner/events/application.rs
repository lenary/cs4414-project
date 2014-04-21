// Events for Application-level RPCs

use super::traits::RaftEvent;

// TODO: fill this out
pub struct ApplicationReq;
pub struct ApplicationRes;
pub struct HandoffReq;
pub struct HandoffRes;

impl RaftEvent for ApplicationReq {
    // TODO: implement respond
}

impl RaftEvent for ApplicationRes {}

impl RaftEvent for HandoffReq {
    // TODO: implement respond
}

impl RaftEvent for HandoffRes {}
