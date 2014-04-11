#![feature(phase)]
extern crate collections;
#[phase(syntax, link)]
extern crate log;
extern crate rand;
extern crate serialize;
extern crate sync;
extern crate uuid;

use std::comm::Select;
use std::{cmp,str};
use std::io::{Acceptor,BufferedReader,InvalidInput,IoError,IoResult,Listener,Timer};
use std::io::net::ip::SocketAddr;
use std::io::net::tcp::{TcpListener,TcpStream};
use std::vec::Vec;

use collections::hashmap::HashMap;
use rand::{task_rng, Rng};
use serialize::json;
use uuid::Uuid;


// use std::comm::{Empty, Data, Disconnected};

use schooner::{append_entries, peer};
use schooner::append_entries::{AppendEntriesRequest,AppendEntriesResponse};
use schooner::log::Log;
use schooner::log_entry::LogEntry;
use schooner::peer::Peer;
use serror::{InvalidState,SError};

pub mod schooner;
pub mod serror;  // TODO: move to schooner dir

// static DEFAULT_HEARTBEAT_INTERVAL: uint = 50;   // in millis
// static DEFAULT_ELECTION_TIMEOUT  : uint = 150;  // in millis
static STOP_MSG: &'static str = "STOP";   // '
static UNKNOWN: u64     = 0u64;
static UNKNOWN_LDR: int = -1;

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
    // TODO: should these three fields move to a Peer that represents "self" ?
    ip: ~str,
    tcpport: uint,
    id: uint,

    state: State,

    // TODO: should this just be Log (on stack => can it be copied arnd?)
    log: ~Log,  // log holds state information about log idx and term

    // stateMachine state
    commit_idx: u64,           // idx of highest log entry known to be committed across servers
    last_applied_commit: u64,  // idx of highest log entry applied locally (may not yet be committed on other servers)

    peers: Vec<Peer>,          // peer servers
    leader: int,               // pointer to peer that is current leader (might be self)  // TODO: this should be &Peer, but not sure how to do that in a Rust struct yet => do later

    c: Sender<~Event>,  // TODO: keep chan or port?
    p: Receiver<~Event>,

    // more later
}

// TODO: does this need to be enhanced?
pub struct Event {
    msg: ~str,  // just to get started
    // target: ??,
    // return_val: ??,
    ch: Sender<~str>,
}

// TODO: may get rid of this once elections are set up
// for providing special setup options to a server 
pub struct CfgOptions {
    init_state: State,
    // TODO: add more?
}

/* ---[ Server impl ]--- */

impl Server {
    pub fn new(id: uint, cfgpath: Path, logpath: Path) -> IoResult<~Server> {

        let (ch, pt): (Sender<~Event>, Receiver<~Event>) = channel();
        let lg = try!(Log::new(logpath.clone()));
        let peers: Vec<Peer> = try!(peer::parse_config(cfgpath.clone()));
        let myidx = get_peer_idx(&peers, id);
        if myidx == -1 {
            fail!("Server::new: Id {} is not in the config file {}", id, cfgpath.display());
        }
        let myidx = myidx as uint;
        let myid = peers.get(myidx).id;
        let myip = peers.get(myidx).ip.to_owned();
        let mytcpport = peers.get(myidx).tcpport;

        // remove self from list of peers
        let other_peers: Vec<Peer> = peers.iter().filter_map( |pr| if pr.id != myid { Some(pr.clone()) } else {None} ).collect();

        assert_eq!(peers.len(), other_peers.len() + 1);

        let s = ~Server {
            id: myid,
            ip: myip,
            tcpport: mytcpport,  // TODO: could we use udp instead? are we doing our own ACKs at the app protocol level?
            state: Stopped,
            log: lg,
            commit_idx: UNKNOWN,  // commit_idx 0 = UNKNOWN
            last_applied_commit: UNKNOWN,
            peers: other_peers,
            leader: UNKNOWN_LDR,
            c: ch,
            p: pt,
        };

        Ok(s)
    }

