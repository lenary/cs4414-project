use std::io::BufferedReader;
use std::io::net::ip::SocketAddr;
use std::io::net::tcp::TcpStream;
use std::option::Option;
use uuid::{Uuid, UuidVersion, Version4Random};
use super::super::events::*;
use super::parsers::{read_rpc, as_network_msg};
use super::types::*;

// Each peer should have one of these, and they should be consistent across
// nodes.
pub struct NetPeer {
    pub conf: ~NetPeerConfig,
    // If we have an open connection to this peer, then this will be Some(...).
    pub stream: Option<TcpStream>,
    // When we iterate over the NetPeers, we can check if we have a reply from
    // an AppendEntriesReq/etc. on this receiver.
    // In this case, this means when we send an AppendEntriesReq up to Schooner,
    // we need to assign this as the receiving end of the AppendEntriesReq's
    // to_leader channel.
    rpc_send: ~Sender<RaftRpc>,
}

impl NetPeer {
    /*
     * sender: the port we can use to send received commands back to the server.
     * should be a clone of a single Sender attached to a single Receiver, basically.
     * since each command will contain a port we can use for replies, we don't
     * need two ports in the arguments.
     */
    pub fn new(config: ~NetPeerConfig, sender: ~Sender<RaftRpc>) -> NetPeer {
        NetPeer {
            conf: config,
            stream: None,
            rpc_send: sender,
        }
    }

    /*
     * Try connecting to the peer from here. Note that this is only one way
     * we can establish a peer connection - the other way is if the peer tries
     * to connect to *us*.
     */
    pub fn try_spawn(&mut self) -> bool {
        if self.stream.is_some() {
            return false;
        }
        match TcpStream::connect_timeout(self.conf.address, 10000) {
            Ok(mut stream) => {
                stream.write_uint(self.conf.id);
                stream.write_line("");
                self.stream = Some(stream.clone());
                self.listen();
                true
            }
            err => {
                false
            }
        }
    }

    fn listen(&mut self) {
        if self.stream.is_none() {
            return;
        }
        let stream = self.stream.clone().unwrap();
        let sender = self.rpc_send.clone();
        spawn(proc() {
            loop {
                let either_rpc = read_rpc(stream.clone());
                match either_rpc {
                    Ok(rpc) => {
                        sender.send(rpc);
                    }
                    Err(e) => {
                        debug!("Dropped peer: {}", e);
                        break;
                    }
                }
            }
        });
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
    pub fn add_connection(&mut self, stream: &TcpStream) -> bool {
        if self.stream.is_some() {
            return false;
        }
        self.stream = Some(stream.clone());
        true
    }

    /*
     * Used by the leader to send commands to followers, and by candidates, etc.
     */
    pub fn send(&mut self, cmd: RaftRpc) -> Option<Receiver<RaftRpc>> {
        if self.stream.is_none() {
            return None;
        }
        let (rpc_send, rpc_recv) = channel();
        let stream = self.stream.clone().unwrap();
        spawn(proc() {
            //stream.write(as_network_msg(cmd));
            // TODO: replies.
            /*
            reply = // wait for a reply on the TCP connection
            // Probably we should break the channel if the TCPstream dies,
            // so the leader will know we didn't get a reply.
            rpc_send.send(reply);
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
        let pc = ~NetPeerConfig {
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
        let mut peer1 = NetPeer::new(pc.clone(), ~send1);
        let mut peer2 = NetPeer::new(pc, ~send2);
        let listen_addr = SocketAddr {
            ip: Ipv4Addr(127, 0, 0, 1),
            port: 8844,
        };
        let listener: TcpListener = TcpListener::bind(listen_addr).unwrap();
        let mut acceptor: TcpAcceptor = listener.listen().unwrap();
        // Spawn two peers
        peer1.try_spawn();
        peer2.try_spawn();
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
        assert!(recv1.recv() == vote);
        assert!(recv2.recv() == vote);
        drop(acceptor);
    }
}
