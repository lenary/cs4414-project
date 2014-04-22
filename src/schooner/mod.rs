#![feature(phase,globs)]

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
use serialize::encoded;
use rand::{task_rng,Rng};
use sync::TaskPool;

use self::events::*;
use self::consistent_log::{Log,LogEntry};
use self::net::*;
use self::append_entries::{AppendEntriesRequest,AppendEntriesResponse};

use net::handlers::leader_peer_handler;
use net::messages::{network_listener, is_stop_msg, serialize};
mod events;
mod consistent_log;
mod net;

mod traits;
mod leader;
mod candidate;
mod follower;

// TODO: find a better place for this
mod append_entries;


pub static DEFAULT_HEARTBEAT_INTERVAL: u64 =  50; // millis
pub static DEFAULT_ELECTION_TIMEOUT  : u64 = 150; // millis

// TODO: need to implement this => what command line args/switches to require/allow?
fn main() {
}

// TODO: a trait for the Application's State Machine. See also
// ApplicationReq/Res and HandoffReq/Res

// TODO use Option(thing)
static UNKNOWN: u64     = 0u64;  // used for idx and term
static UNKNOWN_LDR: int = -1;
static TASK_POOL_SZ: uint = 16;  // TODO: how decide on the appropriate size?

/* ---[ data structures ]--- */

#[deriving(Clone, Eq)]
pub enum State {
    Stopped,
    Follower,
    Candidate,
    Leader,
    Snapshotting
}

// TODO: what else?
pub enum SError {
    InvalidArgument(~str),
    InvalidState(~str),
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
    leader: int,               // "pointer" to peer that is current leader (idx into peers Vec)  // TODO: should this be &Peer?

    // c: Sender<~Event>,  // TODO: keep chan/sender?
    // p: Receiver<~Event>,

    heartbeat_interval: u64,
    election_timeout: u64,
    // TOOD: more later?
}

/// Deprecated in favour of RaftEvents above
///
/// Events are used to pass incoming network messages from the network_listener
/// task to the server_loop task
///
// TODO: does this need to be enhanced?
pub struct Event {
    msg: ~str,         // incoming message from network (Peer svr or client)
    ch: Sender<~str>,  // for server_loop task to message back to network_listener task
    // target: ??,     // TODO: these are from go-raft: needed?
    // return_val: ??,
}

// TODO: may get rid of this once elections are set up - or still needed for testing?
// for providing special setup options to a server
pub struct CfgOptions {
    init_state: State,
    // TODO: add more?
}

///
/// struct to hold the two parts of a (non-HTTP) client message
///
#[deriving(Clone, Show)]
struct ClientMsg {
    uuid: ~str,
    cmd: ~str
}

/* ---[ Server impl ]--- */

impl Server {
    ///
    /// Initialize a new server.  This constructor does NOT start the server
    /// or spawn any tasks.  Call Server#run to do that.
    /// Params:
    /// - id: id of server - must match an id in the config file
    /// - cfgpath: path to config file listing all the servers in the cluster
    /// - logpath: path to the event log for this server (and uniq to this server)
    ///
    pub fn new(id: uint, cfgpath: Path, logpath: Path) -> IoResult<~Server> {

        let lg = try!(Log::new(logpath.clone()));
        let peers: Vec<Peer> = try!(parse_config(cfgpath.clone()));
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
            tcpport: mytcpport,  // TODO: could we use UDP instead? are we doing our own ACKs at the app protocol level?
            state: Stopped,
            log: lg,
            commit_idx: UNKNOWN,  // commit_idx 0 = UNKNOWN
            last_applied_commit: UNKNOWN,
            peers: other_peers,
            leader: UNKNOWN_LDR,
            // c: ch,
            // p: pt,
            heartbeat_interval: DEFAULT_HEARTBEAT_INTERVAL,  // TODO: need to parameterize
            election_timeout: DEFAULT_ELECTION_TIMEOUT,      // TODO: need to parameterize
        };

