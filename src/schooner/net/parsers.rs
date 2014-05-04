use std::io::{TcpStream, BufferedReader, IoResult, IoError, InvalidInput};
use std::path::BytesContainer;
use std::slice::bytes;
use std::str::from_utf8;
use std::char::*;
use regex::*;
use serialize::{Decodable, json};
use super::types::*;

static ID_TOKEN: &'static str = "Server-Id";
static LENGTH_TOKEN: &'static str = "Content-Length";
static TYPE_TOKEN: &'static str = "Content-Type";

pub fn as_network_msg(rpc: RaftRpc) -> ~[u8] {
    let content_type = make_content_type(rpc.clone());
    let content = json::Encoder::str_encode(&rpc);
    let msg = content_type + "\n" + frame_msg(content);
    msg.as_bytes().to_owned()
}

/*
 * Frame a str wth the length header.
 */
pub fn frame_msg(msg: &str) -> ~str {
    (make_content_length(msg) + "\n" + msg)
}

/*
 * Get the length header for a string.
 */
fn make_content_length(s: &str) -> ~str {
    LENGTH_TOKEN + ": " + s.as_bytes().len().to_str()
}

/*
 * Used for first communication with a peer, when
 * they need to tell you their id.
 */
fn make_id_hdr(id: uint) -> ~str{
    ID_TOKEN + ": " + id.to_str()
}

/*
 * Get the type of a RaftRpc as a str.
 */
fn make_content_type(rpc: RaftRpc) -> ~str {
    let name = match rpc {
        RpcARQ(_)  => "RpcARQ",
        RpcARS(_)  => "RpcARS",
        RpcVRQ(_)  => "RpcVRQ",
        RpcVRS(_)  => "RpcVRS",
        RpcStopReq => "RpcStopReq",
    };
    "Content-Type" + ": " + name
}

fn to_result<T>(ok: Option<T>, err: &'static str) -> Result<T, IoError> {
    match ok {
        Some(res) => Ok(res),
        None => Err(IoError{
            kind: InvalidInput,
            desc: err,
            detail: None,
        }),
    }
}

fn parse_content_length(len_hdr: &str) -> IoResult<uint> {
    to_result(regex!(r"Content-Length: (\d+)")
        .captures(len_hdr)
        .map(|c| c.at(1))
        .and_then(|l| from_str(l)), "Failed parsing content length")
}


fn parse_content_type(text: &str) -> IoResult<~str> {
    to_result(regex!(r"Content-Type: (.+)")
        .captures(text)
        .map(|c| c.at(1).to_owned()), "Failed parsing content length")
}

fn parse_server_id(id_hdr: &str) -> IoResult<uint> {
    to_result(regex!(r"Server-Id: (\d+)")
        .captures(id_hdr)
        .map(|c| c.at(1))
        .and_then(|i| from_str(i)), "Failed parsing server ID")
}

pub fn read_rpc(mut stream: TcpStream) -> IoResult<RaftRpc> {
    let mut reader   = ~BufferedReader::new(stream);
    let content_type = try!(parse_content_type(try!(reader.read_line())));
    let content      = try!(read_network_msg(reader));
    let content_json = try!(json::from_str(content)
        .map_err(|e| IoError {
            kind: InvalidInput,
            desc: "Conversion to JSON error",
            detail: None}));
    let mut decoder = json::Decoder::new(content_json);
    let mut rpc: IoResult<RaftRpc> = Decodable::decode(&mut decoder)
        .map_err(|e| IoError {
            kind: InvalidInput,
            desc: "Json decode error",
            detail: None});
    rpc
}

pub fn read_network_msg(mut reader: ~BufferedReader<TcpStream>) -> IoResult<~str> {
    let length  = try!(parse_content_length(try!(reader.read_line())));
    let content = try!(reader.read_exact(length));
    to_result(content.container_as_str().map(|c| c.to_owned()),
        "Conversion of Network message from bytes to str failed")
}

#[cfg(test)]
mod test {
    use std::io::{TcpStream, BufferedReader, IoResult, IoError, InvalidInput};
    use std::io::net::ip::{SocketAddr, Ipv4Addr};
    use std::io::{Acceptor, Listener, TcpListener, TcpStream};
    use std::io::net::tcp::TcpAcceptor;

    use super::super::super::events::*;
    use super::super::types::*;
    use super::{parse_content_length, parse_server_id,
                read_network_msg, as_network_msg,
                LENGTH_TOKEN, make_id_hdr, frame_msg, read_rpc};

    /*
     * Can we parse content length fields?
     */
    #[test]
    fn test_content_length() {
        assert!(parse_content_length(LENGTH_TOKEN + ": 800").unwrap() == 800);
        assert!(parse_content_length(LENGTH_TOKEN + ": 32").unwrap() == 32);
        assert!(parse_content_length(LENGTH_TOKEN + ": 0").unwrap() == 0);
        assert!(parse_content_length("buckets: 0").is_err());
        assert!(parse_content_length("Length 0").is_err());
    }

    /*
     * Can we create and parse server ID fields?
     */
    #[test]
    fn test_parse_id() {
        assert!(parse_server_id(make_id_hdr(53)).unwrap() == 53);
        assert!(parse_server_id(make_id_hdr(0)).unwrap() == 0);
        assert!(parse_server_id("buckets: 0").is_err());
        assert!(parse_server_id("Length 0").is_err());
    }

    /*
     * Spawns a server, listens, sends one message and tries to parse it.
     */
    #[test]
    fn test_read_network_msg() {
        let listen_addr = SocketAddr {
            ip: Ipv4Addr(127, 0, 0, 1),
            port: 8844,
        };
        let listener: TcpListener = TcpListener::bind(listen_addr).unwrap();
        let mut acceptor: TcpAcceptor = listener.listen().unwrap();
        spawn(proc() {
            let mut client_stream = TcpStream::connect(listen_addr);
            let msg = frame_msg("Hello world");
            client_stream.write(msg.as_bytes());
        });
        for server_stream in acceptor.incoming() {
            let reader = ~BufferedReader::new(server_stream.unwrap());
            let result = read_network_msg(reader);
            assert!(result.is_ok());
            assert!(result.unwrap() == ~"Hello world");
            break;
        }
        drop(acceptor);
    }

    #[test]
    fn test_read_rpc_ars() {
        let listen_addr = SocketAddr {
            ip: Ipv4Addr(127, 0, 0, 1),
            port: 8844,
        };
        let listener: TcpListener = TcpListener::bind(listen_addr).unwrap();
        let mut acceptor: TcpAcceptor = listener.listen().unwrap();
        let msg = RpcARS(AppendEntriesRes {
            success: true,
            term: 12,
        });
        let (send, recv) = channel();
        spawn(proc() {
            let msg = recv.recv();
            let mut client_stream = TcpStream::connect(listen_addr);
            let msg_bytes = as_network_msg(msg);
            client_stream.write(msg_bytes);
        });
        send.send(msg.clone());
        for server_stream in acceptor.incoming() {
            let result = read_rpc(server_stream.unwrap());
            assert!(msg.clone() == result.unwrap())
            break;
        }
        drop(acceptor);
    }
}
