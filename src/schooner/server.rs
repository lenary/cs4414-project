
use super::traits::*;
use super::machine::sm_loop;
use super::events::*;
use super::follower::Follower;

use std::comm::*;
use sync::{duplex, DuplexStream};
use std::io::timer::Timer;

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
//    +-------+-------+
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
//     /    Raft     \
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
struct RaftServer {
    to_app_sm: Option<Sender<RaftAppMsg>>,

    election_timeout:  u64,
    heartbeat_timeout: u64,

    // TODO: store peers... somehow
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
// rs.spawn(to_app_send, from_app_rec, from_endpoint_rec);


impl RaftServer {
    pub fn new(election_timeout: u64) -> RaftServer {
        RaftServer {
            to_app_sm: None,
            election_timeout: election_timeout,
            heartbeat_timeout: (election_timeout/2),
        }
    }

    // TODO: Add Peers
    pub fn add_peer() -> bool {
        false
    }

    fn spawn_peers(&mut self, sender: Sender<RaftPeerMsg>) {
        ()
    }

    pub fn spawn(&mut self, 
                 to_app_sm:         Sender<RaftAppMsg>,
                 from_app_sm:       Receiver<RaftAppMsg>,
                 from_app_endpoint: Receiver<RaftAppMsg>) {
        
        self.to_app_sm = Some(to_app_sm);

        let (from_peers_send, from_peers_rec) = channel();
        self.spawn_peers(from_peers_send);

        let follower = Follower::new(); // TODO: real follower with
        // self inside

        let heartbeat_timeout = self.heartbeat_timeout;

        spawn(proc() {
            let mut timer = Timer::new().unwrap();
            let timer_rec : Receiver<()> = timer.periodic(heartbeat_timeout);

            // some other way to encapsulate self?
            sm_loop(follower,
                    &timer_rec,
                    &from_peers_rec,
                    &from_app_endpoint,
                    &from_app_sm);
        });
    }

}
