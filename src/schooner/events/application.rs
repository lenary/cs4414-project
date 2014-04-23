// Events for Application-level RPCs

use super::traits::RaftEvent;

// TODO: fill this out
pub struct ApplicationReq;
pub struct ApplicationRes;

impl RaftEvent for ApplicationReq {
    // TODO: implement respond
}

impl RaftEvent for ApplicationRes {}
