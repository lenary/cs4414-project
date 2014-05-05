use std::io::{Acceptor, Listener, TcpListener, TcpStream, IoResult, BufferedReader};
use std::io::net::ip::SocketAddr;
use std::io::net::tcp::TcpAcceptor;
use super::macros::*;
use super::super::events::{ClientCmdReq, ClientCmdRes};

static CONNECT_TIMEOUT: u64 = 3000;

/*
 * Start the client listener.
 *
 * this_id: id of this server. used for debug messages etc.
 * addr: *client* listen address for this server
 * from_clients_send: sends messages from peers back to the main loop.
 */
pub fn listen_peers(this_id: uint, addr: SocketAddr) -> (Sender<uint>, Receiver<(uint, TcpStream)>) {
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
 * this_id: id of this server. used for debug messages etc.
 * addr: *client* listen address for this server
 * from_clients_send: sends messages from peers back to the main loop.
 *
 */
pub fn listen_clients(this_id: uint, addr: SocketAddr, from_client_send: Sender<(ClientCmdReq, Sender<ClientCmdRes>)>) -> Sender<uint> {
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

#[cfg(test)]
mod test{
    use std::vec::Vec;
    use std::io::net::ip::{SocketAddr, Ipv4Addr};
    use std::io::net::tcp::TcpStream;
    use std::io::timer::sleep;
    use std::io::BufferedWriter;
    use std::container::MutableSet;
    use super::super::types::*;
    use super::{listen_peers};

    
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
        let (shutdown_send, new_peer_recv) = listen_peers(1, listen_addr);
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
