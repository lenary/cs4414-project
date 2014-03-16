extern crate serialize;
extern crate sync;

use std::comm::Select;
use std::io::{Acceptor,BufferedReader,InvalidInput,IoError,IoResult,Listener,Timer};
use std::io::timer;
use std::io::net::ip::{Ipv4Addr,SocketAddr};
use std::io::net::tcp::{TcpListener,TcpStream};
use std::str;
use std::sync::atomics::{AtomicBool,AcqRel,INIT_ATOMIC_BOOL};
use std::vec_ng::Vec;

use serialize::json;


// use std::comm::{Empty, Data, Disconnected};

use append_entries::{AppendEntriesRequest,AppendEntriesResponse};
use log::Log;
use log_entry::LogEntry; // should probably be log::entry::LogEntry => MOVE LATER
use serror::{InvalidArgument,InvalidState,SError};

pub mod append_entries;
mod log;
pub mod log_entry;
pub mod serror;

// static DEFAULT_HEARTBEAT_INTERVAL: uint = 50;   // in millis
// static DEFAULT_ELECTION_TIMEOUT  : uint = 150;  // in millis
static STOP_MSG: &'static str = "STOP";

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
    current_term: u64,  // curr_term is already in the log, so why need in both? TODO: remove from one or t'other
	conx_str: ~str,

    priv log: ~Log,  // TODO: should this just be Log (on stack => can it be copied arnd?)

    c: Sender<~Event>,  // TODO: keep chan or port?
    p: Receiver<~Event>,
    // more later
}

