extern crate serialize;
extern crate sync;

use std::comm::Data;
use std::io::{InvalidInput,IoError,IoResult};
use std::sync::atomics::{AtomicBool,AcqRel,INIT_ATOMIC_BOOL};
    
use sync::{SyncChan, SyncPort};
// use std::comm::{Empty, Data, Disconnected};

use log::Log;
use serror::{InvalidArgument,InvalidState,SError};

mod log;
mod log_entry;
pub mod serror;

// static DEFAULT_HEARTBEAT_INTERVAL: uint = 50;   // in millis
// static DEFAULT_ELECTION_TIMEOUT  : uint = 150;  // in millis

static mut ab: AtomicBool = INIT_ATOMIC_BOOL;

/* ---[ data structures ]--- */

#[deriving(Clone, Eq)]
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

    priv log: ~Log,  // TODO: should this just be Log (on stack => can it be copied arnd?)

    c: Chan<~Event>,  // TODO: keep chan or port?
    p: Port<~Event>,
    // more later
}

pub struct Event {
    msg: ~str,  // bogus => just to get started
    // target: ??,
    // return_val: ??,
    // c: Chan<SError>,   // TODO: chan or port?  // TODO: what errors?
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
        let (pt, ch): (Port<~Event>, Chan<~Event>) = Chan::new();

        let lg = try!(Log::new(logpath.clone()));
        
        let s = ~Server {
            name: name,
            path: logpath,
            state: Stopped,
            current_term: 0,
            conx_str: connection_str,
            log: lg,
            c: ch,
            p: pt,                
        };

        Ok(s)
    }

    ///
    /// Central method that sets things up and then runs the server threads/tasks
    /// 
    pub fn run(&mut self) -> Result<(), SError> {
        if self.state != Stopped {
            return Err(InvalidState(~"schooner.Server: Server already running"));
        }

        self.current_term = self.log.curr_term;
        self.state = Follower;

        let event_chan = self.c.clone();
        spawn(proc() {
            // needs to be a separate file/impl
            network_listener(event_chan);
        });
        
        self.serve_loop();

        Ok(())
    }

    fn serve_loop(&mut self) {
        println!("Now serving => loop until Stopped");
        loop {
            match self.state {
                Follower     => self.follower_loop(),
                Candidate    => self.candidate_loop(),
                Leader       => self.leader_loop(),
                Snapshotting => self.snapshotting_loop(),
                Stopped      => break
            }
            std::io::timer::sleep(1);            
        }
        println!("Serve_loop END");        
    }

    fn follower_loop(&mut self) {
        let stopsig = unsafe{ ab.load(AcqRel) };
        println!("follower loop; stop signal is: {:?}", stopsig);
        match self.p.try_recv() {
            Data(ev) => println!("event message: {}", ev.msg),
            _ => ()
        }
        if stopsig {
            self.state = Stopped;
        } else {
            self.state = Candidate;
        }
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
        self.state = Follower;
    }
}


fn network_listener(chan: Chan<~Event>) {
    let ev = ~Event{msg: ~"hi there"};
    chan.send(ev);
    println!("network listener starting up ...");

    std::io::timer::sleep(50);
    let ev = ~Event{msg: ~"last msg"};
    chan.send(ev);

    unsafe{ ab.store(true, AcqRel); }
    println!("network listener: set stop to {}", unsafe{ ab.load(AcqRel) });
    
    println!("network listener shutting down ...");
}


fn main() {
    let name = ~"S1";
    let path = Path::new(~"datalog/S1");
    let conx = ~"127.0.0.1:7007";
    match Server::new(name, path, conx) {
        Ok(mut s) => { s.run(); },
        Err(e)    => { error!("{:?}", e); }
    }
}
