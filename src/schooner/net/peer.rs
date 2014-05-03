use std::io::BufferedReader;
use std::io::net::ip::SocketAddr;
use std::io::net::tcp::TcpStream;
use std::option::Option;
use uuid::{Uuid, UuidVersion, Version4Random};
use super::super::events::*;
use super::parsers::{read_network_msg};

// Bare RPC types. This is the incoming type before we set up channels
// to make Raft messages. Might not be necessary, provided we can setup
// those channels in the functions where the RPCs are built out of network
// bytes.
#[deriving(Decodable, Encodable, Eq)]
pub enum RaftRpc {
    RpcARQ(AppendEntriesReq),
    RpcARS(AppendEntriesRes),
    RpcVRQ(VoteReq),
    RpcVRS(VoteRes),
    RpcStopReq,
}

#[deriving(Clone, Hash, Eq, TotalEq)]
pub struct NetPeerConfig {
    pub id: uint,
    // The port for this field is the peer's *listening* port, not necessarily the
    // port we will get our request from. Hence the peer should send its id when it
    // makes a new request to us.
    pub address: SocketAddr,
}

// Each peer should have one of these, and they should be consistent across
// nodes.
pub struct NetPeer {
    config: ~NetPeerConfig,
    // If we have an open connection to this peer, then this will be Some(...).
    pub stream: Option<~TcpStream>,
    // When we iterate over the NetPeers, we can check if we have a reply from
    // an AppendEntriesReq/etc. on this receiver.
    // In this case, this means when we send an AppendEntriesReq up to Schooner,
    // we need to assign this as the receiving end of the AppendEntriesReq's
    // to_leader channel.
    pub rpc_send: ~Sender<RaftRpc>,
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
            config: config,
            stream: None,
            rpc_send: sender,
        }
    }

    /*
     * Try connecting to the peer from here. Note that this is only one way
     * we can establish a peer connection - the other way is if the peer tries
     * to connect to *us*.
     */
    fn try_spawn(&mut self) -> bool {
        match TcpStream::connect(self.config.address) {
            Ok(stream) => {
                self.stream = Some(~stream.clone());
                let sender = self.rpc_send.clone();
                spawn(proc() {
                    let msg = read_network_msg(BufferedReader::new(stream.clone()));
                    debug!("{}", msg);
                    // TODO: Actually read/parse.
                    let vote = RpcVRQ(VoteReq {
                        id: 0,
                        uuid: Uuid::new(Version4Random).unwrap(),
                    });
                    sender.send(vote);
                });
                true
            }
            err => {
                false
            }
        }
    }

    fn listen(&mut self, stream: &TcpStream) {

    }

    /*
     * If the node chose to connect to us, then we got a connection on our listening
     * address and need to give the stream to us here.
     * 
     * Returns: True if we successfully connected, false if we thought we already had
     * an open connection to this peer (this is an invalid state; we should probably
     * crash or handle it somehow).
     */
    fn add_connection(&mut self, stream: ~TcpStream) -> bool {
        if self.stream.is_some() {
            return false;
        }
        self.stream = Some(stream);
        true
    }

    /*
     * Used by the leader to send commands to followers, and by candidates, etc.
     */
    fn send(&mut self, cmd: RaftRpc) -> Option<Receiver<RaftRpc>> {
        if self.stream.is_none() {
            return None;
        }
        let (rpc_send, rpc_recv) = channel();
        // TODO:
        // spawn(proc() {
        //     self.stream.unwrap().write( /* serialize cmd */ );
        //     reply = /* wait for a reply on the TCP connection */
        //     // Probably we should break the channel if the TCPstream dies,
        //     // so the leader will know we didn't get a reply.
        //     rpc_send.send(reply);
        // });
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
    use super::{NetPeer, NetPeerConfig, RaftRpc, RpcVRQ};
    use super::super::parsers::frame_msg;

    /*
     * Can we parse content length fields?
     */
    #[test]
    fn test_spawn() {
        let pc1 = ~NetPeerConfig {
            id: 1,
            address: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 8844,
            },
        };
        let pc2 = ~NetPeerConfig {
            id: 1,
            address: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 8844,
            },
        };
        let (send1, recv1) = channel();
        let (send2, recv2) = channel();
        let mut peer1 = NetPeer::new(pc1, ~send1);
        let mut peer2 = NetPeer::new(pc2, ~send2);
        let listen_addr = SocketAddr {
            ip: Ipv4Addr(127, 0, 0, 1),
            port: 8844,
        };
        let listener: TcpListener = TcpListener::bind(listen_addr).unwrap();
        let mut acceptor: TcpAcceptor = listener.listen().unwrap();
        peer1.try_spawn();
        peer2.try_spawn();
        let mut count = 0;
        for mut stream in acceptor.incoming() {
            let msg = frame_msg("Hello world", 1);
            stream.write(msg.as_bytes());
            count += 1;
            if count > 1 {
                break;
            }
        }
        let mut replies = 0;
        match recv1.recv() {
            RpcVRQ(vote) => {
                assert!(vote.id == 0);
                replies += 1;
            },
            _ => fail!(),
        }
        match recv2.recv() {
            RpcVRQ(vote) => {
                assert!(vote.id == 0);
                replies += 1;
            },
            _ => fail!(),
        }
        drop(acceptor);
        assert!(count == 2);
        assert!(replies == 2);
    }
}
