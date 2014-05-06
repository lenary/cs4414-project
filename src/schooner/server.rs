#![allow(dead_code)]

use super::events::*;
use super::events::append_entries::{AppendEntriesReq, AppendEntriesRes};
use super::events::{VoteReq, VoteRes};
use super::follower::Follower;   // A trait with impl for RaftServerState
use super::candidate::Candidate; // A trait with impl for RaftServerState
use super::leader::Leader;       // A trait with impl for RaftServerState

use std::comm::*;
use std::io::timer::Timer;
use std::vec::Vec;

// Some kind of Idea for how the whole thing works:
//
// The Raft task in this diagram denotes the task started by
// RaftServer#run() - it contains the local raft state machine.
//
// Then it communicates with 3 other kinds of task via channels
//
// 1. the Raft Peer tasks - these are a set of tasks, one per other
//    peer, which specifically handle all messages relating to RPC
//    between the current peer and the other peer.
//
// 2. the Application State Machine task - This is the
//    strongly-consistent replicated state machine that is custom
//    per-application. It recieves messages as they are committed to
//    the log.
//
// 3. the Application Endpoint tasks- This is the set of tasks that
//    handle requests from Application clients, which have to go
//    through Raft to get to the Application State Machine.
//
//
//    +task-----------+
//    |  Application  |
//    |  Replicated   |
//    |     State     |
//    |    Machine    |
//    |       ^       |
//    +-------+-------+
//            |
//            |<-- RaftAppMsg<AppMsg>
//            |
//            |         RaftPeerRPC<AppMsg>
//    +task---+-------+    |         -------
//    |       v       |    |      --/       \--
//    |               |    v     /             \
//    |     Raft     <+----------+>   Peer     |
//    |               |          \    Tasks    /
//    |       ^       |           --\       /--
//    +-------+-------+              -------
//            |
//            |<-- RaftAppMsg<AppMsg>
//         ---+---
//      --/   v   \--
//     /  Endpoint   \
//     |    Tasks    |
//     \             /
//      --\   ^   /--
//         ---+---
//            |
//            |
//            v
//       Application
//         Clients
//

// TODO: Parameterise by <AppMsg>, the type of messages that the app
// uses.
pub struct RaftServer {
    election_timeout:  u64,
    heartbeat_interval: u64,

    // TODO: store peers... somehow
    peer_configs: Vec<()>,
}

impl RaftServer {
    pub fn new() -> RaftServer {
        let election_timeout = 150;

        RaftServer {
            election_timeout:  election_timeout,
            heartbeat_interval: (election_timeout/2),
            peer_configs:      Vec::new(),
        }
    }

    // TODO: Add Peers
    pub fn add_peer() -> bool {
        false
    }

    // TODO: Spawn Peers
    pub fn spawn_peer_tasks(&mut self, sender: Sender<RaftMsg>) -> Vec<()> {
        Vec::new()
    }

    pub fn spawn(&mut self,
                to_app_sm:         Sender<(ClientCmdReq, Sender<ClientCmdRes>)>,
                from_app_endpoint: Receiver<(ClientCmdReq, Sender<ClientCmdRes>)>) {

        let (from_peers_send, from_peers_rec) = channel();
        let peers = self.spawn_peer_tasks(from_peers_send);

        let heartbeat_interval = self.heartbeat_interval;

        spawn(proc() {

            let mut server_state = RaftServerState::new(RaftFollower,
                                                        to_app_sm,
                                                        peers);

            let mut timer = Timer::new().unwrap();
            let timer_rec : Receiver<()> = timer.periodic(heartbeat_interval);

            // some other way to encapsulate self?
            server_state.main_loop(&timer_rec,
                                   &from_peers_rec,
                                   &from_app_endpoint)
        });
    }
}

// This is the state within the main raft process including the Raft
// Roles, and the Log.
pub struct RaftServerState {
    // State Machine
    current_state: RaftNextState,
    is_setup:      bool,

    // Channel to message the Application State Machine with
    to_app_sm:     Sender<(ClientCmdReq, Sender<ClientCmdRes>)>,

    // Peer Configurations
    peers:         Vec<()>,
}

// These are the possible Roles a Raft Peer could be in: Leader,
// Candidate and Follower
#[deriving(Eq,Clone,Show)]
pub enum RaftNextState {
    RaftLeader,
    RaftCandidate,
    RaftFollower
}

// An Option<T>-like type for saying whether we should transition or
// not (used as a return from the callback functions)
#[deriving(Eq,Clone,Show)]
pub enum RaftStateTransition {
    NextState(RaftNextState),
    Continue,
    Stop
}


