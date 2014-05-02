// Events for Application-level RPCs

use super::traits::RaftEvent;

// TODO: fill this out
pub struct ClientCmdReq;
pub struct ClientCmdRes;


/*
 * How we react to an application's request.
 */
impl RaftEvent for ClientCmdReq {
    // TODO: implement respond
}

/*
 * Raft's resposne to an application's request.
 */
impl RaftEvent for ClientCmdRes {

}
