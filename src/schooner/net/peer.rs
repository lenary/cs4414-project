use std::io::BufferedReader;
use std::io::net::ip::SocketAddr;
use std::io::net::tcp::TcpStream;
use std::option::Option;
use std::io::timer::sleep;
use uuid::{Uuid, UuidVersion, Version4Random};
use super::super::events::*;
use super::parsers::{read_rpc, as_network_msg, make_id_bytes};
use super::types::*;

static CONNECT_TIMEOUT: u64 = 3000;

// Each peer should have one of these, and they should be consistent across
// nodes.
pub struct NetPeer {
    pub id: uint,
    pub conf: NetPeerConfig,
    // If we have an open connection to this peer, then this will be Some(...).
    pub stream: Option<TcpStream>,
    to_raft: Sender<RaftMsg>,
    mgmt_port: Receiver<MgmtMsg>,
}

impl NetPeer {

    /*
     * id: id of local Raft server
     * conf: configuration for network peer
     * to_raft: Sender for telling Raft about network messages
     * mgmt_port: for peer manager
     */
    pub fn spawn(id: uint, conf: &NetPeerConfig, to_raft: Sender<RaftMsg>) -> Sender<MgmtMsg> {
        let (mgmt_send, mgmt_port) = channel();
        let conf = conf.clone();
        spawn(proc() {
            let mut netpeer = NetPeer::new(id, conf, to_raft, mgmt_port);
            netpeer.peer_loop();
        });
        mgmt_send
    }

    
    fn new(id: uint, config: NetPeerConfig, to_raft: Sender<RaftMsg>, mgmt_port: Receiver<MgmtMsg>) -> NetPeer {
        NetPeer {
            id: id,
            conf: config,
            stream: None,
            to_raft: to_raft,
            mgmt_port: mgmt_port,
        }
    }

    fn try_connect(&mut self) -> Option<TcpStream> {
        match TcpStream::connect_timeout(self.conf.address, CONNECT_TIMEOUT) {
            Ok(mut stream) => {
                stream.write(make_id_bytes(self.id));
                debug!("[{}] Sent handshake req to {}", self.id, self.conf.address);
                Some(stream.clone())
            }
            Err(_) => {
                None
            }
        }
    }

    fn peer_loop(&mut self) {
        while(self.stream.is_none()) {
            debug!("[{}] No stream, trying to attach one.", self.id);
            match self.mgmt_port.try_recv() {
                Ok(msg) => {
                    match msg {
                        AttachStreamMsg(id, mut stream) => {
                            if id == self.conf.id {
                                self.attach_stream(stream);
                            }
                        }
                        _ => {
                            // TODO: send something on failure?
                        }
                    }
                }
                _ => {
                }
            }
            let connection = self.try_connect();
            if connection.is_some() {
                if self.attach_stream(connection.clone().unwrap()) {
                }
                else {
                    debug!("[{}] Dropping connection: already have one", self.id);
                    drop(connection)
                }
            };
        }
        let mut stream = self.stream.clone().unwrap();
        let sender = self.to_raft.clone();
        debug!("[{}] Attached stream from {}.", self.id, stream.peer_name());
        debug!("[{}] Entering peer loop", self.id);
        loop {
            sleep(CONNECT_TIMEOUT);
            let either_rpc = read_rpc(stream.clone());
            debug!("[{}] Got msg: {}", self.id, either_rpc);
            match either_rpc {
                Ok(rpc) => {
                    self.send_rpc(rpc, stream.clone());
                }
                Err(e) => {
                    self.stream = None;
                    debug!("[{}] Dropped peer: {}", self.id, e);
                    break;
                }
            }
        }
        debug!("[{}] Broke from main loop", self.id);
        self.stream = None;
    }

    /*
     * Send an RPC up to Raft, waiting for a reply if we need to.
     */
    fn send_rpc(&self, rpc: RaftRpc, mut stream: TcpStream) -> bool {
        match rpc {
            RpcARQ(aereq) => {
                debug!("[{}] Received ARQ: {}", self.id, aereq);
                let (resp_send, resp_recv) = channel();
                self.to_raft.send(ARQ(aereq, resp_send));
                let aeres = resp_recv.recv();
                stream.write(as_network_msg(RpcARS(aeres)));
                true
            }
            RpcARS(aeres) => {
                debug!("[{}] Received ARS: {}", self.id, aeres);
                self.to_raft.send(ARS(aeres));
                true
            }
            RpcVRQ(votereq) => {
                debug!("[{}] Received VRQ: {}", self.id, votereq);
                let (resp_send, resp_recv) = channel();
                self.to_raft.send(VRQ(votereq, resp_send));
                let voteres = resp_recv.recv();
                stream.write(as_network_msg(RpcVRS(voteres)));
                true
            }
            RpcVRS(voteres) => {
                debug!("[{}] Received VRS: {}", self.id, voteres);
                self.to_raft.send(VRS(voteres));
                true
            }
            RpcStopReq => {
                debug!("[{}] Received RpcStop", self.id);
                self.to_raft.send(StopReq);
                false
            }
        }
    }

