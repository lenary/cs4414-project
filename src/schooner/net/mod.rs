use std::vec::Vec;
use std::io::IoError;
use serialize::Encodable;
use serialize::Decodable;
use std::io::{Acceptor, Listener, TcpListener, TcpStream};
use std::io::net::tcp::TcpAcceptor;
use serialize::json::{Encoder,Error};

pub use self::peer::{NetPeer, NetPeerConfig};
use super::events::{RaftMsg, VoteReq, VoteRes, AppendEntriesReq, AppendEntriesRes,
                    ClientCmdRes, ClientCmdReq};

pub mod peer;

// Private stuff, shouldn't be used elsewhere.
mod parsers;

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
    peers: Vec<NetPeer>
}

impl NetListener {
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
    fn spawn_client_listener(conf: NetPeerConfig, peer_configs: Vec<NetPeerConfig>, from_peers_send: Sender<RaftMsg>) {
        // unwrapping because our server is dead in the water if it can't listen on its assigned port
        let listener: TcpListener = TcpListener::bind(conf.address).unwrap();
        let mut acceptor: TcpAcceptor = listener.listen().unwrap();
        for stream in acceptor.incoming() {
            
        }
    }

    /*
     * Start the client listener.
     *
     * conf: the config for this server, containing *client* listen address.
     * from_clients_send: sends messages from peers back to the main loop.
     *
     */
    fn spawn_peer_listener(conf: NetPeerConfig, from_client_send: Sender<(ClientCmdReq, Sender<ClientCmdRes>)>) {
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
    fn get_peer_configs() -> Vec<NetPeerConfig> {
        Vec::new()
    }

    /*
     * Leader only
     */
    fn send_append_req(peer: NetPeerConfig, cmd: AppendEntriesReq) -> Option<Receiver<AppendEntriesRes>> {
        // if we were able to send the data over the TCP connection, then we will send it
        // back out on the returned Receiver.
        None
    }
    
    fn send_vote_req(peer: NetPeerConfig, cmd: VoteReq) -> Option<Receiver<VoteRes>> {
        None
    }

}

#[cfg(test)]
mod test {
}
