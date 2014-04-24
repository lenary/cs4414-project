
use super::traits::*;
use super::events::*;
use super::follower::Follower;

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
//    +task-----------+
//    |  Application  |
//    |     State     |
//    |    Machine    |
//    |               |
//    |       ^       |
//    +---------------+
//            |
//            |<-- RaftAppMsg<AppMsg>
//            |
//            |            RaftPeerMsg<AppMsg>
//    +task---+-------+    |         -------
//    |       v       |    |      --/       \--
//    |               |    v     /    Raft     \
//    |     Raft     <+----------+>   Peer     |
//    |               |          \    Tasks    /
//    |       ^       |           --\       /--
//    +-------+-------+              -------
//            |
//            |<-- RaftAppMsg<AppMsg>
//         ---+---
//      --/   v   \--
//     / Application \
//     |   Endpoint  |
//     \    Tasks    /
//      --\   ^   /--
//         ---+---
//            |
//            |
//            v
//       Application
//         Clients
//

// TODO: Parameterise by <AppMsg>, the type of messages that the app
// uses
pub struct RaftServer {
    election_timeout:  u64,
    heartbeat_interval: u64,

    // TODO: store peers... somehow
    peer_configs: Vec<()>,
}

// static duplex to Application State Machine (from somewhere else)
// static reciever from Application Endpoint  (
// static reciever from Peer

// Some ideas of how to use with Application Tasks:
//
// let (to_app_send, to_app_rec) = channel();
// let (from_app_send, from_app_rec) = channel();
//
// spawn_app_task(to_app_rec, from_app_send);
//
// let (from_endpoint_send, from_endpoint_rec) = channel();
//
// spawn_main_app_endpoint_task(from_endpoint_send);
//
// rs = RaftServer::new(...opts...); // TODO
// rs.add_peer(...peercfg...); // TODO
// RaftServer::spawn(rs, to_app_send, from_app_rec, from_endpoint_rec);
//


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
    pub fn spawn_peer_tasks(&mut self, sender: Sender<RaftPeerMsg>) -> Vec<()> {
        Vec::new()
    }

    pub fn spawn(&mut self,
                 to_app_sm:         Sender<RaftAppMsg>,
                 from_app_sm:       Receiver<RaftAppMsg>,
                 from_app_endpoint: Receiver<RaftAppMsg>) {

        let (from_peers_send, from_peers_rec) = channel();
        let peers = self.spawn_peer_tasks(from_peers_send);

        let heartbeat_interval = self.heartbeat_interval;

        spawn(proc() {

            let follower = Follower::new();
            let mut server_state = RaftServerState::new(RaftFollower(follower),
                                                        to_app_sm,
                                                        peers);

            let mut timer = Timer::new().unwrap();
            let timer_rec : Receiver<()> = timer.periodic(heartbeat_interval);

            // some other way to encapsulate self?
            server_state.main_loop(&timer_rec,
                                   &from_peers_rec,
                                   &from_app_endpoint,
                                   &from_app_sm);
        });
    }
}

struct RaftServerState {
    current_state: RaftNextState,
    is_setup:      bool,

    to_app_sm:     Sender<RaftAppMsg>,

    peers:         Vec<()>,
}

impl RaftServerState {
    fn new(current_state: RaftNextState,
           to_app_sm: Sender<RaftAppMsg>,
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

    fn main_loop(&mut self,
                 timer_rec: &Receiver<()>,
                 peer_rec:  &Receiver<RaftPeerMsg>,
                 endpoint_rec: &Receiver<RaftAppMsg>,
                 app_rec: &Receiver<RaftAppMsg>) {

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
        let mut app_handle      = select.handle(app_rec);

        unsafe {
            timer_handle.add();
            peer_handle.add();
            endpoint_handle.add();
            app_handle.add();
        }

        loop {
            may_transition!(self.handle_setup());

            let ready_id = select.wait();
            if timer_handle.id() == ready_id {
                timer_handle.recv();
                may_transition!(self.handle_timeout());
            }
            else if peer_handle.id() == ready_id {
                // An Event Message from a Peer
                let event : RaftPeerMsg = peer_handle.recv();
                match event {
                    ARQ(ae_req) =>
                        may_transition!(self.handle_append_entries_req(ae_req)),
                    ARS(ae_res) =>
                        may_transition!(self.handle_append_entries_res(ae_res)),
                    VRQ(vote_req) =>
                        may_transition!(self.handle_vote_req(vote_req)),
                    VRS(vote_res) =>
                        may_transition!(self.handle_vote_res(vote_res)),
                    HRQ(handoff_req) =>
                        may_transition!(self.handle_handoff_req(handoff_req)),
                    HRS(handoff_res) =>
                        may_transition!(self.handle_handoff_res(handoff_res)),
                    StopReq => {
                        may_transition!(Stop);
                    }
                }
            }
            else if endpoint_handle.id() == ready_id {
                // An Event Message from an Endpoint
                let event : RaftAppMsg = endpoint_handle.recv();
                match event {
                    APRQ(app_req) =>
                        may_transition!(self.handle_application_req(app_req)),
                    APRS(app_res) =>
                        may_transition!(self.handle_application_res(app_res)),
                }
            }
            else if app_handle.id() == ready_id {
                // An Event Message from the Application State Machine Task
                let event : RaftAppMsg = app_handle.recv();
                match event {
                    APRQ(app_req) =>
                        may_transition!(self.handle_application_req(app_req)),
                    APRS(app_res) =>
                        may_transition!(self.handle_application_res(app_res)),
                }
            }
        }

        unsafe {
            timer_handle.remove();
            peer_handle.remove();
            endpoint_handle.remove();
            app_handle.remove();
        }
    }
}

impl RaftState for RaftServerState {
    // TODO: the rest
    fn handle_setup(&mut self) -> RaftStateTransition {
        if !self.is_setup {
            let res = self.current_state.handle_setup();
            self.is_setup = true;
            res
        }
        else {
            Continue
        }
    }

    fn handle_teardown(&mut self) {
        self.current_state.handle_teardown()
    }

    fn handle_timeout(&mut self) -> RaftStateTransition {
        self.current_state.handle_timeout()
    }

    fn handle_append_entries_req(&mut self, req: AppendEntriesReq) -> RaftStateTransition {
        self.current_state.handle_append_entries_req(req)
    }

    fn handle_append_entries_res(&mut self, res: AppendEntriesRes) -> RaftStateTransition {
        self.current_state.handle_append_entries_res(res)
    }

    fn handle_application_req(&mut self, _req: ApplicationReq) -> RaftStateTransition {
        Continue

        // if self.is_leader() {
        //     pass to application state machine
        // }
        // else {
        //     create handoff request for leader
        // }
    }

    fn handle_application_res(&mut self, _res: ApplicationRes) -> RaftStateTransition {
        Continue

        // if res.was_handoff() {
        //     // wrap as a handoff res and hand it back to the peer the
        //     // handoff req came from
        // }
        // else {
        //     // send back to client
        // }
    }

    fn handle_handoff_req(&mut self, _req: HandoffReq) -> RaftStateTransition {
        Continue

        // if self.is_leader() {
        //     // turn req into ApplicationReq
        //     // self.handle_application_request(new_req)
        // }
        // else {
        //     // send the handoff req to who you think is the leader
        // }
    }

    fn handle_handoff_res(&mut self, _res: HandoffRes) -> RaftStateTransition {
        Continue

        // if res.for_me() {
        //     // unwrap it as an ApplicationRes and send it to the
        //     // client that wanted it
        // }
        // else {
        //     // send it to where the handoff should be going
        // }
    }
}
