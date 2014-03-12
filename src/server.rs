extern crate sync;

use std::io::IoResult;
use std::io::IoError;
use std::io::InvalidInput;
use sync::{SyncChan, SyncPort};
// use std::comm::{Empty, Data, Disconnected};
use serror::{InvalidArgument,InvalidState,SError};

mod serror;

// static DEFAULT_HEARTBEAT_INTERVAL: uint = 50;  // in millis
// static DEFAULT_ELECTION_TIMEOUT  : uint = 150;  // in millis

/* ---[ data structures ]--- */

#[deriving(Eq)]
pub enum State {
    Stopped,
    Follower,
    Candidate,
    Leader,
    Snapshotting
}

pub struct Server {
    name: ~str,
    path: Path,
    state: State,
    current_term: u64,
	conx_str: ~str,

    stopped_chan: SyncChan<()>,  // chan/port for messaging a stop signal
    stopped_port: SyncPort<()>,
    
    c: Chan<~Event>,  // TODO: keep chan or port?
    p: Port<~Event>,  // FIXME: temp keep both ends of the channel
    // more later
}

pub struct Event {
    // target: ??,
    // return_val: ??,
    c: Chan<SError>,   // TODO: chan or port?  // TODO: what errors?
}

/* ---[ functions ]--- */

impl Server {
    pub fn new(name: ~str, logpath: Path,
               /*transporter, */ /*statemachine,*/ /*ctx: ~T,*/
               connection_str: ~str) -> Result<~Server, SError> {

        if name == ~"" {
            return Err(InvalidArgument(~"server name cannot be blank"));
        }
        // TODO: what are we going to do with pt => who gets it?
        let (pt, ch): (Port<~Event>, Chan<~Event>) = Chan::new();
        let (stopp, stopc): (SyncPort<()>, SyncChan<()>) = sync::rendezvous();

        let s = ~Server {
            name: name,
            path: logpath,
            state: Stopped,
            current_term: 0,
            conx_str: connection_str,
            stopped_chan: stopc,
            stopped_port: stopp,
            c: ch,
            p: pt,                
        };

        Ok(s)
    }

    // TODO: need to return some type of error?
    pub fn start(&mut self) -> Result<(), SError> {
        if self.state != Stopped {
            return Err(InvalidState(~"schooner.Server: Server already running"));
        }

        

        self.state = Follower;
        Ok(())
    }
}


fn main() {
    let name = ~"S1";
    let path = Path::new(~"log/S1");
    let conx = ~"127.0.0.1:7007";
    let s = Server::new(name, path, conx);
    println!("{:?}", s);
}
