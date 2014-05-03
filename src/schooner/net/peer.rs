use std::io::net::ip::SocketAddr;
use std::io::net::tcp::TcpStream;
use std::option::Option;
use super::super::events::*;

// Bare RPC types. This is the incoming type before we set up channels
// to make Raft messages. Might not be necessary, provided we can setup
// those channels in the functions where the RPCs are built out of network
// bytes.
#[deriving(Decodable, Encodable)]
pub enum RaftRpc {
    RpcARQ(AppendEntriesReq),
    RpcARS(AppendEntriesRes),
    RpcVRQ(VoteReq),
    RpcVRS(VoteRes),
    RpcStopReq,
}

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
    config: NetPeerConfig,
    // If we have an open connection to this peer, then this will be Some(...).
    pub stream: Option<~TcpStream>,
    // When we iterate over the NetPeers, we can check if we have a reply from
    // an AppendEntriesReq/etc. on this receiver.
    // In this case, this means when we send an AppendEntriesReq up to Schooner,
    // we need to assign this as the receiving end of the AppendEntriesReq's
    // to_leader channel.
    pub rpc_recv: Option<~Receiver<RaftRpc>>,
}

impl NetPeer {
    /*
     * sender: the port we can use to send received commands back to the server.
     * should be a clone of a single Sender attached to a single Receiver, basically.
     * since each command will contain a port we can use for replies, we don't
     * need two ports in the arguments.
     */
    fn new(config: NetPeerConfig, sender: Sender<RaftRpc>) -> NetPeer {
        NetPeer {
            config: config,
            stream: None,
            rpc_recv: None,
        }
    }

    /*
     * Try connecting to the peer from here. Note that this is only one way
     * we can establish a peer connection - the other way is if the peer tries
     * to connect to *us*.
     */
    fn try_spawn(&mut self) -> Option<~Sender<RaftRpc>> {
        match TcpStream::connect(self.config.address) {
            Ok(stream) => {
                self.stream = Some(~stream);
                let (rpc_send, rpc_recv): (Sender<RaftRpc>, Receiver<RaftRpc>) = channel();
                self.rpc_recv = Some(~rpc_recv);
                Some(~rpc_send)
            }
            err => {
                None
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
    fn add_connection(&mut self, rpc_recv: ~Receiver<RaftRpc>, stream: ~TcpStream) -> bool {
        if self.stream.is_some() || self.rpc_recv.is_some() {
            // Failure!
            return false;
        }
        self.stream = Some(stream);
        self.rpc_recv = Some(rpc_recv);
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

    /*
     * If this peer's cmd_recv has a RaftCmd waiting, then send the RaftCmd to
     * the node.
     */
    fn reply(mut self) -> bool {
        match self.rpc_recv {
            Some(mut rpc_recv) => {
                match rpc_recv.recv() {
                    RpcARQ(ae_req) => {
                        // TODO: Send ARQ to this peer
                        return false;
                    },
                    RpcARS(ae_res) => {
                        // TODO: send ARS to peer
                        return false;
                    },
                    RpcVRQ(vote_req) => {
                        // TODO: send VRQ to peer
                        return false;
                    },
                    RpcVRS(vote_res) => {
                        // TODO: send VRS to peer
                        return false;
                    },
                    RpcStopReq => {
                        return false;
                    }
                }
            }
            None => {
                return false;
            }
        }
    }
}

// TODO: Get the old parsing code out of the Git history and work it into
// this configuration.