    /*
     * If the node chose to connect to us, then we got a connection on our listening
     * address and need to give the stream to us here.
     * 
     * Returns: True if we successfully connected, false if we thought we already had
     * an open connection to this peer (this is an invalid state; we should probably
     * crash or handle it somehow).
     */
    pub fn attach_stream(&mut self, stream: TcpStream) -> bool {
        if self.stream.is_some() {
            return false;
        }
        self.stream = Some(stream);
        true
    }

    /*
     * Used by the leader to send commands to followers, and by candidates, etc.
     */
    pub fn send(&mut self, cmd: RaftRpc) -> Option<Receiver<RaftRpc>> {
        if self.stream.is_none() {
            return None;
        }
        let (to_raft, rpc_recv) = channel();
        let stream = self.stream.clone().unwrap();
        spawn(proc() {
            //stream.write(as_network_msg(cmd));
            // TODO: replies.
            /*
            reply = // wait for a reply on the TCP connection
            // Probably we should break the channel if the TCPstream dies,
            // so the leader will know we didn't get a reply.
            to_raft.send(reply);
            */
         });
        Some(rpc_recv)
    }
}

// TODO: Get the old parsing code out of the Git history and work it into
// this configuration.

#[cfg(test)]
mod test {
    use std::io::{TcpStream, BufferedReader, IoResult, IoError, InvalidInput};
    use std::io::net::ip::{SocketAddr, Ipv4Addr};
    use std::io::{Acceptor, Listener, TcpListener, TcpStream};
    use std::io::net::tcp::TcpAcceptor;
    
    use super::super::super::events::*;
    use uuid::{Uuid, UuidVersion, Version4Random};
    use super::NetPeer;
    use super::super::types::*;
    use super::super::parsers::*;

    /*
     * Can we parse content length fields?
     */
    #[test]
    fn test_spawn() {
        let pc = NetPeerConfig {
            id: 1,
            address: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 8844,
            },
            client_addr: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 8840,
            },
        };
        let (send1, recv1) = channel();
        let (send2, recv2) = channel();
        let mut peer1_sd = NetPeer::spawn(2, &pc.clone(), send1);
        let mut peer2_sd = NetPeer::spawn(3, &pc, send2);
        let listen_addr = SocketAddr {
            ip: Ipv4Addr(127, 0, 0, 1),
            port: 8844,
        };
        let listener: TcpListener = TcpListener::bind(listen_addr).unwrap();
        let mut acceptor: TcpAcceptor = listener.listen().unwrap();
        // Spawn two peers
        let mut count = 0;
        // Send each peer the vote
        let vote = VoteReq {
            term: 0,
            candidate_id: 0,
            last_log_index: 0,
            last_log_term: 0,
            uuid: Uuid::new(Version4Random).unwrap(),
        };
        let from_raft_voteres = VoteRes {
            term: 0,
            vote_granted: true,
            uuid: Uuid::new(Version4Random).unwrap(),
        };
        for mut stream in acceptor.incoming() {
            let vote_bytes = as_network_msg(RpcVRQ(vote.clone()));
            debug!("[test_spawn()] {}", read_helo(stream.clone()));
            stream.write(vote_bytes);
            count += 1;
            debug!("[test_spawn()] Sent {} vote requests.", count);
            if count > 1 {
                break;
            }
        }
        let mut replies = 0;
        // We should get the votes back out on the port that we were waiting on
        debug!("test_spawn(): waiting for replies");
        spawn(proc() {
            match recv1.recv() {
                VRQ(recvote, chan) => {
                    assert!(recvote.uuid == vote.uuid);
                    debug!("[test_spawn()] Sending reply from raft: {}", from_raft_voteres);
                    chan.send(from_raft_voteres);
                }
                _ => { fail!(); }
            }
        });
        match recv2.recv() {
            VRQ(recvote, chan) => {
                assert!(recvote.uuid == vote.uuid);
                debug!("[test_spawn()] Sending reply from raft: {}", from_raft_voteres);
                chan.send(from_raft_voteres);
            }
            _ => { fail!(); }
        }
        peer1_sd.send(StopMsg);
        peer2_sd.send(StopMsg);
        drop(acceptor);
    }
}
