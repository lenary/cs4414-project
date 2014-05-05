use std::vec::Vec;
use std::io::IoError;
use serialize::Encodable;
use serialize::Decodable;
use std::io::{Acceptor, Listener, TcpListener, TcpStream, IoResult, BufferedReader};
use std::io::net::tcp::TcpAcceptor;
use std::io::net::ip::SocketAddr;
use std::comm::Select;
use std::comm::Disconnected;
use collections::HashMap;
use sync::{RWLock, Arc};
use serialize::json::{Encoder,Error};
use std::io::timer::sleep;

use super::peer::*;
use super::types::*;
use super::super::events::*;

static CONNECT_TIMEOUT: u64 = 3000;

macro_rules! may_shutdown(
    ($p: ident) => {
        match $p.try_recv() {
            Ok(exitcode) => {
                debug!("Received shutdown signal {} on subsystem.", exitcode);
                break;
            },
            Err(Disconnected) => {
                fail!("Channel broke.");
            },
            _ => {},
        }
    };
    ($s: ident, $p: ident) => {
        match $s.$p.try_recv() {
            Ok(exitcode) => {
                debug!("Received shutdown signal {} on NetListener.", exitcode);
                for sender in $s.shutdown_senders.iter() {
                    sender.send(exitcode);
                }
                break;
            },
            Err(Disconnected) => {
                fail!("Channel broke.");
            },
            _ => {},
        }
    };
)

macro_rules! try_update (
    ($s: ident, $p: ident, $m: ident) => {
        match $s.$p.try_recv() {
            Ok((incoming_id, incoming_stream)) => {
                let msg_port = $s.$m.get(&incoming_id);
                msg_port.send((incoming_id, incoming_stream));
            }
            Err(Disconnected) => {
                fail!("Channel broke.");
            }
            _ => {},
        }
    };
)

/*
 * Contains all the information on what peers we have, and manages Schooner's interactions
 * with them.
 */
pub struct NetManager {
    // config info for this peer
    conf: NetPeerConfig,
    // Maps peer IDs to their associated TCP streams.
    peer_id_map: HashMap<uint, Sender<MgmtMsg>>,
    // Peer configurations
    peer_configs: Vec<NetPeerConfig>,
    // Sender we use to talk back up to Raft
    from_peers_send: Sender<RaftMsg>,
    // Receives peer connections as (id, stream) from listen_peers()
    peerlistener_new: Receiver<(uint, TcpStream)>,
    // Signal from main raft process to do a shutdown.
    shutdown_signal: Receiver<uint>,
    // channels we send the shutdown signal to if we receive one
    shutdown_senders: Vec<Sender<uint>>,
    // for msging individual peers
    to_peers_recv: Receiver<(uint, RaftRpc)>,
}

impl NetManager {
    // returns: (msg peer sender, shutdown sender).
    pub fn new(conf: NetPeerConfig,
               peer_configs: &Vec<NetPeerConfig>,
               from_peers_send: Sender<RaftMsg>,
               from_client_send: Sender<(ClientCmdReq, Sender<ClientCmdRes>)>)
                -> (Sender<(uint, RaftRpc)>, Sender<uint>) {
        let (peerlistener_shutdown, peerlistener_new) = NetManager::listen_peers(conf.id, conf.address);
        let clientlistener_shutdown = NetManager::listen_clients(conf.id, conf.client_addr, from_client_send);
        let (shutdown_send, shutdown_recv) = channel();
        let (to_peers_send, to_peers_recv) = channel();
        let mut this = NetManager {
            conf: conf,
            peer_id_map: HashMap::new(),
            peer_configs: peer_configs.clone(),
            from_peers_send: from_peers_send,
            to_peers_recv: to_peers_recv,
            peerlistener_new: peerlistener_new,
            shutdown_signal: shutdown_recv,
            shutdown_senders: Vec::new(),
        };
        this.shutdown_senders.push(peerlistener_shutdown);
        this.shutdown_senders.push(clientlistener_shutdown);
        let peer_sender = this.from_peers_send.clone();
        for remote_conf in peer_configs.iter() {
            let netport = NetPeer::spawn(conf.id, remote_conf, peer_sender.clone());
            this.peer_id_map.insert(remote_conf.id, netport);
        }
        this.main_loop();
        (to_peers_send, shutdown_send)
    }

    /*
     * Leader only
     */
    fn send_append_req(& mut self, peer: uint, cmd: AppendEntriesReq) -> Option<Receiver<AppendEntriesRes>> {
        // if we were able to send the data over the TCP connection, then we will send it
        // back out on the returned Receiver.
        /*
        let netpeer = self.peer_id_map.get(&peer);
        match netpeer.send(RpcARQ(cmd)) {
            Some(reschan) => {
                // TODO
                None
            }
            None => {
                None
            }
        }
         */
        None
    }
    