// Unfortunately Rust can't generate method names,
// so your code has to do it.
//
// Use this like so:
//
// state_proxy!(leader_setup,
//              candidate_setup,
//              follower_setup);
//
// or:
//
// state_proxy!(leader_append_entries_res,
//              candidate_append_entries_res,
//              follower_append_entries_res,
//              res);
//
// or:
//
// state_proxy!(leader_append_entries_req,
//              candidate_append_entries_req,
//              follower_append_entrries_req,
//              req, resp_chan);
//
macro_rules! state_proxy(
    ($l:ident, $c:ident, $f:ident) => {
        match self.current_state {
            RaftLeader => self.$l(),
            RaftCandidate => self.$c(),
            RaftFollower => self.$f(),
        }
    };
    ($l:ident, $c:ident, $f:ident, $e:ident) => {
        match self.current_state {
            RaftLeader => self.$l($e),
            RaftCandidate => self.$c($e),
            RaftFollower => self.$f($e),
        }
    };
    ($l:ident, $c:ident, $f:ident, $e:ident, $s:ident) => {
        match self.current_state {
            RaftLeader => self.$l($e, $s),
            RaftCandidate => self.$c($e, $s),
            RaftFollower => self.$f($e, $s),
        }
    };
)

impl RaftServerState {
    fn new(current_state: RaftNextState,
           to_app_sm: Sender<(ClientCmdReq, Sender<ClientCmdRes>)>,
           peers: Vec<()>) -> RaftServerState {
        RaftServerState {
            current_state: current_state,
            is_setup: false,
            to_app_sm: to_app_sm,
            peers: peers,
        }
    }

    fn new_state(&mut self, new_state: RaftNextState) {
        self.current_state = new_state;
        self.is_setup = false;
    }

    // The main peer loop. This does a few things:
    // - Waits for requests and responses from peers
    // - Waits for requests from the client endpoint
    // - Waits for timeouts
    //
    // And then calls the appropriate callback when one appears.
    fn main_loop(&mut self,
                 timer_rec: &Receiver<()>,
                 peer_rec:  &Receiver<RaftMsg>,
                 endpoint_rec: &Receiver<(ClientCmdReq, Sender<ClientCmdRes>)>) {

        // Use every time an invocation may transition the state machine
        // Macro Guide: http://static.rust-lang.org/doc/master/guide-macros.html
        macro_rules! may_transition (
            ($e:expr) => (
                match $e {
                    NextState(new_state) => {
                        debug!("Trans {:?} ==> {:?}", self.current_state, new_state);

                        self.handle_teardown();

                        self.new_state(new_state);

                        continue;
                    },
                    Stop => {
                        debug!("Stop {:?} ==> ()", self.current_state);

                        self.handle_teardown();

                        break;
                    },
                    Continue => {}
                }
            )
        );

        let select   = Select::new();
        let mut timer_handle    = select.handle(timer_rec);
        let mut peer_handle     = select.handle(peer_rec);
        let mut endpoint_handle = select.handle(endpoint_rec);

        unsafe {
            timer_handle.add();
            peer_handle.add();
            endpoint_handle.add();
        }

        loop {
            may_transition!(self.handle_setup());

            let ready_id = select.wait();
            if timer_handle.id() == ready_id {
                timer_handle.recv();
                may_transition!(self.handle_heartbeat());
            }
            else if peer_handle.id() == ready_id {
                // An Event Message from a Peer
                let event: RaftMsg = peer_handle.recv();
                match event {
                    ARQ(ae_req, ae_chan) =>
                        may_transition!(self.handle_append_entries_req(ae_req, ae_chan)),
                    ARS(ae_res) =>
                        may_transition!(self.handle_append_entries_res(ae_res)),
                    VRQ(vote_req, vote_chan) =>
                        may_transition!(self.handle_vote_req(vote_req, vote_chan)),
                    VRS(vote_res) =>
                        may_transition!(self.handle_vote_res(vote_res)),
                    StopReq => {
                        may_transition!(Stop);
                    }
                }
            }
            else if endpoint_handle.id() == ready_id {
                // An Event Message from an Endpoint
                let (req, chan) = endpoint_handle.recv();
                may_transition!(self.handle_application_req(req, chan));
            }
        }

        unsafe {
            timer_handle.remove();
            peer_handle.remove();
            endpoint_handle.remove();
        }
    }

    //
    // Event handlers (called from main_loop(...))
    //
    // They return RaftStateTransition so they can transition into
    // another state based on the contents of the message recieved.
    // (except handle_teardown, which is only called when you have
    // asked for a transition)
    //


