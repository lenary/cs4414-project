use std::vec::Vec;
use std::io::IoError;
use serialize::Encodable;
use serialize::Decodable;
use std::io::{Acceptor, Listener, TcpListener, TcpStream, IoResult, BufferedReader};
use std::io::net::tcp::TcpAcceptor;
use std::comm::Select;
use collections::HashMap;
use sync::{Mutex, Arc};
use serialize::json::{Encoder,Error};

pub use self::peer::NetPeer;
use super::events::*;
use self::types::*;
pub mod peer;

// Private stuff, shouldn't be used elsewhere.
pub mod parsers;
mod types;

// TODO: put entry functions in here, like:
// - start_peer_helper (which returns a Peer struct, including fields
// with channels to message with)
// TODO: Peer will contain identity info and channels for
// communicating with this peer task

/*
 * Contains all the information on what peers we have, and manages Schooner's interactions
 * with them.
 */
struct NetListener {
    peer_select_map: HashMap<uint, uint>,
    peer_id_map: HashMap<uint, NetPeer>,
}

impl NetListener {
    fn new() -> NetListener {
        NetListener {
            peer_select_map: HashMap::new(),
            peer_id_map: HashMap::new()
        }
    }
    
    /*
     * Start the listener for this server. This needs to open a TCP listening port
     * at whatever its configured listening address is, and it needs to have a way
     * to send commands that it has received from peers back out to the main loop.
     *
     * conf: the config for this server
     * peer_configs: vector of configs for peers
     * from_peers_send: sends messages from peers back to the main loop.
     *
     */
    fn spawn_peer_listener(mut self, conf: NetPeerConfig, peer_configs: ~Vec<NetPeerConfig>, from_peers_send: Sender<RaftMsg>) {
        // unwrapping because our server is dead in the water if it can't listen on its assigned port
        let selector = Select::new();
        for conf in peer_configs.iter() {
            let (send, recv) = channel();
            let netpeer = NetPeer::new(~conf.clone(), ~send);
            let mut peer_handle = selector.handle(&recv);
            unsafe { peer_handle.add(); }
            self.peer_select_map.insert(peer_handle.id(), netpeer.conf.id);
            self.peer_id_map.insert(conf.id, netpeer);
        }
    }

    /*
     * Start the client listener.
     *
     * conf: the config for this server, containing *client* listen address.
     * from_clients_send: sends messages from peers back to the main loop.
     *
     */
    fn spawn_client_listener(& mut self, conf: NetPeerConfig, from_client_send: Sender<(ClientCmdReq, Sender<ClientCmdRes>)>) {
        // unwrapping because our server is dead in the water if it can't listen on its assigned port
        let listener: TcpListener = TcpListener::bind(conf.address).unwrap();
        let mut acceptor: TcpAcceptor = listener.listen().unwrap();
        for stream in acceptor.incoming() {
            
        }
    }

    /*
     * Get the configurations of the known peers for this network listener. Maybe
     * only used by the leader to get the potential peers that it can send messages
     * to. Probably should send even peers that NetListener thinks are down, since 
     * in case they come back up the leader might like to send requests to them.
     */
    fn get_peer_ids(& self) -> Vec<uint> {
        let mut confs = Vec::new();
        for conf in self.peer_id_map.keys() {
            confs.push(conf.clone());
        }
        confs
    }

    /*
     * Leader only
     */
    fn send_append_req(& mut self, peer: uint, cmd: AppendEntriesReq) -> Option<Receiver<AppendEntriesRes>> {
        // if we were able to send the data over the TCP connection, then we will send it
        // back out on the returned Receiver.
        let netpeer = self.peer_id_map.get(&peer);
        /*
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

}

#[cfg(test)]
mod test {
    use std::vec::Vec;
    use std::io::net::ip::{SocketAddr, Ipv4Addr};
    use std::io::net::tcp::TcpStream;
    use super::types::*;
    use super::NetListener;
    
    #[test]
    fn test_warmup() {
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
        let mut pc_vec = ~Vec::new();
        pc_vec.push(NetPeerConfig {
            id: 1,
            address: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 8888,
            },
            client_addr: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 8840,
            },
        });
        let mut nl = NetListener::new();
        let (from_peers_send, from_peers) = channel();
        nl.spawn_peer_listener(pc, pc_vec, from_peers_send);
    }
}
