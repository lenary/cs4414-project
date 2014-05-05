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

use super::types::{RaftRpc, NetPeerConfig, MgmtMsg};
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
        let (peerlistener_shutdown, peerlistener_new) = listen_peers(conf.id, conf.address);
        let clientlistener_shutdown = listen_clients(conf.id, conf.client_addr, from_client_send);
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

    fn main_loop(&mut self) {
        let select = Select::new();
        let h_shutdown = select.handle(&self.shutdown_signal);
    }

}

#[cfg(test)]
mod test {
    // TODO
}
