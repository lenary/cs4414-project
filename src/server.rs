#![feature(phase)]
#[phase(syntax, link)]
extern crate log;
extern crate serialize;
extern crate sync;
extern crate uuid;

use std::comm::Select;
use std::io::{Acceptor,BufferedReader,InvalidInput,IoError,IoResult,Listener,Timer};
use std::io::net::ip::SocketAddr;
use std::io::net::tcp::{TcpListener,TcpStream};
use std::str;
use std::sync::atomics::{AtomicBool,AcqRel,INIT_ATOMIC_BOOL};
use std::vec::Vec;

use serialize::json;


// use std::comm::{Empty, Data, Disconnected};

use schooner::append_entries;
use schooner::append_entries::AppendEntriesResponse;
use schooner::log::Log;
use serror::{InvalidState,SError};

pub mod schooner;
pub mod serror;  // TODO: move to schooner dir

// static DEFAULT_HEARTBEAT_INTERVAL: uint = 50;   // in millis
// static DEFAULT_ELECTION_TIMEOUT  : uint = 150;  // in millis
static STOP_MSG: &'static str = "STOP";

// TODO: this needs to be removed as global state => so can start multiple threads on same machine
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
    id: uint,
    path: Path,         // path to log file (???)  // TODO: path to log is already in the log struct, so why need it here?
    state: State,
    current_term: u64,  // curr_term is already in the log, so why need in both? TODO: remove from one or t'other
	conx_str: ~str,     // TODO: does this need to be stored as state?

    log: ~Log,  // TODO: should this just be Log (on stack => can it be copied arnd?)

    c: Sender<~Event>,  // TODO: keep chan or port?
    p: Receiver<~Event>,
    // more later
}

// TODO: what is this for?
pub struct Event {
    msg: ~str,  // just to get started
    // target: ??,
    // return_val: ??,
    ch: Sender<~str>,
}

/* ---[ functions ]--- */

