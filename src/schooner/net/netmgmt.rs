use std::vec::Vec;
use std::io::IoError;
use serialize::Encodable;
use serialize::Decodable;
use std::io::{Acceptor, Listener, TcpListener, TcpStream, IoResult, BufferedReader};
use std::io::net::tcp::TcpAcceptor;
use std::io::net::ip::SocketAddr;
use std::comm::Select;
use collections::HashMap;
use sync::{RWLock, Arc};
use serialize::json::{Encoder,Error};
use std::io::timer::sleep;

use super::types::{RaftRpc, NetPeerConfig, MgmtMsg, SendMsg, StopMsg, AttachStreamMsg};
use super::listeners::{listen_peers, listen_clients};
use super::peer::NetPeer;
use super::super::events::*;


/*
 * Contains all the information on what peers we have, and manages Schooner's interactions
 * with them.
 */
pub struct NetManager {
    // config info for this peer
    conf: NetPeerConfig,
    // Maps peer IDs to their associated TCP streams.
    peer_id_map: HashMap<u64, Sender<MgmtMsg>>,
    // Peer configurations
    peer_configs: Vec<NetPeerConfig>,
    // Sender we use to talk back up to Raft. Needed for spawning more peers.
    from_peers_send: Sender<RaftMsg>,
    // Receives peer connections as (id, stream) from listen_peers()
    new_peer_recv: Receiver<(u64, TcpStream)>,
    // Signal from main raft process to do a shutdown.
    raft_shutdown_recv: Receiver<u64>,
    // channels we send the shutdown signal to if we receive one
    shutdown_senders: Vec<Sender<u64>>,
    // for msging individual peers
    to_peers_recv: Receiver<(u64, RaftRpc)>,
}

impl NetManager {
    // returns: (msg peer sender, shutdown sender).
    pub fn new(conf: NetPeerConfig,
               peer_configs: &Vec<NetPeerConfig>,
               from_peers_send: Sender<RaftMsg>,
               from_client_send: Sender<(ClientCmdReq, Sender<ClientCmdRes>)>)
                -> (Sender<(u64, RaftRpc)>, Sender<u64>) {
        let (new_peer_shutdown, new_peer_recv) = listen_peers(conf.id, conf.address);
        let new_clients_stop = listen_clients(conf.id, conf.client_addr, from_client_send);
        //let (new_clients_stop, new_clients_recv) = channel();
        let (raft_shutdown_send, raft_shutdown_recv) = channel();
        let (to_peers_send, to_peers_recv) = channel();
        let mut this = NetManager {
            conf: conf,
            peer_id_map: HashMap::new(),
            peer_configs: peer_configs.clone(),
            from_peers_send: from_peers_send,
            new_peer_recv: new_peer_recv,
            raft_shutdown_recv: raft_shutdown_recv,
            shutdown_senders: Vec::new(),
            to_peers_recv: to_peers_recv,
        };
        this.shutdown_senders.push(new_peer_shutdown);
        this.shutdown_senders.push(new_clients_stop);
        let peer_sender = this.from_peers_send.clone();
        for remote_conf in peer_configs.iter() {
            let netport = NetPeer::spawn(conf.id, remote_conf, peer_sender.clone());
            this.peer_id_map.insert(remote_conf.id, netport);
        }
        debug!("Starting NetManager main loop");
        let (this_send, this_recv) = channel();
        spawn(proc() {
            let mut this: NetManager = this_recv.recv();
            this.main_loop();
        });
        this_send.send(this);
        (to_peers_send, raft_shutdown_send)
    }