    // Called on becoming a given state, so that per-role state can be
    // setup. By this time, self.current_state will have the right
    // value.
    fn handle_setup(&mut self) -> RaftStateTransition {
        // So setup only fires once, without having a loop within a
        // loop.
        if !self.is_setup {
            let res = state_proxy!(leader_setup,
                                   candidate_setup,
                                   follower_setup);
            self.is_setup = true;
            res
        }
        else {
            Continue
        }
    }

    // Called before becoming a new state, so that per-role state can
    // be cleared up. self.current_state will not yet have been
    // changed to its new value.
    fn handle_teardown(&mut self) {
        state_proxy!(leader_teardown,
                     candidate_teardown,
                     follower_teardown)
    }


    // This is called every heartbeat_interval msecs, so that the
    // leader can send out heartbeats to the followers.
    fn handle_heartbeat(&mut self) -> RaftStateTransition {
        if self.is_leader() {
            self.leader_heartbeat()
        }
        else {
            Continue
        }
    }

    // Called upon recieving an AppendEntries RPC Request.
    fn handle_append_entries_req(&mut self,
                                 req: AppendEntriesReq,
                                 chan: Sender<AppendEntriesRes>) -> RaftStateTransition {
        state_proxy!(leader_append_entries_req, 
                     candidate_append_entries_req,
                     follower_append_entries_req,
                     req, chan)
    }

    // Called upon receiving a response to an AppendEntries RPC.
    fn handle_append_entries_res(&mut self, res: AppendEntriesRes) -> RaftStateTransition {
        state_proxy!(leader_append_entries_res,
                     candidate_append_entries_res,
                     follower_append_entries_res,
                     res)
    }

    // Called upon receiving a VoteRequest RPC.
    fn handle_vote_req(&mut self, req: VoteReq, chan: Sender<VoteRes>) -> RaftStateTransition {
        state_proxy!(leader_vote_req,
                     candidate_vote_req,
                     follower_vote_req,
                     req, chan)
    }

    // Called upon receiving Response to a VoteRequest RPC.
    fn handle_vote_res(&mut self, res: VoteRes) -> RaftStateTransition {
        state_proxy!(leader_vote_res,
                     candidate_vote_res,
                     follower_vote_res,
                     res)
    }


    // Called upon receiving a request from a client
    fn handle_application_req(&mut self,
                              req: ClientCmdReq,
                              chan: Sender<ClientCmdRes>) -> RaftStateTransition {
        Continue
        // if self.is_leader() {
        //     pass to application state machine
        // }
        // else {
        //     reply with info on how to talk to the leader
        // }
    }

    //
    // Generic Helper Methods
    //

    fn is_leader(&self) -> bool {
        self.current_state == RaftLeader
    }

}


// STATE MACHINE
/* NOTES
    pass as <~str>
    LockReq -> LockResp(bool, id)  //using Option<int>
    UnlockReq(<id>) -> UnlockResp(bool)

loop {
next_msg = Rec(Msg).recv();
let mut state = Option<id>;
match state {
    None => state = Unlocked;
    Some<id> => state = Locked(id: ---)
}
*/

pub enum State {
    Locked,
    Unlocked
}
pub struct Locked {
    locked: bool,
    state: Option<int>,
    id: Option<int>
}

pub struct Unlocked {
    unlocked: bool,
    state: Option<int>,
    id: Option<int>
}

pub struct LockRes {
    new_state: Option<State>,
    status: bool,
    id: Option<int>
}

pub struct UnlockRes {
    new_state: Option<State>,
    status: bool,
    id: Option<int>
}

/*
impl Unlocked {

    fn lock_request(&mut self) -> LockRes {
        if self.unlocked {
            //update with some deterministic but RANDOM NUMBER
            let result = LockRes { new_state: Some(Locked), status: true, id: Some(1) };         
            //- send lock msg on a Chan 
            //- send "true" and <id> on Chan
        }

        else if !self.unlocked {
            //already locked do nothing
        }
    }

    fn unlock_request(&mut self) -> UnlockRes {
        if self.unlocked {
            //already unlocked
            }
        // else if !self.locked {
        //     Continue //already unlocked do nothing
        // }
    }
}

impl Locked {

    fn unlock_request(&mut self) -> UnlockRes {
        if self.locked {
            match self.id {
                //if unlock request (but different lock <id>) continue / do nothing
                //if unlock request (with same <id>) send unlock msg on Chan + "true" and <id> on Chan
                None => Unlocked,
                Some(id) => Locked,

            }

            let result = UnlockRes{new_state: Some(Unlocked), status: true, id: None};
            result
        }

        else if !self.locked {
            //already unlocked do nothing
        }
    }

    fn lock_request(&mut self) -> LockRes {
        if self.locked {
            //already locked
        }

        // else if !self.locked {
        //     Continue //already locked do nothing
        // }
    }
}
*/


