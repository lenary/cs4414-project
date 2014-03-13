extern crate serialize;
extern crate sync;

use std::io::{InvalidInput,IoError,IoResult};
use sync::{SyncChan, SyncPort};
// use std::comm::{Empty, Data, Disconnected};

use log::Log;
use serror::{InvalidArgument,InvalidState,SError};

mod log;
mod log_entry;
pub mod serror;

// static DEFAULT_HEARTBEAT_INTERVAL: uint = 50;   // in millis
// static DEFAULT_ELECTION_TIMEOUT  : uint = 150;  // in millis

/* ---[ data structures ]--- */

#[deriving(Clone, Eq)]
pub enum State {
    Stopped,
    Follower,
    Candidate,
    Leader,
    Snapshotting
}

//#[deriving(Clone)]
pub struct Server {
    name: ~str,
    path: Path,
    state: State,
    current_term: u64,
	conx_str: ~str,

    priv log: ~Log,  // TODO: should this just be Log (on stack => can it be copied arnd?)
    
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
               connection_str: ~str) -> IoResult<~Server> {

        if name == ~"" {
            return Err(IoError{
                kind: InvalidInput,
                desc: "server name cannot be blank",
                detail: None,
            });
        }
        // TODO: what are we going to do with pt => who gets it?
        let (pt, ch): (Port<~Event>, Chan<~Event>) = Chan::new();
        let (stopp, stopc): (SyncPort<()>, SyncChan<()>) = sync::rendezvous();

        let lg = try!(Log::new(logpath.clone()));
        
        let s = ~Server {
            name: name,
            path: logpath,
            state: Stopped,
            current_term: 0,
            conx_str: connection_str,
            log: lg,
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

        self.current_term = self.log.curr_term;
        self.state = Follower;

        // let server_clone = self.clone();
        // spawn(proc() {
        //     server_clone.serve_loop();
        // });

        Ok(())
    }

    fn serve_loop(&mut self) {
        // LEFT OFF
        println!("Now serving => put loop here");
        loop {
            match self.state {
                Follower     => self.follower_loop(),
                Candidate    => self.candidate_loop(),
                Leader       => self.leader_loop(),
                Snapshotting => self.snapshotting_loop(),
                Stopped      => break
            }
        }
    }

    fn follower_loop(&mut self) {
        println!("follower loop");
        self.state = Candidate;
    }
    fn candidate_loop(&mut self) {
        println!("candidate loop");
        self.state = Leader;
    }
    fn leader_loop(&mut self) {
        println!("leader loop");
        self.state = Snapshotting;
    }
    fn snapshotting_loop(&mut self) {
        println!("snapshotting loop");
        self.state = Stopped;
    }
}

// fn serve_loop(s: )

fn main() {
    let name = ~"S1";
    let path = Path::new(~"log/S1");
    let conx = ~"127.0.0.1:7007";
    match Server::new(name, path, conx) {
        Ok(mut s) => { s.start(); },
        Err(e)    => { error!("{:?}", e); }
    }
}
