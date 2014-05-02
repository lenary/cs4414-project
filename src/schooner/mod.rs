#![feature(phase,globs)]
#![feature(macro_rules)]
#![feature(trace_macros, concat_idents)]

#[phase(syntax, link)]
extern crate log;
extern crate rand;
extern crate serialize;
extern crate sync;
extern crate uuid;

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
use self::events::traits::RaftEvent;

mod events;
mod consistent_log;
mod net;

mod server;
mod leader;
mod candidate;
mod follower;

fn main() {
    spawn(proc() {
        let (send, recv): (Sender<(ClientCmdReq, Sender<ClientCmdRes>)>,
                           Receiver<(ClientCmdReq, Sender<ClientCmdRes>)>) = channel();
        loop {
            recv.recv();
        }
    });
}