impl Server {
    pub fn new(id: uint, logpath: Path, ipaddr: ~str, tcpport: uint) -> IoResult<~Server> {

        let (ch, pt): (Sender<~Event>, Receiver<~Event>) = channel();
        let lg = try!(Log::new(logpath.clone()));
        let conx_str = format!("{}:{:u}", &ipaddr, tcpport);

        let s = ~Server {
            ip: ipaddr,
            tcpport: tcpport,  // TODO: could we use udp instead? are we doing our own ACKs at the app protocol level?
            id: id,
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

        self.current_term = self.log.term;
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

                    let aeresp = match self.log.append_entries(&aereq) {
                        Ok(_)  => AppendEntriesResponse{success: true,
                                                        term: self.log.term,
                                                        idx: self.log.idx,
                                                        commit_idx: self.log.idx},   // FIXME: wrong for now
                        Err(e) => {
                            error!("******>>>>>>>>>>>>>> {:?}", e);
                            AppendEntriesResponse{success: false,
                                                  term: self.log.term,
                                                  idx: self.log.idx,
                                                  commit_idx: self.log.idx}   // FIXME: wrong for now
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

    // TODO: this is probably not the right (or long-term) way to handle this => ask on IRC
    let mut buf: Vec<u8> = Vec::from_elem(length, 0u8);
    let nread = try!(reader.read(buf.as_mut_slice()));  // FIXME: read needs ~[u8], so how deal with that?

    if nread != length {
        return Err(IoError{kind: InvalidInput,
                           desc: "Network message read of specified bytes failed",
                           detail: Some(format!("Expected {} bytes, but read {} bytes", length, nread))});
    }

    match str::from_utf8(buf.as_slice()) {
        Some(s) => Ok(s.to_owned()),
        None    => Err(IoError{kind: InvalidInput,
                               desc: "Conversion of Network message from bytes to str failed",
                               detail: None})
    }
}

fn network_listener(conx_str: ~str, chan: Sender<~Event>) {
    let addr = from_str::<SocketAddr>(conx_str).expect("Address error.");
    let mut acceptor = TcpListener::bind(addr).unwrap().listen();
    println!("server <id> listening on {:}", addr);  // TODO: need to pass in id?

    let (chsend, chrecv): (Sender<~str>, Receiver<~str>) = channel();

    for stream in acceptor.incoming() {
        println!("NL: DEBUG 0");

        let mut stream = stream.unwrap();

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


fn main() {
    let id = 1;
    let path = Path::new(~"datalog/S1");
    let ipaddr = ~"127.0.0.1";
    let port = 23158;

    let result = Server::new(id, path, ipaddr, port);
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


/* ---[ TESTS ]--- */

#[cfg(test)]
mod test {
    extern crate serialize;

    use std::io;
    use std::io::{BufferedReader,File,IoResult};
    use std::io::fs;
    use std::io::net::ip::SocketAddr;
    use std::io::net::tcp::TcpStream;
    use std::io::timer;
    use std::vec::Vec;
    use std::sync::atomics::AcqRel;

    use serialize::json;
    use uuid::Uuid;

    use schooner::append_entries::{AppendEntriesRequest,APND};
    use schooner::log_entry::LogEntry;

    static S1TEST_DIR    : &'static str = "datalog";
    static S1TEST_PATH   : &'static str = "datalog/S1TEST";
    static S1TEST_IPADDR : &'static str = "127.0.0.1";
    static S1TEST_PORT   : uint         = 23158;

    fn setup() -> ~super::Server {
        unsafe { super::stop.store(false, AcqRel); }

        let id = 1;
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

        let result = super::Server::new(id, filepath, S1TEST_IPADDR.to_owned(), S1TEST_PORT);
        if result.is_err() {
            fail!("{:?}", result.err());
        }
        return result.unwrap();
    }

    fn signal_shutdown() {
        let addr = from_str::<SocketAddr>(format!("{:s}:{:u}", S1TEST_IPADDR, S1TEST_PORT)).unwrap();
        let mut stream = TcpStream::connect(addr);

        // TODO: needs to change to AER with STOP cmd
        let stop_msg = format!("Length: {:u}\n{:s}", "STOP".len(), "STOP");  // TODO: make "STOP" a static constant
        let result = stream.write_str(stop_msg);
        if result.is_err() {
            fail!("Client ERROR: {:?}", result.err());
        }
        drop(stream); // close the connection   ==> NEED THIS? ask on #rust
        timer::sleep(110);
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
                Err(e) => fail!("launch_server: ERROR: {:?}", e)
            }
        });
        timer::sleep(750); // FIXME this is unstable => how fix?
        from_str::<SocketAddr>(format!("{:s}:{:u}", S1TEST_IPADDR.to_owned(), S1TEST_PORT)).unwrap()
    }

    ///
    /// The number of entries in idx_vec and term_vec determines the # of logentries to send to
    /// the server. The len of the two vectors must be the same.
    ///
    fn send_aereqs(stream: &mut IoResult<TcpStream>, idx_vec: Vec<u64>, term_vec: Vec<u64>, prev_log_idx: u64, prev_log_term: u64) -> Vec<LogEntry> {
        if idx_vec.len() != term_vec.len() {
            fail!("send_reqs: size of vectors doesn't match: idx_vec: {}; term_vec: {}", idx_vec.len(), term_vec.len());
        }

        let mut entries = Vec::with_capacity(term_vec.len());
        let mut it = idx_vec.iter().zip(term_vec.iter());

        // loop and create number of LogEntries requested
        for (idx, term) in it {
            let uuidstr = Uuid::new_v4().to_hyphenated_str();
            let entry = LogEntry {
                idx: *idx,
                term: *term,
                data: format!("inventory.widget.count = {}", 100 - *idx as u64),
                uuid: format!("uuid-{}", uuidstr),
            };

            entries.push(entry);
        }

        let aereq = ~AppendEntriesRequest{
            cmd: APND,
            term: *term_vec.get( term_vec.len() - 1 ),
            prev_log_idx: prev_log_idx,
            prev_log_term: prev_log_term,
            commit_idx: 0,           // TODO: need to handle this field
            leader_id: 100,
            entries: entries.clone(),
        };

        let json_aereq = json::Encoder::str_encode(aereq);
        let req_msg = format!("Length: {:u}\n{:s}", json_aereq.len(), json_aereq);

        let result = stream.write_str(req_msg);
        if result.is_err() {
            println!("Client ERROR: {:?}", result.err());
        }
        let _ = stream.flush();

        entries
    }

    // meant for sending a single AERequest
    fn send_aereq1(stream: &mut IoResult<TcpStream>) -> LogEntry {
        /* ---[ prepare and send request ]--- */
        let logentry1 = LogEntry {
            idx: 1,
            term: 1,
            data: ~"inventory.widget.count = 100",
            uuid: ~"uuid-999"
        };

        let entries: Vec<LogEntry> = vec!(logentry1.clone());
        let aereq = ~AppendEntriesRequest{
            cmd: APND,
            term: 0,
            prev_log_idx: 0,
            prev_log_term: 0,
            commit_idx: 0,
            leader_id: 100,
            entries: entries,
        };

        let json_aereq = json::Encoder::str_encode(aereq);
        let req_msg = format!("Length: {:u}\n{:s}", json_aereq.len(), json_aereq);

        ////
        let result = stream.write_str(req_msg);
        if result.is_err() {
            println!("Client ERROR: {:?}", result.err());
        }
        let _ = stream.flush();
        logentry1
    }

    fn num_entries_in_log() -> uint {
        let p = Path::new(S1TEST_PATH);
        let f = File::open(&p);
        let mut br = BufferedReader::new(f);
        let mut count: uint = 0;
        for _ in br.lines() {
            count += 1;
        }
        count
    }

    /* ---[ tests ]--- */

    #[test]
    fn test_follower_with_single_AppendEntryRequest() {
        // launch server => this will not shutdown until a STOP signal is sent
        let addr = launch_server();
        let mut stream = TcpStream::connect(addr);
        let logentry1 = send_aereq1(&mut stream);

        /* ---[ read response and signal server to shut down ]--- */

        let result1 = stream.read_to_str();
        drop(stream); // close the connection

        signal_shutdown();  // TODO: is there a better way to ensure a fn is called if an assert fails?

        /* ---[ validate results ]--- */

        // validate response

        assert!(result1.is_ok());
        let resp = result1.unwrap();
        println!("{:?}", resp);

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(1, aeresp.term);
        assert_eq!(1, aeresp.idx);
        // TODO: need to test aeresp.commit_idx
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
        // launch server => this will not shutdown until a STOP signal is sent
        let addr = launch_server();
        let mut stream = TcpStream::connect(addr);

        /* ---[ send logentry1 (term1) ]--- */

        let logentry1 = send_aereq1(&mut stream);
        let result1 = stream.read_to_str();
        drop(stream); // close the connection


        /* ---[ send logentry1 again (term1) ]--- */

        stream = TcpStream::connect(addr);
        let _ = send_aereq1(&mut stream);
        let result2 = stream.read_to_str();
        drop(stream); // close the connection


        signal_shutdown();  // TODO: is there a better way to ensure a fn is called if an assert fails?

        /* ---[ validate results ]--- */

        // validate response => result 1 should be success = true

        assert!(result1.is_ok());
        let resp = result1.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(1, aeresp.term);
        assert_eq!(1, aeresp.idx);
        // TODO: need to test commit_idx
        assert_eq!(true, aeresp.success);


        // result 2 should be success = false

        assert!(result2.is_ok());
        let resp2 = result2.unwrap();

        assert!(resp2.contains("\"success\":false"));
        let aeresp2 = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(1, aeresp2.term);
        assert_eq!(1, aeresp2.idx);
        // TODO: need to test commit_idx
        assert_eq!(true, aeresp2.success);


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
    fn test_follower_with_multiple_valid_AppendEntryRequests() {
        // launch server => this will not shutdown until a STOP signal is sent
        let addr = launch_server();
        let mut stream = TcpStream::connect(addr);
        let terms: Vec<u64> = vec!(1, 1, 1, 1);
        let indexes: Vec<u64> = vec!(1, 2, 3, 4);
        // let prev_log_idx = 0u64;  // TODO: does this need to be used?
        let prev_log_term = 0u64;
        let logentries: Vec<LogEntry> = send_aereqs(&mut stream, indexes, terms, prev_log_term, prev_log_term);

        /* ---[ read response and signal server to shut down ]--- */

        let result1 = stream.read_to_str();
        drop(stream); // close the connection   ==> NEED THIS? ask on #rust

        signal_shutdown();  // TODO: is there a better way to ensure a fn is called if an assert fails?

        /* ---[ validate ]--- */
        assert_eq!(4, logentries.len());

        assert!(result1.is_ok());
        let resp = result1.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(1, aeresp.term);
        assert_eq!(4, aeresp.idx);
        // TODO: need to test commit_idx
        assert_eq!(true, aeresp.success);

        tear_down();   // TODO: is there a better way to ensure a fn is called if an assert fails?
    }

    #[test]
    fn test_follower_with_AppendEntryRequests_with_invalid_term() {
        // launch server => this will not shutdown until a STOP signal is sent
        let addr = launch_server();
        let mut stream = TcpStream::connect(addr);

        /* ---[ AER 1, valid, initial ]--- */
        let mut terms: Vec<u64> = vec!(1);
        let mut indexes: Vec<u64> = vec!(1);
        let prev_log_idx = 0u64;
        let prev_log_term = 0u64;
        let logentries1: Vec<LogEntry> = send_aereqs(&mut stream, indexes, terms, prev_log_idx, prev_log_term);
        let result1 = stream.read_to_str();
        drop(stream); // close the connection


        /* ---[ AER 2: valid, term increment ]--- */
        stream = TcpStream::connect(addr);
        terms = vec!(2, 2);
        indexes = vec!(2, 3);
        let prev_log_idx = 1u64;
        let prev_log_term = 1u64;
        let logentries2: Vec<LogEntry> = send_aereqs(&mut stream, indexes, terms, prev_log_idx, prev_log_term);
        let result2 = stream.read_to_str();
        drop(stream); // close the connection


        /* ---[ AER 3: invalid, term in past ]--- */
        stream = TcpStream::connect(addr);
        terms = vec!(1);
        indexes = vec!(4);
        let prev_log_idx = 3u64;
        let prev_log_term = 2u64;
        let logentries3: Vec<LogEntry> = send_aereqs(&mut stream, indexes, terms, prev_log_idx, prev_log_term);
        let result3 = stream.read_to_str();
        drop(stream); // close the connection

        signal_shutdown();  // TODO: is there a better way to ensure a fn is called if an assert fails?

        /* ---[ validate ]--- */

        assert_eq!(1, logentries1.len());
        assert_eq!(2, logentries2.len());
        assert_eq!(1, logentries3.len());

        // result 1
        assert!(result1.is_ok());
        let resp = result1.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(1, aeresp.term);
        assert_eq!(1, aeresp.idx);
        // TODO: need to test commit_idx

        // result 2
        assert!(result2.is_ok());
        let resp = result2.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(3, aeresp.idx);
        // TODO: need to test commit_idx

        // result 3
        assert!(result3.is_ok());
        let resp = result3.unwrap();

        assert!(resp.contains("\"success\":false"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(false, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(3, aeresp.idx);
        // TODO: need to test commit_idx

        tear_down();   // TODO: is there a better way to ensure a fn is called if an assert fails?
    }

    #[test]
    fn test_follower_with_AppendEntryRequests_with_invalid_prevLogTerm() {
        // launch server => this will not shutdown until a STOP signal is sent
        let addr = launch_server();
        let mut stream = TcpStream::connect(addr);

        /* ---[ AER 1, valid, initial ]--- */
        let mut terms: Vec<u64> = vec!(1,1);
        let mut indexes: Vec<u64> = vec!(1,2);
        let prev_log_idx = 0u64;
        let prev_log_term = 0u64;
        let logentries1: Vec<LogEntry> = send_aereqs(&mut stream, indexes, terms, prev_log_idx, prev_log_term);
        let result1 = stream.read_to_str();
        drop(stream); // close the connection

        let num_entries_logged1 = num_entries_in_log();

        /* ---[ AER 2: invalid, term increment ]--- */
        stream = TcpStream::connect(addr);
        terms = vec!(2, 2);
        indexes = vec!(2, 3);
        let prev_log_idx = 2u64;
        let prev_log_term = 2u64; // NOTE: this is what's being tested: should be 1, not 2
        let logentries2: Vec<LogEntry> = send_aereqs(&mut stream, indexes, terms, prev_log_idx, prev_log_term);
        let result2 = stream.read_to_str();
        drop(stream); // close the connection

        let num_entries_logged2 = num_entries_in_log();

        /* ---[ AER 3: same as AER2, now valid (after truncation) ]--- */
        stream = TcpStream::connect(addr);
        terms = vec!(2, 2);
        indexes = vec!(2, 3);
        let prev_log_idx = 1u64;
        let prev_log_term = 1u64; // NOTE: this switched to 1 bcs the leader agrees that t1_idx1 is correct
        let logentries3: Vec<LogEntry> = send_aereqs(&mut stream, indexes, terms, prev_log_idx, prev_log_term);
        let result3 = stream.read_to_str();
        drop(stream); // close the connection

        let num_entries_logged3 = num_entries_in_log();

        signal_shutdown();  // TODO: is there a better way to ensure a fn is called if an assert fails?

        /* ---[ validate ]--- */

        // number sent
        assert_eq!(2, logentries1.len());
        assert_eq!(2, logentries2.len());
        assert_eq!(2, logentries3.len());

        // number in logfile after each AER
        assert_eq!(2, num_entries_logged1);
        assert_eq!(1, num_entries_logged2);  // one entry truncated bcs t1_idx2 != t2_idx2
        assert_eq!(3, num_entries_logged3);  // two added

        // result 1
        assert!(result1.is_ok());
        let resp = result1.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(1, aeresp.term);
        // TODO: need to test commit_idx
        assert_eq!(2, aeresp.idx);

        // result 2
        assert!(result2.is_ok());
        let resp = result2.unwrap();

        assert!(resp.contains("\"success\":false"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(false, aeresp.success);
        assert_eq!(1, aeresp.term);
        assert_eq!(1, aeresp.idx);

        // result 3
        assert!(result3.is_ok());
        let resp = result3.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(3, aeresp.idx);

        tear_down();   // TODO: is there a better way to ensure a fn is called if an assert fails?
    }

}