    fn send_vote_req(peer: NetPeerConfig, cmd: VoteReq) -> Option<Receiver<VoteRes>> {
        None
    }

    fn main_loop(&mut self) {
        loop {
            may_shutdown!(self, shutdown_signal);
        }
    }
    
    fn listen_peers(this_id: uint, addr: SocketAddr) -> (Sender<uint>, Receiver<(uint, TcpStream)>) {
        let (shutdown_send, shutdown) = channel();
        let (peer_send, peer_recv) = channel();
        spawn(proc() {
            let listener: TcpListener = TcpListener::bind(addr).unwrap();
            let mut acceptor: TcpAcceptor = listener.listen().unwrap();
            debug!("{}: Started listening for peers @ {}", this_id, addr);
            loop {
                acceptor.set_timeout(Some(CONNECT_TIMEOUT));
                for maybe_stream in acceptor.incoming() {
                    match maybe_stream {
                        Ok(mut stream) => {
                            debug!("{}: got a connection from {}", this_id, stream.peer_name());
                            let mut reader = ~BufferedReader::new(stream);
                            let line = reader.read_line();
                            let id: Option<uint> = line.ok().and_then(|l| from_str(l));
                            if id.is_some() {
                                let id = id.unwrap();
                                let mut stream = reader.unwrap();
                                debug!("{}: identified {} as peer {}.", this_id, stream.peer_name(), id);
                                peer_send.send((id, stream));
                            }
                            else {
                                let mut stream = reader.unwrap();
                                debug!("{}: Dropping peer at {} never got an id.", this_id, stream.peer_name());
                                drop(stream);
                            }
                        }
                        Err(e) => {
                            break;
                        }
                    }
                }
                may_shutdown!(shutdown);
                debug!("{}: listening ...", this_id);
            }
        });
        (shutdown_send, peer_recv)
    }

    /*
     * Start the client listener.
     *
     * conf: the config for this server, containing *client* listen address.
     * from_clients_send: sends messages from peers back to the main loop.
     *
     */
    fn listen_clients(this_id: uint, addr: SocketAddr, from_client_send: Sender<(ClientCmdReq, Sender<ClientCmdRes>)>) -> Sender<uint> {
        // unwrapping because our server is dead in the water if it can't listen on its assigned port
        let (shutdown_send, shutdown_signal) = channel();
        spawn(proc() {
            let listener: TcpListener = TcpListener::bind(addr).unwrap();
            let mut acceptor: TcpAcceptor = listener.listen().unwrap();
            debug!("{}: Started listening for clients @ {}", this_id, addr);
            loop {
                acceptor.set_timeout(Some(CONNECT_TIMEOUT));
                for maybe_stream in acceptor.incoming() {
                    match maybe_stream {
                        Ok(stream) => {
                            // TODO!
                        }
                        Err(e) => {
                            break;
                        }
                    }
                }
                may_shutdown!(shutdown_signal);
            }
        });
        shutdown_send
    }
}

#[cfg(test)]
mod test {
    use std::vec::Vec;
    use std::io::net::ip::{SocketAddr, Ipv4Addr};
    use std::io::net::tcp::TcpStream;
    use std::io::timer::sleep;
    use std::io::BufferedWriter;
    use std::container::MutableSet;
    use super::super::types::*;
    use super::NetManager;

    fn connect_handshake(id: uint, addr: SocketAddr) {
        match TcpStream::connect(addr) {
            Ok(mut stream) => {
                let mut writer = BufferedWriter::new(stream);
                writer.write_uint(id);
                drop(writer.unwrap());
            }
            Err(e) => {
                fail!("{}", e);
            }
        }
    }

    #[test]
    fn test_listen_peers() {
        let listen_addr = SocketAddr {
            ip: Ipv4Addr(127, 0, 0, 1),
            port: 9999,
        };
        let (shutdown_send, new_peer_recv) = NetManager::listen_peers(1, listen_addr);
        sleep(1000);
        connect_handshake(14, listen_addr);
        let (res_id, _) = new_peer_recv.recv();
        assert_eq!(res_id, 14);
        connect_handshake(10, listen_addr);
        let (res_id, _) = new_peer_recv.recv();
        assert_eq!(res_id, 10);
        connect_handshake(1, listen_addr);
        let (res_id, _) = new_peer_recv.recv();
        assert_eq!(res_id, 1);
        shutdown_send.send(0);
    }
}