pub struct Event {
    msg: ~str,  // just to get started
    // target: ??,
    // return_val: ??,
    ch: Sender<~str>,
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
                if is_stop_msg(ev.msg) {
                    println!("FLW: DEBUG 2");
                    unsafe{ stop.store(true, AcqRel); }
                    self.state = Stopped;
                    break;

                } else {
                    let result = append_entries::decode_append_entries_request(ev.msg);
                    if result.is_err() {
                        fail!("ERROR: Unable to decode msg into append_entry_request: {:?}.\nError is: {:?}", ev.msg, result.err());
                    }
                    let aereq = result.unwrap();

                    let aeresp = match self.log.append_entries(aereq.entries) {
                        Ok(_)  => AppendEntriesResponse{term: self.log.curr_term,
                                                        curr_idx: self.log.curr_idx,
                                                        success: true},
                        Err(e) => {
                            error!("{:?}", e);
                            AppendEntriesResponse{term: self.log.curr_term,
                                                  curr_idx: self.log.curr_idx,
                                                  success: false}
                        }
                    };
                    let jstr = json::Encoder::str_encode(&aeresp);
                    ev.ch.send(jstr);
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

///
/// Expects content-length string of format
///   `Length: NN`
/// where NN is an integer >= 0.
/// Returns the length as a uint or None if the string is not
/// of the specified format.
///
fn parse_content_length(len_line: &str) -> Option<uint> {
    if ! len_line.starts_with("Length:") {
        return None;
    }

    let parts: Vec<&str> = len_line.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let lenstr = parts.get(1).trim();
    from_str::<uint>(lenstr)
}

fn read_network_msg(stream: TcpStream) -> IoResult<~str> {
    let mut reader = BufferedReader::new(stream);

    let length_hdr = try!(reader.read_line());
    let result = parse_content_length(length_hdr);
    if result.is_none() {
        return Err(IoError{kind: InvalidInput,
                           desc: "Length not parsable in network message",
                           detail: Some(format!("length line parsed: {:s}", length_hdr))
                          });
    }
    let length = result.unwrap();
    println!("** CL: {:u}", length);  // TODO: remove

    let mut buf: ~[u8] = std::vec::from_elem(length, 0u8);
    let nread = try!(reader.read(buf));

    if nread != length {
        return Err(IoError{kind: InvalidInput,
                           desc: "Network message read of specified bytes failed",
                           detail: Some(format!("Expected {} bytes, but read {} bytes", length, nread))});
    }

    match str::from_utf8(buf) {
        Some(s) => Ok(s.to_owned()),
        None    => Err(IoError{kind: InvalidInput,
                               desc: "Conversion of Network message from bytes to str failed",
                               detail: None})
    }
}

fn network_listener(conx_str: ~str, chan: Sender<~Event>) {
    let addr = from_str::<SocketAddr>(conx_str).expect("Address error.");
    let mut acceptor = TcpListener::bind(addr).unwrap().listen();
    println!("server <name> listening on {:}", addr);

    let (chsend, chrecv): (Sender<~str>, Receiver<~str>) = channel();

    for stream in acceptor.incoming() {
        println!("NL: DEBUG 0");

        let mut stream = stream.unwrap();

        /////////////
        // TODO: only handling one request at a time for now => spawn threads later?
        match read_network_msg(stream.clone()) {
            Ok(input)  => {
                let ev = ~Event{msg: input.clone(), ch: chsend.clone()};
                chan.send(ev);

                if is_stop_msg(input) {
                    println!("NL: DEBUG 1: was stop msg");
                    unsafe { stop.store(true, AcqRel) }

                } else {
                    println!("NL: sent Event to event-loop; now waiting on response");

                    let resp = chrecv.recv();
                    println!("NL: sending response: {:?}", resp);
                    let result = stream.write_str(resp);
                    if result.is_err() {
                        error!("ERROR: Unable to respond to sender over network: {:?}", result.err());
                    }
                    let _ = stream.flush();
                }
                println!("NL: DEBUG 2b");
            },
            Err(ioerr) => error!("ERROR: {:?}", ioerr)
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
    s == STOP_MSG
}


// this needs to go into test mod
fn test_client(ipaddr: ~str, port: uint) {
    spawn(proc() {
        timer::sleep(1299);
        println!(">>> Client sending AER from 'frank' for widget count");

        let addr = from_str::<SocketAddr>(format!("{:s}:{:u}", ipaddr, port)).unwrap();
        let mut stream = TcpStream::connect(addr);

        let logentry1 = LogEntry {
            index: 17,
            term: 1,
            command_name: ~"inventory.widget.count = 84",
            command: None,
        };
        let logentry2 = LogEntry {
            index: 18,
            term: 1,
            command_name: ~"inventory.widget.count = 83",
            command: None,
        };
        // let logentry3 = LogEntry {
        //     index: 16,
        //     term: 1,
        //     command_name: ~"inventory.widget.count = 85",
        //     command: None,
        // };

        let entries: Vec<LogEntry> = vec!(logentry1, logentry2/*, logentry3*/);
        println!(">>> SIZE {}", entries.len());

        // first send AppendEntriesRequest
        let aereq = ~AppendEntriesRequest{
            term: 0,
            prev_log_idx: 0,
            prev_log_term: 0,
            commit_idx: 0,
            leader_name: ~"frank",
            entries: entries,
        };

        let json_aereq = json::Encoder::str_encode(aereq);
        let req_msg = format!("Length: {:u}\n{:s}", json_aereq.len(), json_aereq);
        let mut result = stream.write_str(req_msg);
        if result.is_err() {
            println!("Client ERROR: {:?}", result.err());
        }
        let _ = stream.flush();
        println!(">>>> Client: message sent to server >> wiating for RESPONSE!");

        // TODO: messages sent will need to include some "EOF" marker => either size or a sentinel "DONE" marker

        match stream.read_to_str() {
            Ok(resp) => println!("vvvvvv Server response to client: {:?}", resp),
            Err(e)   => println!("vvvvvvvServer response error in client {:?}", e)
        }

        drop(stream); // close the connection   ==> NEED THIS? ask on #rust


        // then send stop request
        timer::sleep(2222);
        println!(">>> Client sending STOP client");
        stream = TcpStream::connect(addr);

        let stop_msg = format!("Length: {:u}\n{:s}", "STOP".len(), "STOP");  // TODO: make "STOP" a static constant
        result = stream.write_str(stop_msg);
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

#[cfg(test)]
mod test {
    extern crate serialize;

    use std::io;
    use std::io::{BufferedReader,File};    
    use std::io::fs;
    use std::io::net::ip::SocketAddr;
    use std::io::net::tcp::TcpStream;
    use std::io::timer;
    use std::vec_ng::Vec;

    use serialize::json;

    use append_entries::{AppendEntriesRequest};
    use log_entry::LogEntry; // should probably be log::entry::LogEntry => MOVE LATER

    // mod append_entries;
    // mod log_entry;

    static S1TEST_DIR    : &'static str = "datalog";
    static S1TEST_PATH   : &'static str = "datalog/S1TEST";
    static S1TEST_IPADDR : &'static str = "127.0.0.1";
    static S1TEST_PORT   : uint         = 23158;

    fn setup() -> ~super::Server {
        let name = ~"S1TEST";
        let dirpath = Path::new(S1TEST_DIR);
        let filepath = Path::new(S1TEST_PATH);

        if filepath.exists() {
            let fs_res = fs::unlink(&filepath);
            assert!(fs_res.is_ok());
        }
        if ! dirpath.exists() {
            let fs_res = fs::mkdir(&dirpath, io::UserRWX);
            assert!(fs_res.is_ok());
        }

        let result = super::Server::new(name, filepath, S1TEST_IPADDR.to_owned(), S1TEST_PORT);
        if result.is_err() {
            fail!("{:?}", result.err());
        }
        return result.unwrap();
    }

    fn signal_shutdown() {
        let addr = from_str::<SocketAddr>(format!("{:s}:{:u}", S1TEST_IPADDR, S1TEST_PORT)).unwrap();
        let mut stream = TcpStream::connect(addr);

        let stop_msg = format!("Length: {:u}\n{:s}", "STOP".len(), "STOP");  // TODO: make "STOP" a static constant
        let result = stream.write_str(stop_msg);
        if result.is_err() {
            fail!("Client ERROR: {:?}", result.err());
        }
        drop(stream); // close the connection   ==> NEED THIS? ask on #rust
    }

    fn tear_down() {
        let filepath = Path::new(S1TEST_PATH);
        let _ = fs::unlink(&filepath);
    }
        
    fn launch_server() -> SocketAddr {
        spawn(proc() {
            let mut server = setup();
            match server.run() {
                Ok(_) => (),
                Err(e) => fail!("ERROR: {:?}", e)
            }
        });
        timer::sleep(800); // FIXME this is unstable => how fix?
        from_str::<SocketAddr>(format!("{:s}:{:u}", S1TEST_IPADDR.to_owned(), S1TEST_PORT)).unwrap()
    }

    #[test]
    fn test_follower_with_single_AppendEntryRequest() {
        // launch server => this will not shutdown until a STOP signal is sent
        let addr = launch_server();

        let mut stream = TcpStream::connect(addr);

        /* ---[ prepare and send request ]--- */
        let logentry1 = LogEntry {
            index: 1,
            term: 1,
            command_name: ~"inventory.widget.count = 100",
            command: None,
        };

        let entries: Vec<LogEntry> = vec!(logentry1.clone());
        let aereq = ~AppendEntriesRequest{
            term: 0,
            prev_log_idx: 0,
            prev_log_term: 0,
            commit_idx: 0,
            leader_name: ~"S100TEST",  // TODO: make static
            entries: entries,
        };

        let json_aereq = json::Encoder::str_encode(aereq);
        let req_msg = format!("Length: {:u}\n{:s}", json_aereq.len(), json_aereq);

        let result = stream.write_str(req_msg);
        if result.is_err() {
            println!("Client ERROR: {:?}", result.err());
        }
        let _ = stream.flush();


        /* ---[ read response and signal server to shut down ]--- */
        
        let result2 = stream.read_to_str();
        drop(stream); // close the connection   ==> NEED THIS? ask on #rust
        signal_shutdown();  // TODO: is there a better way to ensure a fn is called if an assert fails?

        /* ---[ validate results ]--- */

        // validate response
        
        assert!(result2.is_ok());
        let resp = result2.unwrap();
        println!("{:?}", resp);

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(1, aeresp.term);
        assert_eq!(1, aeresp.curr_idx);
        assert_eq!(true, aeresp.success);

        // validate that response was written to disk
        let filepath = Path::new(S1TEST_PATH);
        assert!(filepath.exists());

        // read in first entry and make sure it matches the logentry sent in the AEReq
        let mut br = BufferedReader::new(File::open(&filepath));
        let mut readres = br.read_line();
        assert!(readres.is_ok());
        
        let line = readres.unwrap().trim().to_owned();
        let exp_str = json::Encoder::str_encode(&logentry1);
        assert_eq!(exp_str, line);

        // should only be one entry in the file
        readres = br.read_line();
        assert!(readres.is_err());
        
        tear_down();   // TODO: is there a better way to ensure a fn is called if an assert fails?
    }

    #[test]
    fn test_follower_with_same_AppendEntryRequest_twice_should_return_false_2nd_time() {
        assert!(true);
    }
}
