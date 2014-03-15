extern crate serialize;
extern crate sync;

use std::comm::Select;
use std::io::{Acceptor,InvalidInput,IoError,IoResult,Listener,Timer};
use std::io::timer;
use std::io::net::ip::{Ipv4Addr,SocketAddr};
use std::io::net::tcp::{TcpListener,TcpStream};
use std::sync::atomics::{AtomicBool,AcqRel,INIT_ATOMIC_BOOL};
use std::vec_ng::Vec;

use serialize::json;


// use std::comm::{Empty, Data, Disconnected};

use append_entries_request::AppendEntriesRequest;
use log::Log;
use log_entry::LogEntry; // should probably be log::entry::LogEntry => MOVE LATER
use serror::{InvalidArgument,InvalidState,SError};

mod append_entries_request;
mod log;
mod log_entry;
pub mod serror;

// static DEFAULT_HEARTBEAT_INTERVAL: uint = 50;   // in millis
// static DEFAULT_ELECTION_TIMEOUT  : uint = 150;  // in millis

static mut stop: AtomicBool = INIT_ATOMIC_BOOL;

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
    ip: ~str,
    tcpport: uint,
    name: ~str,
    path: Path,
    state: State,
    current_term: u64,
	conx_str: ~str,

    priv log: ~Log,  // TODO: should this just be Log (on stack => can it be copied arnd?)

    c: Sender<~Event>,  // TODO: keep chan or port?
    p: Receiver<~Event>,
    // more later
}

pub struct Event {
    msg: ~str,  // bogus => just to get started
    // target: ??,
    // return_val: ??,
    // c: Sender<SError>,   // TODO: chan or port?  // TODO: what errors?
}

/* ---[ functions ]--- */

impl Server {
    pub fn new(name: ~str, logpath: Path, ipaddr: ~str, tcpport: uint) -> IoResult<~Server> {
        
        if name == ~"" {
            return Err(IoError{
                kind: InvalidInput,
                desc: "server name cannot be blank",
                detail: None,
            });
        }
        let (ch, pt): (Sender<~Event>, Receiver<~Event>) = channel();

        let lg = try!(Log::new(logpath.clone()));
        let conx_str = format!("{}:{:u}", &ipaddr, tcpport);
        
        let s = ~Server {
            ip: ipaddr,
            tcpport: tcpport,  // TODO: could we use udp instead? are we doing our own ACKs at the app protocol level?
            name: name,
            path: logpath,
            state: Stopped,
            current_term: 0,
            conx_str: conx_str,  // TODO: what the hell is this for? (from goraft)
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
        let conx_str = self.conx_str.clone();
        spawn(proc() {
            // needs to be a separate file/impl
            network_listener(conx_str, event_chan);
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
        // let mut stopsig = unsafe{ stop.load(AcqRel) };
        let mut timer = Timer::new().unwrap();

        loop {
            println!("FLW: DEBUG 0");
            let timeout = timer.oneshot(1000); // use for detecting lost leader

            let sel = Select::new();
            let mut pt = sel.handle(&self.p);
            let mut timeout = sel.handle(&timeout);
            unsafe{
                pt.add();
                timeout.add();
            }
            let ret = sel.wait();

            if ret == timeout.id() {
                timeout.recv();
                println!("FWL: TIMEOUT!! => change state to Candidate");
                self.state = Candidate;
                break;
                
            } else if ret == pt.id() {
                let ev = pt.recv();
                println!("follower: event message: {}", ev.msg);
                println!("FLW: DEBUG 1 {:?} :: {:?}", ev.msg, is_stop_msg(ev.msg));
                if is_stop_msg(ev.msg) {
                    println!("FLW: DEBUG 2");
                    unsafe{ stop.store(true, AcqRel); }
                    self.state = Stopped;
                    break;
                }
                println!("FLW: DEBUG 3");            
            }
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


fn network_listener(conx_str: ~str, chan: Sender<~Event>) {
    let addr = from_str::<SocketAddr>(conx_str).expect("Address error.");
    let mut acceptor = TcpListener::bind(addr).unwrap().listen();
    println!("server <name> listening on {:}", addr);
    
    for mut stream in acceptor.incoming() {
        println!("NL: DEBUG 0");
        // TODO: only handling one request at a time for now => spawn threads later?
        match stream.read_to_str() {
            Ok(input)  => {
                if is_stop_msg(input) {
                    println!("NL: DEBUG 1");
                    unsafe { stop.store(true, AcqRel) }
                }
                let ev = ~Event{msg: input};
                chan.send(ev);
                println!("NL: DEBUG 2");
            },
            Err(ioerr) => println!("ERROR: {:?}", ioerr)
        }
        unsafe {
            println!("NL: DEBUG 3: {:?}", stop.load(AcqRel));
            if stop.load(AcqRel) {
                println!("NL: DEBUG 4");
                break;
            }
            println!("NL: DEBUG 5");
        }
    }
    
    println!("network listener shutting down ...");
}

fn is_stop_msg(s: &str) -> bool {
    s == "STOP"
}


fn test_client(ipaddr: ~str, port: uint) {
    spawn(proc() {
        timer::sleep(1299);
        println!(">>> Client sending AER from 'frank' for widget count");

        let addr = SocketAddr { ip: Ipv4Addr(127, 0, 0, 1), port: port as u16 };
        let mut stream = TcpStream::connect(addr);

        let logentry = LogEntry {
            index: 1,
            term: 1,
            command_name: ~"inventory.widget.count = 100",
            command: None,
        };

        let entries: Vec<LogEntry> = Vec::from_elem(1, logentry);
        
        // first send AppendEntriesRequest
        let aereq = ~AppendEntriesRequest{
            term: 0,
            prev_log_idx: 0,
            prev_log_term: 0,
            commit_idx: 0,
            leader_name: ~"frank",
            entries: entries,
        };

        let json_aereq: ~str = json::Encoder::str_encode(aereq.entries.get(0));
        
        let mut result = stream.write_str(json_aereq);
        if result.is_err() {
            println!("Client ERROR: {:?}", result.err());
        }
        drop(stream); // close the connection   ==> NEED THIS? ask on #rust
        

        // then send stop request
        timer::sleep(1888);
        stream = TcpStream::connect(addr);
        
        result = stream.write_str("STOP");
        if result.is_err() {
            println!("Client ERROR: {:?}", result.err());
        }
        drop(stream); // close the connection   ==> NEED THIS? ask on #rust
    });
}



fn main() {    
    let name = ~"S1";
    let path = Path::new(~"datalog/S1");
    let ipaddr = ~"127.0.0.1";
    let port = 23158;

    println!("Now starting test client");
    test_client(ipaddr.clone(), port);

    
    let result = Server::new(name, path, ipaddr, port);
    if result.is_err() {
        error!("{:?}", result.err());
        return;
    }
    
    let mut s = result.unwrap();
    match s.run() {
        Ok(_) => (),
        Err(e) => println!("ERROR: {:?}", e)
    }
    
}