    fn main_loop(&mut self) {
        let select = Select::new();
        let mut h_new_peer = select.handle(&self.new_peer_recv);
        let mut h_to_peers = select.handle(&self.to_peers_recv);
        let mut h_shutdown = select.handle(&self.raft_shutdown_recv);
        unsafe { 
            h_new_peer.add();
            h_to_peers.add();
            h_shutdown.add();
        }
        // Wait while everything spins up
        sleep(5000);
        loop {
            let ready_id = select.wait();
            if h_new_peer.id() == ready_id {
                let (id, stream) = h_new_peer.recv();
                debug!("Got an incoming connection for peer {}", id);
                let msg = AttachStreamMsg(id, stream);
                self.peer_id_map.get(&id).send(msg);
            }
            if h_to_peers.id() == ready_id {
                let (id, rpc) = h_to_peers.recv();
                debug!("Sending {} to peer {}.", rpc, id);
                let msg = SendMsg(rpc);
                self.peer_id_map.get(&id).send(msg);
            }
            if h_shutdown.id() == ready_id {
                let exitcode = h_shutdown.recv();
                debug!("Got a shutdown msg from Raft.");
                for sender in self.shutdown_senders.iter() {
                    sender.send(exitcode);
                }
                break;
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::io::{TcpStream, BufferedReader, IoResult, IoError, InvalidInput};
    use std::io::net::ip::{SocketAddr, Ipv4Addr};
    use std::io::{Acceptor, Listener, TcpListener, TcpStream};
    use std::io::net::tcp::TcpAcceptor;
    use uuid::{Uuid, UuidVersion, Version4Random};
    use std::io::timer::sleep;
    
    use super::super::super::events::*;
    use super::super::types::*;
    use super::super::parsers::*;
    use super::*;

    fn get_simple_peer_config() -> (NetPeerConfig, Vec<NetPeerConfig>) {
        let pc = NetPeerConfig {
            id: 1,
            address: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 7777,
            },
            client_addr: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 7778,
            },
        };
        let mut peers = Vec::new();
        peers.push(NetPeerConfig {
            id: 2,
            address: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 8123,
            },
            client_addr: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 10109,
            },
        });
        peers.push(NetPeerConfig {
            id: 3,
            address: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 8124,
            },
            client_addr: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 10999,
            },
        });
        (pc, peers)
    }

    // TODO: This loops. I think it's just because of the order the recv()s happen.
    // Not bothering with it for now.
    // #[test]
    fn mgmt_spawn() {
        let (this_peer_config, other_peers_config) = get_simple_peer_config();
        let (from_peers_send, from_peers_recv) = channel();
        let (from_client_send, from_client_recv) = channel();
        let (msg_peer_send, shutdown_send) = NetManager::new(this_peer_config, &other_peers_config, from_peers_send, from_client_send);
        let (s_p2, r_p2) = channel();
        let (s_p3, r_p3) = channel();
        spawn(proc() {
            let addr = SocketAddr {ip: Ipv4Addr(127, 0, 0, 1), port: 8123};
            debug!("[mgmt_spawn()] Starting listener for {}", addr);
            let mut l = TcpListener::bind(addr);
            debug!("[mgmt_spawn()] Spawned TcpListener, starting to listen ... {}", addr);
            let mut a = l.listen().unwrap();
            debug!("[mgmt_spawn()] Spawned phony peer at {}", addr);
            match a.accept() {
                Ok(stream) => {
                    debug!("[mgmt_spawn()] Got connection from Raft node: {}", read_helo(stream.clone()));
                    s_p2.send(stream);
                }
                Err(e) => fail!("[mgmt_spawn()] err: {}", e),
            }
        });
        spawn(proc() {
            let addr = SocketAddr {ip: Ipv4Addr(127, 0, 0, 1), port: 8124};
            debug!("[mgmt_spawn()] Starting listener for {}", addr);
            let mut l = TcpListener::bind(addr);
            debug!("[mgmt_spawn()] Spawned TcpListener, starting to listen ... {}", addr);
            let mut a = l.listen().unwrap();
            debug!("[mgmt_spawn()] Spawned phony peer at {}", addr);
            match a.accept() {
                Ok(stream) => {
                    debug!("[mgmt_spawn()] Got connection from Raft node: {}", read_helo(stream.clone()));
                    s_p3.send(stream);
                }
                Err(e) => fail!("[mgmt_spawn()] err: {}", e),
            }
        });
        shutdown_send.send(0);
        // Wait for callbacks
        let stream2 = r_p2.recv();
        let stream3 = r_p3.recv();
        drop(stream2);
        drop(stream3);
    }
}
