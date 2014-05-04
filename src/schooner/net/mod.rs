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
    ($p: ident, $m: ident) => {
        match $p.try_recv() {
            Ok((incoming_id, incoming_stream)) => {
                let has_stream: bool = $m.get(&incoming_id).is_some();
                if !has_stream {
                    $m.insert(incoming_id, Some(incoming_stream));
                }
            }
            Err(Disconnected) => {
                fail!("Channel broke.");
            }
            _ => {},
        }
    };
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
    // config info for this peer
    conf: NetPeerConfig,
    // Maps peer IDs to their associated TCP streams.
    peer_configs: ~Vec<NetPeerConfig>,
    // Sender we use to talk back up to Raft
    shutdown_sender: Sender<uint>,
}

impl NetListener {
    pub fn new(conf: NetPeerConfig,
    peer_configs: ~Vec<NetPeerConfig>,
    from_peers_send: Sender<RaftMsg>,
    from_client_send: Sender<(ClientCmdReq, Sender<ClientCmdRes>)>) -> NetListener {
        let (shutdown_sender, shutdown_receiver) = channel();
        let mut this = NetListener {
            conf: conf,
            peer_configs: peer_configs.clone(),
            shutdown_sender: shutdown_sender,
        };
        NetListener::main_loop(conf, peer_configs, from_peers_send, from_client_send,
                       shutdown_receiver);
        this
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

    fn shutdown(&mut self) {
        self.shutdown_sender.send(0);
    }
    
    fn send_vote_req(peer: NetPeerConfig, cmd: VoteReq) -> Option<Receiver<VoteRes>> {
        None
    }

    fn main_loop(conf: NetPeerConfig,
        configs: &Vec<NetPeerConfig>,
        from_peers_send: Sender<RaftMsg>,
        from_client_send: Sender<(ClientCmdReq, Sender<ClientCmdRes>)>,
        shutdown_signal: Receiver<uint>) {
        spawn(proc() {
            let mut id_map: HashMap<uint, Option<TcpStream>> = HashMap::new();
            let mut shutdown_senders = Vec::new();
            let (peer_connect_send, peer_connect_recv):
                (Sender<(uint, TcpStream)>, Receiver<(uint, TcpStream)>) = channel();
            let (peer_shutdown_send, peer_shutdown_recv) = channel();
            NetListener::listen_peers(conf.id, conf.address, peer_connect_send, peer_shutdown_recv);
            let (client_shutdown_send, client_shutdown_recv) = channel();
            NetListener::listen_clients(conf.id, conf.client_addr, from_client_send, client_shutdown_recv);
            shutdown_senders.push(peer_shutdown_send);
            shutdown_senders.push(client_shutdown_send);
            for conf in configs.iter() {
                id_map.insert(conf.id, None);
                try_update!(peer_connect_recv, id_map);
            }
            loop {
                may_shutdown!(shutdown_signal);
                try_update!(peer_connect_recv, id_map);
                NetListener::connect_peers(conf.id, configs, &id_map);
                sleep(200);
            }
        });
    }
    
    fn connect_peers(this_id: uint, configs: &Vec<NetPeerConfig>, id_map: &HashMap<uint, Option<TcpStream>>) {
        for conf in configs.iter() {
            let id = conf.id;
            if id_map.get(&id).is_none() {
                debug!("{}: Trying to connect to peer {}", this_id, id);
                let mstream = NetListener::try_connect(conf);
                match mstream {
                    Some(mut stream) => {
                        if id_map.get(&id).is_none() {
                            debug!("{}: Initiated a connection with {} via {}", this_id, id, stream.socket_name());
                            id_map.insert(id, Some(stream));
                        }
                        else {
                            drop(stream);
                        }
                    }
                    None => {
                        debug!("{}, Couldn't get a connection to id: {}", this_id, id);
                    }
                }
            }
        }
    }

    fn lookup_peer_config<'a>(&'a mut self, id: uint) -> Option<&'a NetPeerConfig> {
        let mut peer_config: Option<&NetPeerConfig> = None;
        for conf in self.peer_configs.iter() {
            if conf.id == id {
                peer_config = Some(conf);
            }
        }
        peer_config
    }

    fn listen_peers(this_id: uint, addr: SocketAddr, peer_send: Sender<(uint, TcpStream)>, shutdown: Receiver<uint>) {
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
    }

    /*
     * Start the client listener.
     *
     * conf: the config for this server, containing *client* listen address.
     * from_clients_send: sends messages from peers back to the main loop.
     *
     */
    fn listen_clients(this_id: uint, addr: SocketAddr, from_client_send: Sender<(ClientCmdReq, Sender<ClientCmdRes>)>, shutdown_signal: Receiver<uint>) {
        // unwrapping because our server is dead in the water if it can't listen on its assigned port
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
    }

    fn try_connect(peer: &NetPeerConfig) -> Option<TcpStream> {
        match TcpStream::connect_timeout(peer.address, CONNECT_TIMEOUT) {
            Ok(mut stream) => {
                stream.write_uint(peer.id);
                debug!("Sent handshake req to {}", peer.id);
                Some(stream.clone())
            }
            err => {
                None
            }
        }
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
        let pc1 = NetPeerConfig {
            id: 1,
            address: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 9090,
            },
            client_addr: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 9091,
            },
        };
        let pc2 = NetPeerConfig {
            id: 2,
            address: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 9092,
            },
            client_addr: SocketAddr {
                ip: Ipv4Addr(127, 0, 0, 1),
                port: 9093,
            },
        };
        let mut pc_vec1 = ~Vec::new();
        let mut pc_vec2 = ~Vec::new();
        pc_vec1.push(pc2);
        pc_vec2.push(pc1);
        let (from_peers_send1, from_peers_recv1) = channel();
        let (from_client_send1, from_client_recv1) = channel();
        let (from_peers_send2, from_peers_recv2) = channel();
        let (from_client_send2, from_client_recv2) = channel();
        let mut nl1 = NetListener::new(pc1, pc_vec1, from_peers_send1, from_client_send1);
        let mut nl2 = NetListener::new(pc2, pc_vec2, from_peers_send2, from_client_send2);
        sleep(5000);
        nl1.shutdown();
        nl2.shutdown();
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
        NetListener::listen_peers(1, listen_addr, peer_connect_send, shutdown_recv);
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