    ///
    /// Central method that sets things up and then runs the server threads/tasks
    /// Two tasks/threads are in operation:
    /// - a network-listener task is spawned and
    /// - the current task goes into the server_loop until a STOP signal is received
    ///
    pub fn run(&mut self, opt: Option<CfgOptions>) -> Result<(), SError> {
        if self.state != Stopped {
            return Err(InvalidState(~"schooner.Server: Server already running"));
        }

        match opt {
            Some(cfg) => self.state = cfg.init_state,
            None => self.state = Follower
        }
        

        let event_chan = self.c.clone();
        let conx_str = format!("{}:{:u}", &self.ip, self.tcpport);
        spawn(proc() {
            // FIXME: needs to be a separate file/impl ??
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
            std::io::timer::sleep(1);  // TODO: why is this here? put behind a debug flag?
        }
        debug!("Serve_loop END");  // TODO: change to debug!
    }


    fn follower_loop(&mut self) {
        let mut timer = Timer::new().unwrap();

        loop {
            println!("FLW: DEBUG 0");
            let timeout = timer.oneshot(1000); // use for detecting lost leader  // TODO: adjust this time

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

            } else if ret == pt.id() {  // received event from network_listener
                let ev = pt.recv();
                debug!("follower: event message: {}", ev.msg);
                if is_stop_msg(ev.msg) {
                    println!("FLW: DEBUG 2");
                    self.state = Stopped;
                    break;

                } else if is_cmd_from_client(ev.msg) {  // redirect to leader
                    debug!("redirecting client to leader");
                    ev.ch.send( self.redirect_msg() );

                } else {
                    let result = append_entries::decode_append_entries_request(ev.msg);
                    if result.is_err() {
                        fail!("ERROR: Unable to decode msg into append_entry_request: {:?}.\nError is: {:?}", ev.msg, result.err());
                    }
                    let aereq = result.unwrap();
                    self.leader = get_leader_ptr(&aereq, &self.peers); // update leader based on each AEReq
                    let aeresp = match self.log.append_entries(&aereq) {
                        Ok(_)  => {
                            // from the Raft spec: adjust commit_idx only if you accepted the AEReq
                            // and the leader's commit_idx is greater than yours
                            if aereq.commit_idx > self.commit_idx {
                                self.commit_idx = cmp::min(aereq.commit_idx, self.log.idx);
                            }
                            AppendEntriesResponse{success: true,
                                                  term: self.log.term,
                                                  idx: self.log.idx,
                                                  commit_idx: self.commit_idx}
                        },
                        Err(e) => {
                            error!("******>>>>>>>>>>>>>> {:?}", e);
                            AppendEntriesResponse{success: false,
                                                  term: self.log.term,
                                                  idx: self.log.idx,
                                                  commit_idx: self.commit_idx}
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
        println!("leader loop with id = {:?}", self.id);

        let mut timer = Timer::new().unwrap();
        // FIXME: this needs to be in a loop
        let timeout = timer.oneshot(100); // frequency of heartbeat to followers  // TODO: parameterize this time
        timeout.recv();

        // TODO: change this to a Vec<(peer.id, chsend)> and just search linearly through it => no need for full hashmap
        //       or implement some simple ArrayHashMap like Clojure has
        let mut peer_chans: HashMap<uint, Sender<~str>> = HashMap::new();

        // spawn the peer handler tasks
        for p in self.peers.iter() {
            let (chsend, chrecv): (Sender<~str>, Receiver<~str>) = channel();
            peer_chans.insert(p.id, chsend);

            // TODO: need to launch a separate task to handle each IO interaction with the followers
            let peer = p.clone();
            spawn(proc() {
                // TODO: will need to send a chsend as well to message back (at least shutdown notices)
                // TODO: put in logic to flw_handler that if no messages from papa after some timeout, to shutdown
                Server::leader_peer_handler(peer, chrecv);
            });
        }

        // now wait for network msgs (or timeout -> need to add timeout)
        // TODO: should be in loop
        let ev = self.p.recv();
        debug!("follower: event message: {}", ev.msg);
        if is_stop_msg(ev.msg) {
            println!("LDR: DEBUG 200");
            self.state = Stopped;
            // for p in self.peers.iter() {
            //     let chsend: &Sender<~str> = peer_chans.get(&p.id);
            //     chsend.send(~"STOP");
            // }
            // break;

        } else if is_cmd_from_client(ev.msg) {  // redirect to leader
            println!("LDR: DEBUG 201: msg from client: {:?}", ev.msg);
            ev.ch.send(~"200 OK");
        }

        // for now just stop after receiving one message
        for p in self.peers.iter() {
            let chsend: &Sender<~str> = peer_chans.get(&p.id);
            if self.state == Stopped {
                chsend.send(~"STOP");
            } 
        }
        self.state = Snapshotting;
    }

    // TODO: this could return a Future and send Future<bool> to indicate closing down
    ///
    /// Runs in its own task and handles all leader->follower messaging for 
    /// 
    fn leader_peer_handler(peer: Peer, chrecv: Receiver<~str>) {
        let msg = chrecv.recv();
        println!("{:?}", msg);
        if msg == ~"STOP" {
            println!("RECEIVED STOP MESSAGE for peer hdlr {:?}", peer.id);
        }
        println!("PEER HANDLER FOR {:?} SHUTTING DOWN", peer.id);
    }
    
    // TODO: what the hell is this state?  Got it from goraft => is it needed?
    fn snapshotting_loop(&mut self) {
        println!("snapshotting loop");
        self.state = Follower;
    }
        
    ///
    /// Builds AEReq with no entries.
    /// Called by Leader to send AEReqs with followers
    /// To send a heartbeat message, pass in an empty vector
    ///
    fn make_aereq(&self, entry_msgs: Vec<~str>) -> AppendEntriesRequest {
        let mut entries: Vec<LogEntry> = Vec::with_capacity(entry_msgs.len());
        for m in entry_msgs.move_iter() {
            let ent = LogEntry {idx: self.log.idx,
                                term: self.log.term,
                                data: m,
                                uuid: Uuid::new_v4().to_hyphenated_str()};
            entries.push(ent);
        }
        AppendEntriesRequest {
            term: self.log.term,
            prev_log_idx: self.log.idx,
            prev_log_term: self.log.term,
            commit_idx: self.commit_idx,
            leader_id: self.id,
            entries: entries,
        }        
    }
        
    ///
    /// For cases where a client needs to be redirected to the Schooner leader
    /// this returns the redirect message of format:
    /// 'Length: n\nRedirect: ipaddr:port\n'
    /// where 'n' is the length of the message starting from 'Redirect' to the end.
    /// If the leader is not known, this redirects the client to some other server
    /// that may know.
    ///
    fn redirect_msg(&self) -> ~str {
        let mut ldx = self.leader as uint;
        // if leader is unknown then pick one of the peers at random
        if self.leader == UNKNOWN_LDR {
            ldx = task_rng().gen_range(0u, self.peers.len());
        }
        let ldr = self.peers.get(ldx);
        let redir_msg = format!("Redirect: {}:{:u}\n", &ldr.ip, ldr.tcpport);
        format!("Length: {}\n{}", redir_msg.len(), redir_msg)
    }
}


/* ---[ functions ]--- */

///
/// Searches through Vec<Peer> for the Peer with id 'id'
/// and returns its index in the vector.  Returns -1 if
/// no Peer with that id is found.
///
fn get_peer_idx(peers: &Vec<Peer>, id: uint) -> int {
    for i in range(0, peers.len()) {
        if peers.get(0).id == id {
            return i as int;
        }
    }
    -1
}


///
/// Iterates through the Peers Vec to find the array idx of the peer
/// that has the leader_id in the AEReq.  If no peer matches the leader_id
/// in AEReq, UNKNOWN_LDR (-1) is returned.
///
fn get_leader_ptr(aereq: &AppendEntriesRequest, peers: &Vec<Peer>) -> int {
    for i in range(0, peers.len()) {
        if peers.get(i).id == aereq.leader_id {
            return i as int;
        }
    }
    debug!("server.get_leader_ptr: no peer has id {}", aereq.leader_id);
    UNKNOWN_LDR
}


fn is_cmd_from_client(msg: &str) -> bool {
    msg.trim().starts_with("PUT ")
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

///
/// TODO: can this fn deal with HTTP style requests? If not, what should the client request look like for this to work?
///
fn read_network_msg(stream: TcpStream) -> IoResult<~str> {
    let mut reader = BufferedReader::new(stream);

    let length_hdr = try!(reader.read_line());
    let result = parse_content_length(length_hdr);
    if result.is_none() {
        return Err(IoError{kind: InvalidInput,
                           desc: "Length not parsable in network message",
                           detail: Some(format!("length line parsed: {:s}", length_hdr))});
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

///
/// The network listener sets up a socket listener loop to accept incoming TCP connections.
/// When a network msg comes in, an Event is created with the string contents of the "message"
/// and a Sender channel is put on the Event (why??) and the event is sent.
/// The serve_loop will read from that channel and process the Event.
/// Events can be any incoming information, such as STOP messages, AEReqs, AEResponses or client commands (???) => not yet implemented
/// chan: Event channel in the Server struct.
///
fn network_listener(conx_str: ~str, chan: Sender<~Event>) {
    let addr = from_str::<SocketAddr>(conx_str).expect("Address error.");
    let mut acceptor = TcpListener::bind(addr).unwrap().listen();
    println!("server <id> listening on {:}", addr);  // TODO: need to pass in id?

    // TODO: document what this channel is for
    let (chsend, chrecv): (Sender<~str>, Receiver<~str>) = channel();
    let mut stop_signalled = false;

    for stream in acceptor.incoming() {
        println!("NL: DEBUG 0");

        let mut stream = stream.unwrap();

        // TODO: only handling one request at a time for now => spawn threads later?
        match read_network_msg(stream.clone()) {
            Ok(input)  => {
                let ev = ~Event{msg: input.clone(), ch: chsend.clone()};
                chan.send(ev);

                if is_stop_msg(input) {
                    stop_signalled = true;  // TODO: do I need to set this bool var or can I just break out here?
                    println!("NL: DEBUG 1: was stop msg");

                } else {
                    println!("NL: sent Event to event-loop; now waiting on response");

                    // Once the Event is sent to serve-loop task it awaits a response (string)
                    // and the response will be send back to the network caller.
                    // Since the response is just a string, all logic of what is in the request
                    // & response is handled by the serve-loop
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
        if stop_signalled {
            println!("NL: DEBUG 4");
            break;
        }
    }

    // TODO: change this to debug!
    println!("network listener shutting down ...");
}


fn is_stop_msg(s: &str) -> bool {
    s == STOP_MSG
}

// TODO: need to implement this
fn main() {
    // let id = 1;
    // let path = Path::new(~"datalog/S1");
    // let ipaddr = ~"127.0.0.1";
    // let port = 23158;

    let peer_cfg_path = Path::new(~"???");

    // let result = Server::new(id, path, ipaddr, port, peer_cfg_path);
    // if result.is_err() {
    //     error!("{:?}", result.err());
    //     return;
    // }

    // let mut s = result.unwrap();
    // match s.run() {
    //     Ok(_) => (),
    //     Err(e) => println!("ERROR: {:?}", e)
    // }
}


/* ---[ TESTS ]--- */

#[cfg(test)]
mod test {
    extern crate serialize;

    use std::io;
    use std::io::{BufferedReader,File,IoResult,Open,Write};
    use std::io::fs;
    use std::io::net::ip::SocketAddr;
    use std::io::net::tcp::TcpStream;
    use std::io::timer;
    use std::vec::Vec;

    use serialize::json;
    use uuid::Uuid;

    use super::CfgOptions;
    use super::Leader;
    use super::Server;

    use schooner::append_entries::AppendEntriesRequest;
    use schooner::log_entry::LogEntry;

    static S1TEST_DIR    : &'static str = "datalog";         // '
    static S1TEST_PATH   : &'static str = "datalog/S1TEST";  // '
    static S1TEST_CFG    : &'static str = "server.test.config";  // '
    static S1TEST_IPADDR : &'static str = "127.0.0.1";       // '
    static S1TEST_PORT   : uint         = 23158;
    static LEADER_ID     : uint         = 4;

    fn write_config() -> Path {
        let path = Path::new(S1TEST_CFG);
        let mut file = File::open_mode(&path, Open, Write).unwrap();

        for i in range(0u, 5u) {
            let entry = format!("peer.{}.addr = 127.0.0.1:{}", i + 1, S1TEST_PORT + i);
            let result = file.write_line(entry);
            if result.is_err() {
                fail!("write_config write_line fail: {}", result.unwrap_err());
            }
        }

        path
    }

    fn setup() -> ~Server {
        let cfgpath = write_config();
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

        let id = 1;  // this server has id 1
        let result = super::Server::new(id, cfgpath, filepath);
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
        let cfgpath = Path::new(S1TEST_CFG);
        let _ = fs::unlink(&cfgpath);
    }

    fn launch_server(opt: Option<CfgOptions>) -> SocketAddr {
        spawn(proc() {
            let mut server = setup();
            match server.run(opt) {
                Ok(_) => (),
                Err(e) => fail!("launch_server: ERROR: {:?}", e)
            }
        });
        timer::sleep(750); // FIXME this is unstable => how fix?
        from_str::<SocketAddr>(format!("{:s}:{:u}", S1TEST_IPADDR.to_owned(), S1TEST_PORT)).unwrap()
    }

    ///
    /// To specify an AEReq with no entries (hearbeat), pass in an idx_vec of size 0 and a term_vec of size 1 (need curr term)
    /// Otherwise, idx_vec.len() == term_vec() must be true.
    ///
    fn send_aereqs_with_commit_idx(stream: &mut IoResult<TcpStream>, idx_vec: Vec<u64>, term_vec: Vec<u64>,
                                   prev_log_idx: u64, prev_log_term: u64, commit_idx: u64) -> Vec<LogEntry> {

        if idx_vec.len() > 0 && idx_vec.len() != term_vec.len() {
            fail!("send_reqs: size of vectors doesn't match: idx_vec: {}; term_vec: {}", idx_vec.len(), term_vec.len());
        }

        let mut entries = Vec::with_capacity(term_vec.len());
        if idx_vec.len() > 0 {
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
        }

        let aereq = ~AppendEntriesRequest{
            term: *term_vec.get( term_vec.len() - 1 ),
            prev_log_idx: prev_log_idx,
            prev_log_term: prev_log_term,
            commit_idx: commit_idx,
            leader_id: LEADER_ID,
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

    ///
    /// The number of entries in idx_vec and term_vec determines the # of logentries to send to
    /// the server. The len of the two vectors must be the same.
    ///
    fn send_aereqs(stream: &mut IoResult<TcpStream>, idx_vec: Vec<u64>, term_vec: Vec<u64>,
                   prev_log_idx: u64, prev_log_term: u64) -> Vec<LogEntry> {
        let commit_idx = *idx_vec.get(0);
        send_aereqs_with_commit_idx(stream, idx_vec, term_vec, prev_log_idx, prev_log_term, commit_idx)
    }

    fn send_client_cmd(stream: &mut IoResult<TcpStream>, cmd: ~str) -> IoResult<()> {
        let msg = format!("Length: {}\n{}", cmd.len(), cmd);
        try!(stream.write_str(msg));
        try!(stream.flush());
        Ok(())
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
            term: 1,
            prev_log_idx: 0,
            prev_log_term: 0,
            commit_idx: 0,
            leader_id: LEADER_ID,
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
    fn test_leader_simple() {
        let cfg = CfgOptions {
            init_state: Leader
        };
        let addr = launch_server(Some(cfg));
        let mut stream = TcpStream::connect(addr);
        let send_result = send_client_cmd(&mut stream, ~"PUT x=1");
        if send_result.is_err() {
            signal_shutdown();
            fail!(send_result);
        }

        let result = stream.read_to_str();
        drop(stream); // close the connection

        assert!(result.is_ok());
        assert_eq!(~"200 OK", result.unwrap());  // BOGUS tmp response
        
        signal_shutdown();
        tear_down();
    }
    
    #[test]
    fn test_follower_to_send_client_redirect_when_leader_is_unknown() {
        // launch server => this will not shutdown until a STOP signal is sent
        let addr = launch_server(None);
        let mut stream = TcpStream::connect(addr);
        let send_result = send_client_cmd(&mut stream, ~"PUT x=1");
        if send_result.is_err() {
            signal_shutdown();
            fail!(send_result);
        }

        let result = stream.read_to_str();
        drop(stream); // close the connection

        signal_shutdown();

        println!("{:?}", result);
        assert!(result.is_ok());
        let resp = result.unwrap();

        assert!( resp.contains("Redirect: 127.0.0.1:231") );
        assert!( !resp.contains("Redirect: 127.0.0.1:23158") );

        tear_down();
    }

    #[test]
    fn test_follower_to_send_client_redirect_when_leader_is_known() {
        // launch server => this will not shutdown until a STOP signal is sent
        let addr = launch_server(None);
        let mut stream = TcpStream::connect(addr);

        // this message lets the follower know who the leader is (LEADER_ID = 4)
        let _ = send_aereq1(&mut stream);
        let result1 = stream.read_to_str();
        drop(stream); // close the connection


        // client cmd to follower => now should get back leader
        stream = TcpStream::connect(addr);
        let send_result = send_client_cmd(&mut stream, ~"PUT x=1");
        if send_result.is_err() {
            signal_shutdown();
            fail!(send_result);
        }
        let result2 = stream.read_to_str();

        drop(stream); // close the connection

        signal_shutdown();  // TODO: is there a better way to ensure a fn is called if an assert fails?

        println!("{:?}", result2);
        assert!(result1.is_ok());
        assert!(result2.is_ok());
        let resp = result2.unwrap();

        assert!(resp.contains(format!("Redirect: 127.0.0.1:{}", 23158 + LEADER_ID - 1))); // port should be 23161

        tear_down();
    }

    #[test]
    fn test_follower_with_single_AppendEntryRequest() {
        // launch server => this will not shutdown until a STOP signal is sent
        let addr = launch_server(None);
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
        let addr = launch_server(None);
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
        let addr = launch_server(None);
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

    // TODO: test commit_idx as well
    #[test]
    fn test_follower_with_AppendEntryRequests_with_invalid_term() {
        // launch server => this will not shutdown until a STOP signal is sent
        let addr = launch_server(None);
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

        // result 2
        assert!(result2.is_ok());
        let resp = result2.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(3, aeresp.idx);

        // result 3
        assert!(result3.is_ok());
        let resp = result3.unwrap();

        assert!(resp.contains("\"success\":false"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(false, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(3, aeresp.idx);

        tear_down();   // TODO: is there a better way to ensure a fn is called if an assert fails?
    }

    #[test]
    fn test_follower_with_AppendEntryRequests_with_invalid_prevLogTerm() {
        // launch server => this will not shutdown until a STOP signal is sent
        let addr = launch_server(None);
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


    // #[test]
    fn test_follower_with_AppendEntryRequests_with_changing_commit_idx() {
        // launch server => this will not shutdown until a STOP signal is sent
        let addr = launch_server(None);
        let mut stream = TcpStream::connect(addr);

        /* ---[ AER 1, valid, initial ]--- */
        let mut terms: Vec<u64> = vec!(1,1);
        let mut indexes: Vec<u64> = vec!(1,2);
        let prev_log_idx = 0u64;
        let prev_log_term = 0u64;
        let commit_idx = super::UNKNOWN;  // first send out -> no consensus, so nothing committed
        let logentries1: Vec<LogEntry> = send_aereqs_with_commit_idx(&mut stream, indexes, terms, prev_log_idx, prev_log_term, commit_idx);
        let result1 = stream.read_to_str();
        drop(stream); // close the connection

        let num_entries_logged1 = num_entries_in_log();

        // /* ---[ AER 2: one new entry, commit-idx to 2 ]--- */
        stream = TcpStream::connect(addr);
        terms = vec!(2);
        indexes = vec!(3);
        let prev_log_idx = 2u64;
        let prev_log_term = 1u64;
        let commit_idx = 2;  // idx two has committed on the leader's stateMachine
        let logentries2: Vec<LogEntry> = send_aereqs_with_commit_idx(&mut stream, indexes, terms, prev_log_idx, prev_log_term, commit_idx);
        let result2 = stream.read_to_str();
        drop(stream); // close the connection

        let num_entries_logged2 = num_entries_in_log();

        /* ---[ AER 3: 4 new entries, commit-idx still 2 ]--- */
        stream = TcpStream::connect(addr);
        terms = vec!(2, 2, 2, 2);
        indexes = vec!(4, 5, 6, 7);
        let prev_log_idx = 3u64;
        let prev_log_term = 2u64;
        let commit_idx = 2;  // only idx two has committed on the leader's stateMachine (no change)
        let logentries3: Vec<LogEntry> = send_aereqs_with_commit_idx(&mut stream, indexes, terms, prev_log_idx, prev_log_term, commit_idx);
        let result3 = stream.read_to_str();
        drop(stream); // close the connection

        let num_entries_logged3 = num_entries_in_log();

        /* ---[ AER 4: no new entries, commit-idx bumped to 6 ]--- */
        stream = TcpStream::connect(addr);
        terms = vec!(2);
        indexes = Vec::new();  // empty vec, so this is a heartbeat msg
        let prev_log_idx = 7u64;
        let prev_log_term = 2u64;
        let commit_idx = 6;
        let logentries4: Vec<LogEntry> = send_aereqs_with_commit_idx(&mut stream, indexes, terms, prev_log_idx, prev_log_term, commit_idx);
        let result4 = stream.read_to_str();
        drop(stream); // close the connection

        let num_entries_logged4 = num_entries_in_log();

        /* ---[ AER 5: 2 new entries, commit-idx bumped to 10, one higher than curr idx ]--- */
        stream = TcpStream::connect(addr);
        terms = vec!(2, 2);
        indexes = vec!(8, 9);
        let prev_log_idx = 7u64;
        let prev_log_term = 2u64;
        let commit_idx = 10;  // higher than entries in current log => follower should only go to 9
        let logentries5: Vec<LogEntry> = send_aereqs_with_commit_idx(&mut stream, indexes, terms, prev_log_idx, prev_log_term, commit_idx);
        let result5 = stream.read_to_str();
        drop(stream); // close the connection

        let num_entries_logged5 = num_entries_in_log();

        signal_shutdown();  // TODO: is there a better way to ensure a fn is called if an assert fails?

        /* ---[ validate ]--- */

        // number sent
        assert_eq!(2, logentries1.len());
        assert_eq!(1, logentries2.len());
        assert_eq!(4, logentries3.len());
        assert_eq!(0, logentries4.len());
        assert_eq!(2, logentries5.len());

        // number in logfile after each AER
        assert_eq!(2, num_entries_logged1);
        assert_eq!(3, num_entries_logged2);
        assert_eq!(7, num_entries_logged3);
        assert_eq!(7, num_entries_logged4);
        assert_eq!(9, num_entries_logged5);

        // result 1
        assert!(result1.is_ok());
        let resp = result1.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(1, aeresp.term);
        assert_eq!(2, aeresp.idx);
        assert_eq!(super::UNKNOWN, aeresp.commit_idx);  // key test

        // result 2
        assert!(result2.is_ok());
        let resp = result2.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(3, aeresp.idx);
        assert_eq!(2, aeresp.commit_idx);  // key test => should match what leader indicated

        // result 3
        assert!(result3.is_ok());
        let resp = result3.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(7, aeresp.idx);
        assert_eq!(2, aeresp.commit_idx);  // key test => should match what leader indicated

        // result 4
        assert!(result4.is_ok());
        let resp = result4.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(7, aeresp.idx);
        assert_eq!(6, aeresp.commit_idx);  // key test => should match what leader indicated

        // result 5
        assert!(result5.is_ok());
        let resp = result5.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = super::append_entries::decode_append_entries_response(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(9, aeresp.idx);
        assert_eq!(9, aeresp.commit_idx);  // key test => 9, not 10 (as leader said)

        tear_down();   // TODO: is there a better way to ensure a fn is called if an assert fails?
    }
}
