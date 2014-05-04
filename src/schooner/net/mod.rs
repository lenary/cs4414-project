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

pub use self::peer::NetPeer;
use super::events::*;
use self::types::*;
pub mod peer;

// Private stuff, shouldn't be used elsewhere.
pub mod parsers;
mod types;

static CONNECT_TIMEOUT: u64 = 3000;

macro_rules! may_shutdown(
    ($p: ident) => {
        match $p.try_recv() {
            Ok(exitcode) => {
                break;
            }
            Err(_) => {},
        }
    }
)

// TODO: put entry functions in here, like:
// - start_peer_helper (which returns a Peer struct, including fields
// with channels to message with)
// TODO: Peer will contain identity info and channels for
// communicating with this peer task

/*
 * Contains all the information on what peers we have, and manages Schooner's interactions
 * with them.
 */
pub struct NetListener {
    // Maps peer IDs to their associated TCP streams.
    // Since these streams might fail, we need to reestablish them sometimes.
    peer_id_map: HashMap<uint, TcpStream>,
    // Peer configurations, stored in a concurrency construct.
    peer_configs: Vec<NetPeerConfig>,
}

impl NetListener {
    pub fn new() -> NetListener {
        NetListener {
            peer_id_map: HashMap::new(),
            peer_configs: Vec::new(),
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
    pub fn startup_peers(mut self, conf: NetPeerConfig, peer_configs: ~Vec<NetPeerConfig>, from_peers_send: Sender<RaftMsg>) {
        for conf in peer_configs.iter() {
            self.peer_configs.push(conf.clone());
        }
        let (peer_connect_send, peer_connect_recv) = channel();
        let (peer_shutdown_send, peer_shutdown_recv) = channel();
        NetListener::listen_peers(conf.address, peer_connect_send, peer_shutdown_recv);
        /*
        let selector = Select::new();
        for conf in peer_configs.iter() {
            let (send, recv) = channel();
            let netpeer = NetPeer::new(~conf.clone(), ~send);
            let mut peer_handle = selector.handle(&recv);
            unsafe { peer_handle.add(); }
            self.peer_select_map.insert(peer_handle.id(), netpeer.conf.id);
            self.peer_id_map.insert(conf.id, netpeer);
        }
        let listener = TcpListener::new();
        a.set_timeout(Some(100));
        match a.accept() {
            Ok(..) => println!("accepted a socket"),
            Err(ref e) if e.kind == TimedOut => { println!("timed out!"); }
            Err(e) => println!("err: {}", e),
        }
        */
    }

    fn listen_peers(addr: SocketAddr, peer_send: Sender<(uint, TcpStream)>, shutdown: Receiver<uint>) {
        spawn(proc() {
            let listener: TcpListener = TcpListener::bind(addr).unwrap();
            let mut acceptor: TcpAcceptor = listener.listen().unwrap();
            debug!("Started listening on {}", addr);
            loop {
                acceptor.set_timeout(Some(5000));
                for maybe_stream in acceptor.incoming() {
                    match maybe_stream {
                        Ok(stream) => {
                            let mut reader = ~BufferedReader::new(stream);
                            let line = reader.read_line();
                            let id: Option<uint> = line.ok().and_then(|l| from_str(l));
                            if id.is_some() {
                                debug!("Handshake with peer {} completed.", id.unwrap());
                                peer_send.send((id.unwrap(), reader.unwrap()));
                            }
                            else {
                                let mut stream = reader.unwrap();
                                debug!("Dropping peer at {} never got an id.", stream.peer_name());
                                drop(stream);
                            }
                        }
                        Err(e) => {
                            break;
                        }
                    }
                }
                may_shutdown!(shutdown);
            }
        });
    }

    fn try_connect(&mut self, peer_id: uint) -> Option<TcpStream> {
        let ref peer_configs = self.peer_configs;
        let mut peer: Option<&NetPeerConfig> = None;
        for conf in peer_configs.iter() {
            if conf.id == peer_id {
                peer = Some(conf);
            }
        }
        if peer.is_none() {
            return None;
        }
        match TcpStream::connect_timeout(peer.unwrap().address, CONNECT_TIMEOUT) {
            Ok(mut stream) => {
                stream.write_uint(peer.unwrap().id);
                stream.write_line("");
                Some(stream.clone())
            }
            err => {
                None
            }
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
        for maybe_stream in acceptor.incoming() {
            match maybe_stream {
                Ok(stream) => {
                    // TODO
                }
                Err(e) => {
                    debug!("{}", e);
                    continue;
                }
            }
        }
    }

    /*
     * Get the configurations of the known peers for this network listener. Maybe
     * only used by the leader to get the potential peers that it can send messages
     * to. Probably should send even peers that NetListener thinks are down, since 
     * in case they come back up the leader might like to send requests to them.
     */
    fn get_peer_ids(& self) -> Vec<uint> {
        /*
        let mut confs = Vec::new();
        for conf in self.peer_id_map.keys() {
            confs.push(conf.clone());
        }
        confs
         */
        Vec::new()
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

}

#[cfg(test)]
mod test {
    use std::vec::Vec;
    use std::io::net::ip::{SocketAddr, Ipv4Addr};
    use std::io::net::tcp::TcpStream;
    use std::io::timer::sleep;
    use std::io::BufferedWriter;
    use std::container::MutableSet;
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
        /*
        let mut nl = NetListener::new();
        let (from_peers_send, from_peers) = channel();
        nl.spawn_peer_listener(pc, pc_vec, from_peers_send);
        */
    }

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
        let (peer_connect_send, peer_connect_recv) = channel();
        let (shutdown_send, shutdown_recv) = channel();
        let listen_addr = SocketAddr {
            ip: Ipv4Addr(127, 0, 0, 1),
            port: 9999,
        };
        NetListener::listen_peers(listen_addr, peer_connect_send, shutdown_recv);
        sleep(1000);
        connect_handshake(14, listen_addr);
        let (res_id, _) = peer_connect_recv.recv();
        assert_eq!(res_id, 14);
        connect_handshake(10, listen_addr);
        let (res_id, _) = peer_connect_recv.recv();
        assert_eq!(res_id, 10);
        connect_handshake(1, listen_addr);
        let (res_id, _) = peer_connect_recv.recv();
        assert_eq!(res_id, 1);
        shutdown_send.send(0);
    }
}