        Ok(s)
    }


    ///
    /// Central method that runs the server threads/tasks
    /// Two tasks/threads are in operation:
    /// - a network-listener task is spawned as a new task and
    /// - the current task goes into the server_loop until a STOP signal is received
    ///   thus, this method does not return until the server shuts down
    ///
    /// TODO: return val should probably be changed to IoResult<()> ...
    pub fn run(&mut self, opt: Option<CfgOptions>) -> Result<(), SError> {
        if self.state != Stopped {
            return Err(InvalidState(~"schooner.Server: Server already running"));
        }

        match opt {
            Some(cfg) => {
                self.state = cfg.init_state;
                if self.state == Leader && self.log.term == 0 {
                    self.log.term = 1;
                }
            },
            None => self.state = Follower
        }

        let (chsend, chrecv): (Sender<~Event>, Receiver<~Event>) = channel();
        let conx_str = format!("{}:{:u}", &self.ip, self.tcpport);
        let svr_id = self.id;
        spawn(proc() {
            network_listener(conx_str, chsend, svr_id);
        });

        self.serve_loop(chrecv);

        Ok(())
    }

    fn serve_loop(&mut self, event_recvr: Receiver<~Event>) {
        debug!("Now serving => loop until Stopped");
        loop {
            match self.state {
                Follower     => self.follower_loop(&event_recvr),
                Candidate    => self.candidate_loop(),
                Leader       => self.leader_loop(&event_recvr),
                Snapshotting => self.snapshotting_loop(),
                Stopped      => break
            }
            std::io::timer::sleep(1);  // TODO: why is this here? put behind a debug flag?
        }
        debug!("Serve_loop END");
    }


    fn follower_loop(&mut self, event_recvr: &Receiver<~Event>) {
        let mut timer = Timer::new().unwrap();

        loop {
            debug!("FLW: DEBUG 0");
            let timeout = timer.oneshot(1000); // use for detecting lost leader

            // select over timeout channel and network_listener port (receiver)
            // Note: cannot use the select! macro here bcs it won't compile with channels in a struct
            //       https://github.com/mozilla/rust/issues/12902
            // so we have to manually code what the macro provides
            let sel = Select::new();
            let mut pt = sel.handle(event_recvr);
            let mut timeout = sel.handle(&timeout);
            unsafe{
                pt.add();
                timeout.add();
            }
            let ret = sel.wait();

            if ret == timeout.id() {
                timeout.recv();
                info!("FWL: TIMEOUT!! => change state to Candidate");
                self.state = Candidate;
                break;

            } else if ret == pt.id() {  // received event from network_listener
                let ev = pt.recv();
                info!("follower: event message: {}", ev.msg);
                if is_stop_msg(ev.msg) {
                    debug!("FLW: DEBUG 2");
                    self.state = Stopped;
                    break;

                } else if is_cmd_from_client(ev.msg) {  // redirect to leader
                    info!("redirecting client to leader from follower {}", self.id);
                    ev.ch.send( self.redirect_msg() );

                } else {
                    let result = append_entries::AppendEntriesRequest::decode(ev.msg);
                    if result.is_err() {
                        fail!("ERROR: Unable to decode msg into append_entry_request: {}.\nError is: {:?}", ev.msg, result.err());
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
                                                  commit_idx: self.commit_idx,
                                                  peer_id: self.id}
                        },
                        Err(e) => {
                            error!("server.follower_loop: ERROR {:?}", e);
                            AppendEntriesResponse{success: false,
                                                  term: self.log.term,
                                                  idx: self.log.idx,
                                                  commit_idx: self.commit_idx,
                                                  peer_id: self.id}
                        }
                    };
                    let jstr = serialize(&aeresp);
                    ev.ch.send(jstr);
                }
                debug!("FLW: DEBUG 3");
            }
        }
    }

    fn candidate_loop(&mut self) {
        info!("candidate loop");
        self.state = Leader;
    }

    fn leader_loop(&mut self, event_recvr: &Receiver<~Event>) {
        info!("leader loop with id = {:?}", self.id);

        let mut pool = TaskPool::new(TASK_POOL_SZ, || {
            proc(id: uint) -> uint { id }
        });

        let (chsend_response, chrecv_response): (Sender<IoResult<AppendEntriesResponse>>,
                                                 Receiver<IoResult<AppendEntriesResponse>>) = channel();

        // execute the peer handler tasks for the first heartbeat
        let first_heartbeat_msg = self.make_aereq_from_client_msg(Vec::new());
        for p in self.peers.iter() {
            let chsend_aeresp = chsend_response.clone();
            let peer = p.clone();
            let heartbeat_aereq = first_heartbeat_msg.clone();
            pool.execute(proc(task_id: &uint) {
                debug!("executing task from pool: task_id {:u}", *task_id);
                leader_peer_handler(peer, chsend_aeresp, heartbeat_aereq);
            });
        }

        // now wait for network msgs (or timeout)
        let mut timer = Timer::new().unwrap();
        let heartbeat_timer = timer.periodic(self.heartbeat_interval);

        loop {
            info!("in leader loop for = {} :: waiting on self.p", self.id);

            // select over
            // 1) network_listener receiver, which brings in:
            //    a) client cmds
            //    b) AEReq from other putative leaders
            // 2) AEResponse Receiver from peer handlers

            // TODO: change to select! macro (Note: hit problem doing that because event_recv is a borrowed ptr => get help from IRC)
            let select = Select::new();
            let mut nl_recvr = select.handle(event_recvr);
            let mut aeresp_recvr = select.handle(&chrecv_response);
            let mut heartbeat_timer = select.handle(&heartbeat_timer);

            unsafe {
                nl_recvr.add();
                aeresp_recvr.add();
                heartbeat_timer.add();
            }
            let ret = select.wait();

            if ret == aeresp_recvr.id() {
                let resp = aeresp_recvr.recv();
                self.ldr_process_aeresponse_from_peer(resp, &mut pool, &chsend_response);

            } else if ret == heartbeat_timer.id() {
                heartbeat_timer.recv();
                info!("LDR: HEARTBEAT TIMEOUT");
                self.ldr_send_heartbeat(&mut pool, &chsend_response);

            } else {
                let ev = nl_recvr.recv();
                debug!("follower: event message: {}", ev.msg);
                if is_stop_msg(ev.msg) {
                    debug!("LDR: DEBUG 200");
                    self.state = Stopped;
                    break;
                }
                // if get an AEReq another peer thinks it is leader, so need to evaulate claim and response
                else if is_aereq_from_another_leader(&ev.msg) {
                    // TODO: implement me!

                } else if is_cmd_from_client(ev.msg) {
                    self.ldr_process_cmd_from_client(&ev, &mut pool, &chsend_response, &chrecv_response);

                } else {
                    error!("LDR: Received message of unknown type: {:?}", ev.msg);
                }
            } // end else handle network_listener_receiver event
        } // end main loop

        // TODO: should this be set like this?
        if self.state != Stopped {
            self.state = Follower;
        }

        info!("LDR: leader_loop finishes with term: {}, log_idx: {}, commit_idx: {}, last_applied_commit: {}",
              self.log.term, self.log.idx, self.commit_idx, self.last_applied_commit);
    }

    ///
    /// TODO: DOCUMENT ME
    /// Returns Some(AppendEntriesResponse) if the IoResult was not an error, otherwise returns None
    ///
    fn ldr_process_aeresponse_from_peer(&mut self, resp: IoResult<AppendEntriesResponse>,
                                        pool: &mut TaskPool<uint>,
                                        chsend_response: &Sender<IoResult<AppendEntriesResponse>>) -> Option<AppendEntriesResponse> {
        match resp {
            Ok(aeresp) => {
                let peer_idx = get_peer_idx(&self.peers, aeresp.peer_id);
                assert!(peer_idx != -1);
                let peer_idx = peer_idx as uint;

                // if get here an AEResponse was returned, but the peer may have
                // rejected the AERequest, so check the success flag in the AEResponse
                if aeresp.success {
                    // TODO: should we check that the aeresp is sane (i.e., not larger than self.log.idx?)
                    self.peers.get_mut(peer_idx).next_idx = aeresp.idx + 1;

                } else {
                    // AEResponse#success=false rejection scenario
                    info!("LDR: AEReq rejection: {:?}", aeresp);

                    // interpretation of scenarios
                    // if terms match:
                    //   if peer_next_idx == self.log.idx + 1: follower has truncated logs and all now matches
                    //   if peer_next_idx  < self.log.idx + 1: follower is behind => send missing AEReqs
                    //   if peer_next_idx  > self.log.idx + 1: code logic error, follower should have truncated its logs
                    match (aeresp.term == self.log.term, aeresp.idx == self.log.idx) {
                        (true, true)   => {
                            debug!("LDR: AEResponse rejection SCENARIO 1: {:?}", aeresp);
                            // log repair scenario 1:
                            // peer idx was *ahead of* leader prev_log_idx and truncated its results back leader
                            self.peers.get_mut(peer_idx).next_idx = aeresp.idx + 1;
                        },
                        (true, false)  => {
                            debug!("LDR: AEResponse rejection SCENARIO 2: {:?}", aeresp);
                            // log repair scenario 2:
                            // peer idx was *behind* leader prev_log_idx and needs to catch up

                            if aeresp.idx > self.log.idx {
                                fail!("server.ldr_process_aeresponse_from_peer: peer <{}> has aeresp.idx > self.log.idx; {}",
                                      self.peers.get(peer_idx).id, "ERROR: should have truncated logs")
                            }

                            let peer_next_idx = aeresp.idx + 1;
                            self.peers.get_mut(peer_idx).next_idx = peer_next_idx;
                            let chsend_aeresp = chsend_response.clone();

                            let aereq = self.make_aereq_from_log(peer_next_idx);
                            let peer_copy = self.peers.get_mut(peer_idx).clone();
                            pool.execute(proc(_: &uint) {
                                leader_peer_handler(peer_copy, chsend_aeresp, aereq);
                            });
                        },
                        (false, true)  => {},
                        (false, false) => {},
                    }

                    self.peers.get_mut(peer_idx).next_idx -= 1;  // back off to try again
                    // TODO: do something with peer.match_idx ?
                    // TODO: send aereq here ? => where/how do we keep trying with this peer ? => log_repair work needed here
                }
                Some(aeresp)
            },
            Err(e) => {
                error!("LDR: Error returned from peer <?>: {:?}", e);
                None
            }
        }
    }

    fn response_is_for_current_client_cmd(&self, aeresp: &AppendEntriesResponse) -> bool {
        aeresp.idx == self.log.idx + 1  // +1 bcs server doesn't log cmd until (majority-1) peers have
    }

    fn ldr_send_heartbeat(&mut self,
                          pool: &mut TaskPool<uint>,
                          chsend_response: &Sender<IoResult<AppendEntriesResponse>>) {
        info!("LDR: INFO 301: sending hearbeat via peer_handlers");
        let hearbeat_aereq = self.make_aereq_from_client_msg(Vec::new());

        for p in self.peers.iter() {
            let chsend_aeresp = chsend_response.clone();
            let peer = p.clone();
            let aereq_copy = hearbeat_aereq.clone();
            pool.execute(proc(task_id: &uint) {
                debug!("executing task from pool: task_id {:u}", *task_id);
                leader_peer_handler(peer, chsend_aeresp, aereq_copy);
            });
        }
    }

    ///
    /// TODO: DOCUMENT ME
    ///
    fn ldr_process_cmd_from_client(&mut self, ev: &~Event,
                                   pool: &mut TaskPool<uint>,
                                   chsend_response: &Sender<IoResult<AppendEntriesResponse>>,
                                   chrecv_response: &Receiver<IoResult<AppendEntriesResponse>>) {
        info!("LDR: INFO 201: msg from client: {}", ev.msg);
        match create_client_msg(&ev.msg) {
            None            => ev.ch.send(~"400 Bad Request"),
            Some(clientMsg) => {
                let aereq = self.make_aereq_from_client_msg(vec!(clientMsg));

                // first send this message to the peer-handlers
                // without logging to own log
                for p in self.peers.iter() {
                    let chsend_aeresp = chsend_response.clone();
                    let peer = p.clone();
                    let aereq_copy = aereq.clone();
                    pool.execute(proc(task_id: &uint) {
                        debug!("executing task from pool: task_id {:u}", *task_id);
                        leader_peer_handler(peer, chsend_aeresp, aereq_copy);
                    });
                }

                // TODO: need to set last_applied_commit at appropriate point ... not being set yet
                let majority_cutoff = self.peers.len() / 2;
                let mut commits = 0;
                let mut timer = Timer::new().unwrap();

                // client_response_timer is for avoiding infinite waits on chrecv_response
                // setting one timeout for the whole interaction with peers, not per interaction
                let client_response_timeout = timer.oneshot(self.heartbeat_interval * 3); // FIXME: semi-arbitrary duration - what would be better?

                // still have to handle heartbeats within the specified timeframe and since the client
                // oneshot interaction may take many multiples of heartbeat_interval, we have to keep
                // signaling while down in this inner loop of leader_loop
                let mut timer = Timer::new().unwrap();
                let heartbeat_timer = timer.periodic(self.heartbeat_interval);

                loop {
                    let sel = Select::new();
                    let mut chrecv_response = sel.handle(chrecv_response);
                    let mut client_response_timeout = sel.handle(&client_response_timeout);
                    let mut heartbeat_timer = sel.handle(&heartbeat_timer);
                    unsafe{
                        chrecv_response.add();
                        client_response_timeout.add();
                        heartbeat_timer.add();
                    }
                    let ret = sel.wait();

                    if ret == client_response_timeout.id() {
                        client_response_timeout.recv();
                        info!("LDR: TIMEOUT while waiting for responses from peer handlers");
                        ev.ch.send(~"500 Server Error: unable to log command on leader");
                        break;

                    } else if ret == heartbeat_timer.id() {
                        heartbeat_timer.recv();
                        info!("LDR: ldr_process_cmd_from_client HEARTBEAT TIMEOUT");
                        self.ldr_send_heartbeat(pool, chsend_response);

                    } else {
                        let resp = chrecv_response.recv();
                        match self.ldr_process_aeresponse_from_peer(resp, pool, chsend_response) {
                            None => (),
                            Some(aeresp) => {
                                if aeresp.success && self.response_is_for_current_client_cmd(&aeresp) {
                                    commits += 1;
                                    if commits >= majority_cutoff {
                                        self.ldr_append_to_log_and_send_client_response(&aereq, ev);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                } // end loop
            }
        }
    }

    fn ldr_append_to_log_and_send_client_response(&mut self, aereq: &AppendEntriesRequest, ev: &~Event) {
        // commit to own log and send response
        match self.log.append_entries(aereq) {
            Ok(_)  => {
                self.commit_idx = self.log.idx;  // ??? I think this is right - double check
                ev.ch.send(~"200 OK");
            },
            Err(e) => {
                error!("leader_loop log.append_entries ERROR: {:?}", e);
                ev.ch.send(~"500 Server Error: unable to log command on leader");
                // TODO: the leader should probably kill itself here?
            }
        };
    }

    // TODO: what is this state?  Got it from goraft => is it needed?
    fn snapshotting_loop(&mut self) {
        info!("snapshotting loop");
        self.state = Follower;
    }

    ///
    /// Crates an AERequest with already logged entries starting from `logidx`
    /// in the Log cache.
    ///
    fn make_aereq_from_log(&mut self, logidx: u64) -> AppendEntriesRequest {
        // let mut entries: Vec<LogEntry> = Vec::with_capacity((self.log.idx - logidx) as uint);

        // FIXME: dangerous => converting from u64 to uint, so in effect can't cache
        //        more then uint-max in memory
        let entries = Vec::from_slice(self.log.logentries.slice_from(logidx as uint));

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
    /// Builds AEReq with specified entries (which may be none).
    /// Called by Leader to send AEReqs to followers.
    /// To send a heartbeat message, pass in an empty vector.
    ///
    fn make_aereq_from_client_msg(&mut self, entry_msgs: Vec<ClientMsg>) -> AppendEntriesRequest {
        let mut entries: Vec<LogEntry> = Vec::with_capacity(entry_msgs.len());
        let mut count = 0u64;
        for cmsg in entry_msgs.move_iter() {
            count += 1;
            let ent = LogEntry {idx: self.log.idx + count,
                                term: self.log.term,
                                data: cmsg.cmd.clone(),
                                uuid: cmsg.uuid};
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
        if peers.get(i).id == id {
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


///
/// The incoming message should be of the format:
/// "Uuid: <some-uuid>\n<cmd>",
/// e.g,:
/// "Uuid: 15a0779e-5881-45ac-9739-cd22d7454d9b\nPUT x=1"
/// The message is split into the component uuid and cmd
/// parts and put into a ClientMsg struct, which is returned.
/// If the message doesn't match that format, then None is returned.
///
fn create_client_msg(msg: &~str) -> Option<ClientMsg> {
    let parts: ~[&str] = msg.splitn('\n', 1).collect();
    if parts.len() != 2 {
        return None;
    }

    Some(ClientMsg {
        uuid: parts[0].trim().to_owned(),
        cmd:  parts[1].trim().to_owned()
    })
}


fn is_aereq_from_another_leader(msg: &~str) -> bool {
    // TODO: implement detection of whether the message is an AppendEntriesRequest
    false  // BOGUS
}


fn is_cmd_from_client(msg: &str) -> bool {
    msg.trim().starts_with("Uuid: ")
}


/* ---[ TESTS ]--- */

#[cfg(test)]
mod test {
    extern crate serialize;

    use std::io;
    use std::io::{BufferedReader,File,IoResult,IoError,InvalidInput,Open,Write};
    use std::io::fs;
    use std::io::net::ip::SocketAddr;
    use std::io::net::tcp::TcpStream;
    use std::io::timer;
    use std::vec::Vec;

    use net::messages::serialize;
    use uuid::Uuid;

    use super::CfgOptions;
    use super::Leader;
    use super::Server;

    use super::append_entries;
    use super::append_entries::AppendEntriesRequest;
    use super::consistent_log::LogEntry;

    static TEST_CFG      : &'static str = "server.test.config";
    static TEST_DIR      : &'static str = "datalog";
    static S1TEST_PATH   : &'static str = "datalog/S1TEST";
    static S2TEST_PATH   : &'static str = "datalog/S2TEST";
    static S3TEST_PATH   : &'static str = "datalog/S3TEST";
    static S4TEST_PATH   : &'static str = "datalog/S4TEST";
    static S5TEST_PATH   : &'static str = "datalog/S5TEST";

    static TEST_IPADDR   : &'static str = "127.0.0.1";
    static S1TEST_PORT   : uint         = 23158;
    // static S2TEST_PORT   : uint         = 23159;
    // static S3TEST_PORT   : uint         = 23160;
    // static S4TEST_PORT   : uint         = 23161;
    // static S5TEST_PORT   : uint         = 23162;
    static LEADER_ID     : uint         = 1;

    fn write_cfg_file(num_svrs: uint) {
        let cfgpath = Path::new(TEST_CFG);

        let mut file = File::open_mode(&cfgpath, Open, Write).unwrap();

        for i in range(0u, num_svrs) {
            let entry = format!("peer.{}.addr = 127.0.0.1:{}", i + 1, S1TEST_PORT + i);
            let result = file.write_line(entry);
            if result.is_err() {
                fail!("write_config write_line fail: {}", result.unwrap_err());
            }
        }
    }

    fn read_datalog(pathstr: &str) -> IoResult<Vec<~str>> {
        let logpath = Path::new(pathstr);
        let file = try!(File::open(&logpath));
        let mut br = BufferedReader::new(file);
        let mut entries: Vec<~str> = Vec::with_capacity(10);
        for line in br.lines() {
            match line {
                Ok(ln) => entries.push(ln.trim().to_owned()),
                Err(e) => return Err(e)
            }
        }
        Ok(entries)
    }

    fn create_server(id: uint) -> IoResult<~Server> {
        let dirpath = Path::new(TEST_DIR);
        let logpath = match id {
            1 => Path::new(S1TEST_PATH),
            2 => Path::new(S2TEST_PATH),
            3 => Path::new(S3TEST_PATH),
            4 => Path::new(S4TEST_PATH),
            5 => Path::new(S5TEST_PATH),
            _ => return Err(IoError{kind: InvalidInput,
                                    desc: "create_server: Invalid server id",
                                    detail: Some(format!("Invalid server id: {}", id))})
        };

        // TODO: these asserts and fails are dangerous -> if another server already started the test will hang
        // TODO: probably need to return Result<~Server, Err> instead and let the test case handle errors
        if logpath.exists() {
            try!(fs::unlink(&logpath));
        }
        if ! dirpath.exists() {
            try!(fs::mkdir(&dirpath, io::UserRWX));
        }
        let server = try!(Server::new(id, Path::new(TEST_CFG), logpath));
        Ok(server)
    }

    fn start_server(svr_id: uint, opt: Option<CfgOptions>) -> IoResult<SocketAddr> {
        let (ch, pt): (Sender<IoResult<SocketAddr>>, Receiver<IoResult<SocketAddr>>) = channel();

        spawn(proc() {
            match create_server(svr_id) {
                Ok(mut server) => {
                    let ipaddr = format!("{:s}:{:u}", server.ip.to_owned(), server.tcpport);
                    let socket_addr = from_str::<SocketAddr>(ipaddr).unwrap();
                    ch.send(Ok(socket_addr));
                    match server.run(opt) {
                        Ok(_) => (),
                        Err(e) => error!("launch_cluster: ERROR: {:?}", e)
                    }
                },
                Err(e) => ch.send(Err(e))
            }
        });

        let socket_addr = try!(pt.recv());
        Ok(socket_addr)
    }

    fn send_shutdown_signal(svr_id: uint) {
        let ipaddr = format!("{:s}:{:u}", TEST_IPADDR, S1TEST_PORT + svr_id - 1);
        let addr = from_str::<SocketAddr>(ipaddr).unwrap();
        let mut stream = TcpStream::connect(addr);

        // TODO: needs to change to AER with STOP cmd
        let stop_msg = format!("Length: {:u}\n{:s}", "STOP".len(), "STOP");  // TODO: make "STOP" a static constant
        let result = stream.write_str(stop_msg);
        if result.is_err() {
            fail!("send_shutdown_signal: Client ERROR connection to {}.  ERROR: {:?}", ipaddr, result.err());
        }
        drop(stream); // close the connection
        timer::sleep(100);  // TODO: remove this ??
    }


    fn setup() {
        let mut filepath = Path::new(S1TEST_PATH);
        let _ = fs::unlink(&filepath);
        filepath = Path::new(S2TEST_PATH);
        let _ = fs::unlink(&filepath);
        filepath = Path::new(S3TEST_PATH);
        let _ = fs::unlink(&filepath);
        filepath = Path::new(S4TEST_PATH);
        let _ = fs::unlink(&filepath);
        filepath = Path::new(S5TEST_PATH);
        let _ = fs::unlink(&filepath);
        let cfgpath = Path::new(TEST_CFG);
        let _ = fs::unlink(&cfgpath);
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

        let encoded_aereq = serialize(aereq);
        let req_msg = format!("Length: {:u}\n{:s}", encoded_aereq.len(), encoded_aereq);

        let result = stream.write_str(req_msg);
        if result.is_err() {
            error!("Client ERROR: {:?}", result.err());
        }
        let _ = stream.flush();

        entries
    }

    fn send_aereq_heartbeat(stream: &mut IoResult<TcpStream>, term: u64,
                            prev_log_idx: u64, prev_log_term: u64, commit_idx: u64) {

        let aereq = ~AppendEntriesRequest{
            term: term,
            prev_log_idx: prev_log_idx,
            prev_log_term: prev_log_term,
            commit_idx: commit_idx,
            leader_id: LEADER_ID,
            entries: Vec::new(),
        };

        let encoded_aereq = serialize(aereq);
        let req_msg = format!("Length: {:u}\n{:s}", encoded_aereq.len(), encoded_aereq);

        let result = stream.write_str(req_msg);
        if result.is_err() {
            error!("Client ERROR: {:?}", result.err());
        }
        let _ = stream.flush();
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
        let uuid = Uuid::new_v4().to_hyphenated_str();
        let id_cmd = format!("Uuid: {}\n{}", uuid, cmd);
        let msg = format!("Length: {}\n{}", id_cmd.len(), id_cmd);
        try!(stream.write_str(msg));
        try!(stream.flush());
        Ok(())
    }

    // Params:
    //  num_to_send: number of commands to send
    //  first_num: first num in the PUT=x cmd
    fn send_client_cmds(num_to_send: uint, first_num: uint, ldr_socket_addr: SocketAddr) -> IoResult<()> {
        for i in range(first_num, first_num + num_to_send) {
            let mut stream = TcpStream::connect(ldr_socket_addr);
            try!(send_client_cmd(&mut stream, format!("PUT x={}", i)));
            let result = stream.read_to_str();
            drop(stream); // close the connection
        }
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

        let encoded_aereq = serialize(aereq);
        let req_msg = format!("Length: {:u}\n{:s}", encoded_aereq.len(), encoded_aereq);

        ////
        let result = stream.write_str(req_msg);
        if result.is_err() {
            error!("Client ERROR: {:?}", result.err());
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
    fn test_leader_with_2_followers_both_active() {
        setup();
        write_cfg_file(3);

        // start leader
        let start_result1 = start_server(1, Some(CfgOptions{init_state: Leader}));
        if start_result1.is_err() {
            fail!("{:?}", start_result1);
        }
        timer::sleep(50); // wait a short while for the leader to get set up and ready to msg followers

        // start follower, svr 2
        let start_result2 = start_server(2, None);
        if start_result2.is_err() {
            send_shutdown_signal(1);
            fail!("resul2.is_err: {:?}", start_result2);
        }

        // start follower, svr 3
        let result3 = start_server(3, None);
        if result3.is_err() {
            send_shutdown_signal(1);
            send_shutdown_signal(2);
            fail!("{:?}", result3);
        }

        timer::sleep(150); // wait a short while for all servers to get set up

        let ldr_addr = start_result1.unwrap();

        // now send client messages to leader
        // Client msg #1
        let mut stream = TcpStream::connect(ldr_addr);
        let send_result1 = send_client_cmd(&mut stream, ~"PUT x=1");
        if send_result1.is_err() {
            send_shutdown_signal(1);
            send_shutdown_signal(2);
            send_shutdown_signal(3);
            fail!(send_result1);
        }

        let result1 = stream.read_to_str();
        drop(stream); // close the connection

        // Client msg #2
        let mut stream = TcpStream::connect(ldr_addr);
        let send_result2 = send_client_cmd(&mut stream, ~"PUT x=2");
        if send_result2.is_err() {
            send_shutdown_signal(1);
            send_shutdown_signal(2);
            send_shutdown_signal(3);
            fail!(send_result2);
        }

        let result2 = stream.read_to_str();
        drop(stream); // close the connection

        // Client msg #3
        let mut stream = TcpStream::connect(ldr_addr);
        let send_result3 = send_client_cmd(&mut stream, ~"PUT x=3");
        if send_result3.is_err() {
            send_shutdown_signal(1);
            send_shutdown_signal(2);
            send_shutdown_signal(3);
            fail!(send_result3);
        }

        let result3 = stream.read_to_str();
        drop(stream); // close the connection

        timer::sleep(50); // wait a short while for all servers to get set up
        spawn(proc() {
            send_shutdown_signal(1);
        });
        spawn(proc() {
            send_shutdown_signal(2);
        });
        send_shutdown_signal(3);

        // validate after shutting down servers
        assert!(result1.is_ok());
        assert_eq!(~"200 OK", result1.unwrap());
        assert!(result2.is_ok());
        assert_eq!(~"200 OK", result2.unwrap());
        assert!(result3.is_ok());
        assert_eq!(~"200 OK", result3.unwrap());
    }

    // this one won't work until we build in the "catch-up"/repair mechanism for a
    // leader to resend messages to an out of date follower
    #[test]
    fn test_leader_with_2_followers_with_one_active_first_then_second_joins_later() {
        // FILL IN
    }


    #[test]
    fn test_leader_with_4_followers_being_hit_by_multiple_clients_simultaneously() {
        setup();
        write_cfg_file(5);

        // start leader
        let start_result1 = start_server(1, Some(CfgOptions{init_state: Leader}));
        if start_result1.is_err() {
            fail!("start_result1.is_err: {:?}", start_result1);
        }
        timer::sleep(20); // wait a short while for the leader to get set up and ready to msg followers

        // start follower, svr 2
        let start_result2 = start_server(2, None);
        if start_result2.is_err() {
            send_shutdown_signal(1);
            fail!("start_result2.is_err: {:?}", start_result2);
        }

        // start follower, svr 3
        let start_result3 = start_server(3, None);
        if start_result3.is_err() {
            send_shutdown_signal(1);
            send_shutdown_signal(2);
            fail!("send_result3.is_err: {:?}", start_result3);
        }

        // start follower, svr 4
        let start_result4 = start_server(4, None);
        if start_result4.is_err() {
            send_shutdown_signal(1);
            send_shutdown_signal(2);
            send_shutdown_signal(3);
            fail!("send_result4.is_err: {:?}", start_result4);
        }

        // start follower, svr 5
        let start_result5 = start_server(5, None);
        if start_result5.is_err() {
            send_shutdown_signal(1);
            send_shutdown_signal(2);
            send_shutdown_signal(3);
            send_shutdown_signal(4);
            fail!("send_result5.is_err: {:?}", start_result5);
        }

        timer::sleep(150); // wait a short while for all servers to get set up

        let ldr_addr = start_result1.unwrap();
        let ldr_addr1 = ldr_addr.clone();

        let (chsend1, chrecv1): (Sender<IoResult<()>>, Receiver<IoResult<()>>) = channel();
        let (chsend2, chrecv2): (Sender<IoResult<()>>, Receiver<IoResult<()>>) = channel();
        let (chsend3, chrecv3): (Sender<IoResult<()>>, Receiver<IoResult<()>>) = channel();
        let (chsend4, chrecv4): (Sender<IoResult<()>>, Receiver<IoResult<()>>) = channel();
        let (chsend5, chrecv5): (Sender<IoResult<()>>, Receiver<IoResult<()>>) = channel();

        spawn(proc() {
            timer::sleep(5);
            let result = send_client_cmds(3, 1, ldr_addr1);
            chsend1.send(result);
        });

        spawn(proc() {
            timer::sleep(5);
            let result = send_client_cmds(3, 4, ldr_addr1);
            chsend2.send(result);
        });

        spawn(proc() {
            timer::sleep(5);
            let result = send_client_cmds(3, 7, ldr_addr1);
            chsend3.send(result);
        });

        spawn(proc() {
            timer::sleep(5);
            let result = send_client_cmds(3, 10, ldr_addr1);
            chsend4.send(result);
        });

        spawn(proc() {
            timer::sleep(5);
            let result = send_client_cmds(3, 13, ldr_addr1);
            chsend5.send(result);
        });

        let clt1_result = chrecv1.recv();
        let clt2_result = chrecv2.recv();
        let clt3_result = chrecv3.recv();
        let clt4_result = chrecv4.recv();
        let clt5_result = chrecv5.recv();

        timer::sleep(500); // wait a short while for all messages to get handled

        spawn(proc() {
            send_shutdown_signal(2);
        });
        spawn(proc() {
            send_shutdown_signal(3);
        });
        spawn(proc() {
            send_shutdown_signal(4);
        });
        spawn(proc() {
            send_shutdown_signal(5);
        });
        send_shutdown_signal(1);

        assert!(clt1_result.is_ok());
        assert!(clt2_result.is_ok());
        assert!(clt3_result.is_ok());
        assert!(clt4_result.is_ok());
        assert!(clt5_result.is_ok());

        let s1_datalog_res = read_datalog(S1TEST_PATH);
        let s2_datalog_res = read_datalog(S2TEST_PATH);
        let s3_datalog_res = read_datalog(S3TEST_PATH);
        let s4_datalog_res = read_datalog(S4TEST_PATH);
        let s5_datalog_res = read_datalog(S5TEST_PATH);

        assert!(s1_datalog_res.is_ok());
        assert!(s2_datalog_res.is_ok());
        assert!(s3_datalog_res.is_ok());
        assert!(s4_datalog_res.is_ok());
        assert!(s5_datalog_res.is_ok());

        let s1_datalog = s1_datalog_res.unwrap();
        let s2_datalog = s2_datalog_res.unwrap();
        let s3_datalog = s3_datalog_res.unwrap();
        let s4_datalog = s4_datalog_res.unwrap();
        let s5_datalog = s5_datalog_res.unwrap();

        assert_eq!(15, s1_datalog.len());
        assert_eq!(15, s2_datalog.len());
        assert_eq!(15, s3_datalog.len());
        assert_eq!(15, s4_datalog.len());
        assert_eq!(15, s5_datalog.len());

        assert_eq!(s1_datalog, s5_datalog);
        for i in range(0, s1_datalog.len()) {
            assert_eq!(s1_datalog.get(i), s3_datalog.get(i));
            assert_eq!(s1_datalog.get(i), s4_datalog.get(i));
        }
    }

    // one follower experiences "network partition" phase, but since 2/3 of cluster still
    // up the commits should still happen
    // 
    #[test]
    fn test_leader_with_2_followers_initially_then_lose_1() {
        setup();
        write_cfg_file(3);

        // start leader
        let start_result1 = start_server(1, Some(CfgOptions{init_state: Leader}));
        if start_result1.is_err() {
            fail!("start_result1.is_err: {:?}", start_result1);
        }
        timer::sleep(20); // wait a short while for the leader to get set up and ready to msg followers

        // start follower, svr 2
        let start_result2 = start_server(2, None);
        if start_result2.is_err() {
            send_shutdown_signal(1);
            fail!("start_result2.is_err: {:?}", start_result2);
        }

        // start follower, svr 3
        let start_result3 = start_server(3, None);
        if start_result3.is_err() {
            send_shutdown_signal(1);
            send_shutdown_signal(2);
            fail!("send_result3.is_err: {:?}", start_result3);
        }

        timer::sleep(150); // wait a short while for all servers to get set up

        let ldr_addr = start_result1.unwrap();

        // now send client messages to leader
        // Client msg #1
        let mut stream = TcpStream::connect(ldr_addr);
        let send_result1 = send_client_cmd(&mut stream, ~"PUT x=1");
        if send_result1.is_err() {
            send_shutdown_signal(1);
            send_shutdown_signal(2);
            fail!("send_result1.is_err: {:?}", send_result1);
        }

        let result1 = stream.read_to_str();
        drop(stream); // close the connection

        // long pause to ensure hearbeats are maintaining leader position
        timer::sleep(200);

        // Client msg #2
        let mut stream = TcpStream::connect(ldr_addr);
        let send_result2 = send_client_cmd(&mut stream, ~"PUT x=2");
        if send_result2.is_err() {
            send_shutdown_signal(1);
            send_shutdown_signal(2);
            fail!("send_result2.is_err: {:?}", send_result2);
        }

        let result2 = stream.read_to_str();
        drop(stream); // close the connection

        // brief pause to ensure time to write to disk
        timer::sleep(25);

        // read state of datalogs at t1
        let t1s1_datalog_res = read_datalog(S1TEST_PATH);
        let t1s2_datalog_res = read_datalog(S2TEST_PATH);
        let t1s3_datalog_res = read_datalog(S3TEST_PATH);  // no file for this

        // now shutdown server 3 -> simulate network partition
        send_shutdown_signal(3);

        // Client msg #3 and #4 nearly simultaneously
        let mut stream3 = TcpStream::connect(ldr_addr);
        let mut stream4 = TcpStream::connect(ldr_addr);

        let send_result3 = send_client_cmd(&mut stream3, ~"PUT x=3");

        if send_result3.is_err() {
            send_shutdown_signal(1);
            send_shutdown_signal(2);
            fail!("send_result3.is_err: {:?}", send_result3);
        }

        let send_result4 = send_client_cmd(&mut stream4, ~"PUT x=4");
        if send_result4.is_err() {
            send_shutdown_signal(1);
            send_shutdown_signal(2);
            fail!("send_result4.is_err: {:?}", send_result4);
        }

        // brief pause to ensure time to distribute messages and write to disk
        timer::sleep(25);

        spawn(proc() {
            send_shutdown_signal(1);
        });
        send_shutdown_signal(2);

        // read state of datalogs at t2
        let t2s1_datalog_res = read_datalog(S1TEST_PATH);
        let t2s2_datalog_res = read_datalog(S2TEST_PATH);
        let t2s3_datalog_res = read_datalog(S3TEST_PATH);  // no file for this


        /* ---[ validate results ]--- */

        assert!(result1.is_ok());
        assert_eq!(~"200 OK", result1.unwrap());

        assert!(result2.is_ok());
        assert_eq!(~"200 OK", result2.unwrap());

        // t1: should be two entries in the svr1 and svr2 datalogs and none in svr3
        assert!(t1s1_datalog_res.is_ok());
        assert!(t1s2_datalog_res.is_ok());
        assert!(t1s3_datalog_res.is_ok());  // should be no file with that name

        let t1s1_datalog = t1s1_datalog_res.unwrap();
        let t1s2_datalog = t1s2_datalog_res.unwrap();
        let t1s3_datalog = t1s3_datalog_res.unwrap();

        assert_eq!(2, t1s1_datalog.len());
        assert_eq!(2, t1s2_datalog.len());
        assert_eq!(2, t1s3_datalog.len());

        assert!(t1s1_datalog.get(0).contains("PUT x=1"));
        assert_eq!(t1s1_datalog.get(0), t1s2_datalog.get(0));
        assert_eq!(t1s1_datalog.get(0), t1s3_datalog.get(0));

        assert!(t1s2_datalog.get(1).contains("PUT x=2"));
        assert_eq!(t1s1_datalog.get(1), t1s2_datalog.get(1));
        assert_eq!(t1s1_datalog.get(1), t1s3_datalog.get(1));


        // t2: should be two entries in all logs
        assert!(t2s1_datalog_res.is_ok());
        assert!(t2s2_datalog_res.is_ok());
        assert!(t2s3_datalog_res.is_ok());  // should be file now

        let t2s1_datalog = t2s1_datalog_res.unwrap();
        let t2s2_datalog = t2s2_datalog_res.unwrap();
        let t2s3_datalog = t2s3_datalog_res.unwrap();

        assert_eq!(4, t2s1_datalog.len());
        assert_eq!(4, t2s2_datalog.len());
        assert_eq!(2, t2s3_datalog.len());

        assert!(t2s1_datalog.get(2).contains("PUT x=3"));
        assert_eq!(t2s1_datalog.get(2), t2s2_datalog.get(2));

        assert!(t2s1_datalog.get(3).contains("PUT x=4"));
        assert_eq!(t2s1_datalog.get(3), t2s2_datalog.get(3));
    }

    // this one does not test the leader with any other Peers
    // it works right now since there is no majority committed logic
    // once majority committed logic is added this will hang since the client
    // won't get a response  => perhaps have to build in a timeout to respond
    // to the client if can't commit within 2 seconds or something?
    // 
    #[test]
    fn test_leader_simple() {
        // setup();
        // write_cfg_file(1);

        // let svr_id = 1;
        // let start_result = start_server(svr_id, Some(CfgOptions {init_state: Leader}));
        // if start_result.is_err() {
        //     fail!("{:?}", start_result);
        // }
        // timer::sleep(350);

        // let addr = start_result.unwrap();

        // // Client msg #1
        // let mut stream = TcpStream::connect(addr);
        // let send_result1 = send_client_cmd(&mut stream, ~"PUT x=1");
        // if send_result1.is_err() {
        //     send_shutdown_signal(svr_id);
        //     fail!(send_result1);
        // }

        // let result1 = stream.read_to_str();
        // drop(stream); // close the connection


        // // Client msg #2
        // let mut stream = TcpStream::connect(addr);
        // let send_result2 = send_client_cmd(&mut stream, ~"PUT x=2");
        // if send_result2.is_err() {
        //     send_shutdown_signal(svr_id);
        //     fail!(send_result2);
        // }

        // let result2 = stream.read_to_str();
        // drop(stream); // close the connection

        // send_shutdown_signal(svr_id);

        // assert!(result1.is_ok());
        // assert_eq!(~"200 OK", result1.unwrap());  // BOGUS tmp response

        // assert!(result2.is_ok());
        // assert_eq!(~"200 OK", result2.unwrap());  // BOGUS tmp response
    }

    #[test]
    fn test_follower_to_send_client_redirect_when_leader_is_unknown() {
        setup();
        write_cfg_file(3);
        let svr_id = 1;
        let start_result = start_server(svr_id, None);
        if start_result.is_err() {
            fail!("{:?}", start_result);
        }
        timer::sleep(350); // wait a short while for the leader to get set up and listening on its socket

        let addr = start_result.unwrap();
        let mut stream = TcpStream::connect(addr);
        let send_result = send_client_cmd(&mut stream, ~"PUT x=1");
        if send_result.is_err() {
            send_shutdown_signal(svr_id);
            fail!(send_result);
        }

        let result = stream.read_to_str();
        drop(stream); // close the connection

        send_shutdown_signal(svr_id);

        assert!(result.is_ok());
        let resp = result.unwrap();

        assert!( resp.contains("Redirect: 127.0.0.1:231") );
        assert!( !resp.contains("Redirect: 127.0.0.1:23158") );
    }

    #[test]
    fn test_follower_to_send_client_redirect_when_leader_is_known() {
        setup();
        write_cfg_file(3);
        let svr_id = 2;
        let start_result = start_server(svr_id, None);
        if start_result.is_err() {
            fail!("{:?}", start_result);
        }
        timer::sleep(350); // wait a short while for the leader to get set up and listening on its socket

        let addr = start_result.unwrap();
        let mut stream = TcpStream::connect(addr);

        // this message lets the follower know who the leader is (LEADER_ID = 1)
        let _ = send_aereq1(&mut stream);
        let result1 = stream.read_to_str();
        drop(stream); // close the connection


        // client cmd to follower => now should get back leader
        stream = TcpStream::connect(addr);
        let send_result = send_client_cmd(&mut stream, ~"PUT x=1");
        if send_result.is_err() {
            send_shutdown_signal(svr_id);
            fail!(send_result);
        }

        let result2 = stream.read_to_str();
        drop(stream); // close the connection

        send_shutdown_signal(svr_id);

        assert!(result1.is_ok());
        assert!(result2.is_ok());
        let resp = result2.unwrap();

        assert!(resp.contains(format!("Redirect: 127.0.0.1:{}", S1TEST_PORT)));
    }

    #[test]
    fn test_follower_with_single_AppendEntryRequest() {
        setup();
        write_cfg_file(1);
        let svr_id = 1;
        let start_result = start_server(svr_id, None);
        if start_result.is_err() {
            fail!("{:?}", start_result);
        }
        timer::sleep(350); // wait a short while for the leader to get set up and listening on its socket

        let addr = start_result.unwrap();
        let mut stream = TcpStream::connect(addr);

        // the test task acts as leader, sending AEReqs
        let logentry1 = send_aereq1(&mut stream);

        /* ---[ read response and signal server to shut down ]--- */

        let result1 = stream.read_to_str();
        drop(stream); // close the connection

        send_shutdown_signal(svr_id);

        /* ---[ validate results ]--- */

        assert!(result1.is_ok());
        let resp = result1.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(1, aeresp.term);
        assert_eq!(1, aeresp.idx);
        assert_eq!(1, aeresp.peer_id);
        assert_eq!(super::UNKNOWN, aeresp.commit_idx);  // since no ldr has established any commits is UNKNOWN
        assert_eq!(true, aeresp.success);

        // validate that response was written to disk
        let filepath = Path::new(S1TEST_PATH);
        assert!(filepath.exists());

        // read in first entry and make sure it matches the logentry sent in the AEReq
        let mut br = BufferedReader::new(File::open(&filepath));
        let mut readres = br.read_line();
        assert!(readres.is_ok());

        let line = readres.unwrap().trim().to_owned();
        let exp_str = serialize(&logentry1);
        assert_eq!(exp_str, line);

        // should only be one entry in the file
        readres = br.read_line();
        assert!(readres.is_err());
    }

    #[test]
    fn test_follower_where_one_entry_was_not_committed_on_leader_so_must_be_truncated() {
        setup();
        write_cfg_file(1);

        let svr_id = 1;
        let start_result = start_server(svr_id, None);
        if start_result.is_err() {
            fail!("{:?}", start_result);
        }
        timer::sleep(350); // wait a short while for the leader to get set up and listening on its socket

        let addr = start_result.unwrap();

        // test case task acts as leader sending AEReq

        /* ---[ send logentries 1 and 2 (term=1) ]--- */

        let mut stream = TcpStream::connect(addr);
        let terms: Vec<u64> = vec!(1,1,1);
        let indexes: Vec<u64> = vec!(1,2,3);
        let prev_log_idx = 0u64;
        let prev_log_term = 0u64;
        let logentries123 = send_aereqs(&mut stream, indexes, terms, prev_log_idx, prev_log_term);

        let result1 = stream.read_to_str();
        drop(stream);


        // now send hearbeat with prev_log_idx set to 2, meaning that idx 3 entry
        // was NOT committed, so the follower needs to truncate this entry
        let mut stream = TcpStream::connect(addr);
        let term = 1u64;
        let prev_log_idx = 2u64;
        let prev_log_term = 1u64;
        let commit_idx = 2u64;
        send_aereq_heartbeat(&mut stream, term, prev_log_idx, prev_log_term, commit_idx);

        let result2 = stream.read_to_str();
        drop(stream);

        // send second heartbeat
        let mut stream = TcpStream::connect(addr);
        let term = 1u64;
        let prev_log_idx = 2u64;
        let prev_log_term = 1u64;
        let commit_idx = 2u64;
        send_aereq_heartbeat(&mut stream, term, prev_log_idx, prev_log_term, commit_idx);

        let result3 = stream.read_to_str();
        drop(stream);

        send_shutdown_signal(svr_id);

        /* ---[ validate results ]--- */

        // validate response => result 1 should be success = true

        assert!(result1.is_ok());
        let resp = result1.unwrap();

        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(1, aeresp.term);
        assert_eq!(3, aeresp.idx);
        assert_eq!(1, aeresp.peer_id);
        assert_eq!(1, aeresp.commit_idx);  // since no ldr has established any commits is UNKNOWN


        // result 2 should be success=false

        assert!(result2.is_ok());
        let resp2 = result2.unwrap();

        let aeresp2 = append_entries::AppendEntriesResponse::decode(resp2).unwrap();
        assert_eq!(false, aeresp2.success);
        assert_eq!(1, aeresp2.term);
        assert_eq!(2, aeresp2.idx);        // this has dropped to 2
        assert_eq!(1, aeresp2.peer_id);
        assert_eq!(1, aeresp2.commit_idx);  // commit_idx doesn't get updated when response is false


        // result 3 should be success=true

        assert!(result3.is_ok());
        let resp3 = result3.unwrap();

        let aeresp3 = append_entries::AppendEntriesResponse::decode(resp3).unwrap();
        assert_eq!(true, aeresp3.success);
        assert_eq!(1, aeresp3.term);
        assert_eq!(2, aeresp3.idx);
        assert_eq!(1, aeresp3.peer_id);
        assert_eq!(2, aeresp3.commit_idx);  // commit_idx should now be updated on success

        // validate that response was written to disk
        let filepath = Path::new(S1TEST_PATH);
        assert!(filepath.exists());

        // read in first entry and make sure it matches the logentry sent in the AEReq
        let mut br = BufferedReader::new(File::open(&filepath));
        let readres = br.read_line();
        assert!(readres.is_ok());

        let line = readres.unwrap().trim().to_owned();
        let exp_str = serialize(&logentries123.get(0));
        assert_eq!(exp_str, line);

        let readres = br.read_line();
        assert!(readres.is_ok());

        let line = readres.unwrap().trim().to_owned();
        let exp_str = serialize(&logentries123.get(1));
        assert_eq!(exp_str, line);

        // should only be two entries in the file (not three)
        assert!(br.read_line().is_err());
    }


    // REMOVE ME !!!!!!!!!!!
    // #[test]
    // fn test_follower_with_same_AppendEntryRequest_twice_should_return_false_2nd_time() {
    //     setup();
    //     write_cfg_file(1);

    //     let svr_id = 1;
    //     let start_result = start_server(svr_id, None);
    //     if start_result.is_err() {
    //         fail!("{:?}", start_result);
    //     }
    //     timer::sleep(350); // wait a short while for the leader to get set up and listening on its socket

    //     let addr = start_result.unwrap();
    //     let mut stream = TcpStream::connect(addr);

    //     // test case task acts as leader sending AEReq

    //     /* ---[ send logentry1 (term1) ]--- */

    //     let logentry1 = send_aereq1(&mut stream);
    //     let result1 = stream.read_to_str();
    //     drop(stream); // close the connection


    //     /* ---[ send logentry1 again (term1) ]--- */

    //     stream = TcpStream::connect(addr);
    //     let _ = send_aereq1(&mut stream);
    //     let result2 = stream.read_to_str();
    //     drop(stream); // close the connection

    //     send_shutdown_signal(svr_id);

    //     /* ---[ validate results ]--- */

    //     // validate response => result 1 should be success = true

    //     assert!(result1.is_ok());
    //     let resp = result1.unwrap();

    //     assert!(resp.contains("\"success\":true"));
    //     let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
    //     assert_eq!(1, aeresp.term);
    //     assert_eq!(1, aeresp.idx);
    //     assert_eq!(1, aeresp.peer_id);
    //     assert_eq!(super::UNKNOWN, aeresp.commit_idx);  // since no ldr has established any commits is UNKNOWN
    //     assert_eq!(true, aeresp.success);


    //     // result 2 should be success = false

    //     assert!(result2.is_ok());
    //     let resp2 = result2.unwrap();

    //     assert!(resp2.contains("\"success\":false"));
    //     let aeresp2 = append_entries::AppendEntriesResponse::decode(resp).unwrap();
    //     assert_eq!(1, aeresp2.term);
    //     assert_eq!(1, aeresp2.idx);
    //     assert_eq!(1, aeresp.peer_id);
    //     assert_eq!(super::UNKNOWN, aeresp.commit_idx);  // since no ldr has established any commits is UNKNOWN
    //     assert_eq!(true, aeresp2.success);


    //     // validate that response was written to disk
    //     let filepath = Path::new(S1TEST_PATH);
    //     assert!(filepath.exists());

    //     // read in first entry and make sure it matches the logentry sent in the AEReq
    //     let mut br = BufferedReader::new(File::open(&filepath));
    //     let mut readres = br.read_line();
    //     assert!(readres.is_ok());

    //     let line = readres.unwrap().trim().to_owned();
    //     let exp_str = serialize(&logentry1);
    //     assert_eq!(exp_str, line);

    //     // should only be one entry in the file
    //     readres = br.read_line();
    //     assert!(readres.is_err());
    // }

    #[test]
    fn test_follower_with_multiple_valid_AppendEntryRequests() {
        setup();
        write_cfg_file(1);
        let svr_id = 1;
        let start_result = start_server(svr_id, None);
        if start_result.is_err() {
            fail!("{:?}", start_result);
        }
        timer::sleep(350); // wait a short while for the leader to get set up and listening on its socket

        let addr = start_result.unwrap();
        let mut stream = TcpStream::connect(addr);
        let terms: Vec<u64> = vec!(1, 1, 1, 1);
        let indexes: Vec<u64> = vec!(1, 2, 3, 4);
        // let prev_log_idx = 0u64;  // TODO: does this need to be used?
        let prev_log_term = 0u64;
        let logentries: Vec<LogEntry> = send_aereqs(&mut stream, indexes, terms, prev_log_term, prev_log_term);

        /* ---[ read response and signal server to shut down ]--- */

        let result1 = stream.read_to_str();
        drop(stream); // close the connection   ==> NEED THIS? ask on #rust

        send_shutdown_signal(svr_id);

        /* ---[ validate ]--- */
        assert_eq!(4, logentries.len());

        assert!(result1.is_ok());
        let resp = result1.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(1, aeresp.term);
        assert_eq!(4, aeresp.idx);
        assert_eq!(1, aeresp.peer_id);
        assert_eq!(1, aeresp.commit_idx); // with send_aereqs, commit_idx == first idx of indexes passed in
    }

    #[test]
    fn test_follower_with_AppendEntryRequests_with_invalid_term() {
        setup();
        write_cfg_file(1);
        let svr_id = 1;
        let start_result = start_server(svr_id, None);
        if start_result.is_err() {
            fail!("{:?}", start_result);
        }
        timer::sleep(350); // wait a short while for the leader to get set up and listening on its socket

        let addr = start_result.unwrap();
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

        send_shutdown_signal(svr_id);

        /* ---[ validate ]--- */

        assert_eq!(1, logentries1.len());
        assert_eq!(2, logentries2.len());
        assert_eq!(1, logentries3.len());

        // result 1
        assert!(result1.is_ok());
        let resp = result1.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(1, aeresp.term);
        assert_eq!(1, aeresp.idx);
        assert_eq!(1, aeresp.peer_id);
        assert_eq!(1, aeresp.commit_idx);  // with send_aereqs, commit_idx == first idx of indexes passed in

        // result 2
        assert!(result2.is_ok());
        let resp = result2.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(3, aeresp.idx);
        assert_eq!(1, aeresp.peer_id);
        assert_eq!(2, aeresp.commit_idx);  // with send_aereqs, commit_idx == first idx of indexes passed in

        // result 3
        assert!(result3.is_ok());
        let resp = result3.unwrap();

        assert!(resp.contains("\"success\":false"));
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(false, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(3, aeresp.idx);
        assert_eq!(1, aeresp.peer_id);
        assert_eq!(2, aeresp.commit_idx);  // this req failed, so is same commit_idx as prev
    }

    #[test]
    fn test_follower_with_AppendEntryRequests_with_invalid_prevLogTerm() {
        setup();
        write_cfg_file(1);

        let svr_id = 1;
        let start_result = start_server(svr_id, None);
        if start_result.is_err() {
            fail!("{:?}", start_result);
        }
        timer::sleep(350); // wait a short while for the leader to get set up and listening on its socket

        let addr = start_result.unwrap();
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

        send_shutdown_signal(svr_id);

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
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(1, aeresp.term);
        assert_eq!(2, aeresp.idx);
        assert_eq!(1, aeresp.peer_id);
        assert_eq!(1, aeresp.commit_idx); // with send_aereqs, commit_idx == first idx of indexes passed in

        // result 2
        assert!(result2.is_ok());
        let resp = result2.unwrap();

        assert!(resp.contains("\"success\":false"));
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(false, aeresp.success);
        assert_eq!(1, aeresp.term);
        assert_eq!(1, aeresp.idx);
        assert_eq!(1, aeresp.peer_id);
        assert_eq!(1, aeresp.commit_idx); // req failed, so same as prev

        // result 3
        assert!(result3.is_ok());
        let resp = result3.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(3, aeresp.idx);
        assert_eq!(1, aeresp.peer_id);
        assert_eq!(2, aeresp.commit_idx); // with send_aereqs, commit_idx == first idx of indexes passed in
    }


    #[test]
    fn test_follower_with_AppendEntryRequests_with_changing_commit_idx() {
        setup();
        write_cfg_file(1);
        let svr_id = 1;
        let start_result = start_server(svr_id, None);
        if start_result.is_err() {
            fail!("{:?}", start_result);
        }
        timer::sleep(350); // wait a short while for the leader to get set up and listening on its socket

        let addr = start_result.unwrap();
        let mut stream = TcpStream::connect(addr);

        /* ---[ AER 1, valid, initial ]--- */
        let mut terms: Vec<u64> = vec!(1,1);
        let mut indexes: Vec<u64> = vec!(1,2);
        let prev_log_idx = 0u64;
        let prev_log_term = 0u64;
        let commit_idx = super::UNKNOWN;  // first send out -> no consensus, so nothing committed
        let logentries1: Vec<LogEntry> = send_aereqs_with_commit_idx(&mut stream, indexes, terms, prev_log_idx, prev_log_term, commit_idx);
        let result1 = stream.read_to_str();
        drop(stream);

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
        drop(stream);

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
        drop(stream);

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

        send_shutdown_signal(svr_id);

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
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(1, aeresp.term);
        assert_eq!(2, aeresp.idx);
        assert_eq!(super::UNKNOWN, aeresp.commit_idx);  // key test
        assert_eq!(1, aeresp.peer_id);

        // result 2
        assert!(result2.is_ok());
        let resp = result2.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(3, aeresp.idx);
        assert_eq!(2, aeresp.commit_idx);  // key test => should match what leader indicated

        // result 3
        assert!(result3.is_ok());
        let resp = result3.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(7, aeresp.idx);
        assert_eq!(2, aeresp.commit_idx);  // key test => should match what leader indicated

        // result 4
        assert!(result4.is_ok());
        let resp = result4.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(7, aeresp.idx);
        assert_eq!(6, aeresp.commit_idx);  // key test => should match what leader indicated

        // result 5
        assert!(result5.is_ok());
        let resp = result5.unwrap();

        assert!(resp.contains("\"success\":true"));
        let aeresp = append_entries::AppendEntriesResponse::decode(resp).unwrap();
        assert_eq!(true, aeresp.success);
        assert_eq!(2, aeresp.term);
        assert_eq!(9, aeresp.idx);
        assert_eq!(9, aeresp.commit_idx);  // key test => 9, not 10 (as leader said)
    }
}
