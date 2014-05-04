#![feature(phase,globs)]
#![feature(macro_rules)]
#![feature(trace_macros, concat_idents)]

#[phase(syntax, link)]
extern crate log;
extern crate rand;
extern crate serialize;
extern crate collections;
extern crate sync;
extern crate uuid;

extern crate regex;
#[phase(syntax)] extern crate regex_macros;


use std::comm::Select;
// use std::comm::{Empty, Data, Disconnected};
use std::{cmp};
use std::vec::Vec;
use std::io::Timer;

// TODO: None of this should be in this module. It should go into net
// (I believe they are all to do with communicating with Peers)
use std::io::IoResult;
use rand::{task_rng,Rng};
use sync::TaskPool;

use self::consistent_log::{Log,LogEntry};
use self::net::*;
use events::{ClientCmdReq, ClientCmdRes};
use self::events::append_entries::{AppendEntriesReq,AppendEntriesRes};
use self::server::RaftServer;

mod events;
mod consistent_log;
mod net;

mod server;
mod leader;
mod candidate;
mod follower;

fn main() {
    let (endp_send, endp_recv): (Sender<(ClientCmdReq, Sender<ClientCmdRes>)>,
                                 Receiver<(ClientCmdReq, Sender<ClientCmdRes>)>) = channel();
    let (sm_send, sm_recv): (Sender<(ClientCmdReq, Sender<ClientCmdRes>)>,
                             Receiver<(ClientCmdReq, Sender<ClientCmdRes>)>) = channel();
    spawn(proc() {
        // Stupid dummy state machine
        loop {
            sm_recv.recv();
        }
    });
    let mut server = RaftServer::new();
    server.spawn(sm_send, endp_recv);
}
