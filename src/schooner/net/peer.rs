use std::io::BufferedReader;
use std::io::net::ip::SocketAddr;
use std::io::net::tcp::TcpStream;
use std::option::Option;
use uuid::{Uuid, UuidVersion, Version4Random};
use super::super::events::*;
use super::parsers::{read_rpc, as_network_msg};
use super::types::*;

static CONNECT_TIMEOUT: u64 = 3000;

// Each peer should have one of these, and they should be consistent across
// nodes.
pub struct NetPeer {
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
    pub fn spawn(id: uint, conf: NetPeerConfig, to_raft: Sender<RaftMsg>) -> Sender<MgmtMsg> {
        let (mgmt_send, mgmt_port) = channel();
        spawn(proc() {
            let netpeer = NetPeer::new(conf, to_raft, mgmt_port);
        });
        mgmt_send
    }

    
    fn new(config: NetPeerConfig, to_raft: Sender<RaftMsg>, mgmt_port: Receiver<MgmtMsg>) -> NetPeer {
        NetPeer {
            conf: config,
            stream: None,
            to_raft: to_raft,
            mgmt_port: mgmt_port,
        }
    }

    fn try_connect(&mut self) -> Option<TcpStream> {
        match TcpStream::connect_timeout(self.conf.address, CONNECT_TIMEOUT) {
            Ok(mut stream) => {
                stream.write_uint(self.conf.id);
                debug!("Sent handshake req to {}", self.conf.address);
                Some(stream.clone())
            }
            err => {
                None
            }
        }
    }

    fn listen(&mut self) {
        while(self.stream.is_none()) {
            match self.mgmt_port.try_recv() {
                Ok(msg) => {
                    match msg {
                        AttachStreamMsg(id, stream) => {
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
                    self.try_connect();
                }
            }
        }
        if self.stream.is_none() {
            return;
        }
        let stream = self.stream.clone().unwrap();
        let sender = self.to_raft.clone();
        loop {
            let either_rpc = read_rpc(stream.clone());
            match either_rpc {
                Ok(rpc) => {
                    match rpc {
                        RpcARQ(aereq) => {
                            let (resp_send, resp_recv) = channel();
                            self.to_raft.send(ARQ(aereq, resp_send));
                            let aeres = resp_recv.recv();
                            stream.write(as_network_msg(RpcARS(aeres)));
                        }
                        RpcARS(aeres) => {
                            self.to_raft.send(ARS(aeres));
                        }
                        RpcVRQ(votereq) => {
                            let (resp_send, resp_recv) = channel();
                            self.to_raft.send(VRQ(votereq, resp_send));
                            let voteres = resp_recv.recv();
                            stream.write(as_network_msg(RpcVRS(voteres)));
                        }
                        RpcVRS(voteres) => {
                            self.to_raft.send(VRS(voteres));
                        }
                        RpcStopReq => {
                            self.to_raft.send(StopReq);
                        }
                    }
                }
                Err(e) => {
                    debug!("Dropped peer: {}", e);
                    break;
                }
            }
        }
        self.stream = None;
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
    use super::super::parsers::{as_network_msg};

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
        let mut peer1_sd = NetPeer::spawn(2, pc.clone(), send1);
        let mut peer2_sd = NetPeer::spawn(3, pc, send2);
        let listen_addr = SocketAddr {
            ip: Ipv4Addr(127, 0, 0, 1),
            port: 8844,
        };
        let listener: TcpListener = TcpListener::bind(listen_addr).unwrap();
        let mut acceptor: TcpAcceptor = listener.listen().unwrap();
        // Spawn two peers
        let mut count = 0;
        // Send each peer the vote
        let vote = RpcVRQ(VoteReq {
            term: 0,
            candidate_id: 0,
            last_log_index: 0,
            last_log_term: 0,
            uuid: Uuid::new(Version4Random).unwrap(),
        });
        for mut stream in acceptor.incoming() {
            let vote_bytes = as_network_msg(vote.clone());
            stream.write(vote_bytes);
            count += 1;
            if count > 1 {
                break;
            }
        }
        let mut replies = 0;
        // We should get the votes back out on the port that we were waiting on
        match recv1.recv() {
            VRQ(recvote, chan) => {
                assert!(recvote.uuid = vote.uuid);
            }
            _ => { fail!(); }
        }
        match recv2.recv() {
            VRQ(recvote, chan) => {
                assert!(recvote.uuid = vote.uuid);
            }
            _ => { fail!(); }
        }
        drop(acceptor);
    }
}
