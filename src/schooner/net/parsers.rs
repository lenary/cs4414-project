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

/************************/
/* Reader-aware parsers */
/************************/

/*
 * Reads a "network" message framed with a content length as an RPC.
 */
pub fn read_rpc<R: Reader>(mut stream: R) -> IoResult<RaftRpc> {
    let mut reader   = ~BufferedReader::new(stream);
    let content      = try!(read_str(reader));
    let rpc = str_to_rpc(content);
    rpc
}

/*
 * Reads a "network" message framed with a content length as a string.
 */
pub fn read_str<T: Reader>(mut reader: ~BufferedReader<T>) -> IoResult<~str> {
    let length  = try!(parse_content_length(try!(reader.read_line())));
    let content = try!(reader.read_exact(length));
    to_result(content.container_as_str().map(|c| c.to_owned()),
        "Conversion of Network message from bytes to str failed")
}

/**************/
/* Formatters */
/**************/

/*
 * Convert an RPC to some bytes we can send over the network.
 */
pub fn as_network_msg(rpc: RaftRpc) -> ~[u8] {
    let content = json::Encoder::str_encode(&rpc);
    let msg = frame_msg(content);
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

/******************/
/* "Pure" parsers */
/******************/

fn parse_content_length(len_hdr: &str) -> IoResult<uint> {
    to_result(regex!(r"Content-Length: (\d+)")
        .captures(len_hdr)
        .map(|c| c.at(1))
        .and_then(|l| from_str(l)), "Failed parsing content length")
}


fn parse_server_id(id_hdr: &str) -> IoResult<uint> {
    to_result(regex!(r"Server-Id: (\d+)")
        .captures(id_hdr)
        .map(|c| c.at(1))
        .and_then(|i| from_str(i)), "Failed parsing server ID")
}

pub fn str_to_rpc(text: ~str) -> IoResult<RaftRpc> {
    let content_json = try!(json::from_str(text)
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

/********************/
/* Helper functions */
/********************/

/*
 * Convert an `Option` to a really basic `IoResult` with an error msg `err`
 * if it is `None`.
 */
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

#[cfg(test)]
mod test {
    use std::io::{TcpStream, BufferedReader, IoResult, IoError, InvalidInput};
    use std::io::net::ip::{SocketAddr, Ipv4Addr};
    use std::io::{Acceptor, Listener, TcpListener, TcpStream};
    use std::io::net::tcp::TcpAcceptor;
    use uuid::{Uuid, UuidVersion, Version4Random};

    use super::super::super::events::*;
    use super::super::types::*;
    use super::{parse_content_length, parse_server_id,
                read_str, as_network_msg,
                LENGTH_TOKEN, make_id_hdr, frame_msg, read_rpc};

    fn test_read_rpc(msg: RaftRpc) {
        let listen_addr = SocketAddr {
            ip: Ipv4Addr(127, 0, 0, 1),
            port: 8844,
        };
        let listener: TcpListener = TcpListener::bind(listen_addr).unwrap();
        let mut acceptor: TcpAcceptor = listener.listen().unwrap();
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

    /*
     * Can we parse content length fields?
     */
    #[test]
    fn test_content_length() {
        assert!(parse_content_length(LENGTH_TOKEN + ": 800").unwrap() == 800);
        assert!(parse_content_length(LENGTH_TOKEN + ": 32").unwrap() == 32);
        assert!(parse_content_length(LENGTH_TOKEN + ": 0").unwrap() == 0);
        assert!(parse_content_length(": 0").is_err());
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
    fn test_read_str() {
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
            let result = read_str(reader);
            assert!(result.is_ok());
            assert!(result.unwrap() == ~"Hello world");
            break;
        }
        drop(acceptor);
    }

    #[test]
    fn test_read_rpc_ars() {
        test_read_rpc(RpcARS(AppendEntriesRes {
            success: true,
            term: 12,
            uuid: Uuid::new(Version4Random).unwrap(),
        }));
    }

    #[test]
    fn test_read_rpc_arq() {
        test_read_rpc(RpcARQ(AppendEntriesReq {
            term: 3,
            prev_log_idx: 1,
            prev_log_term: 4,
            commit_idx: 1,
            leader_id: 5,
            entries: Vec::new(),
            uuid: Uuid::new(Version4Random).unwrap(),
        }));
    }

    #[test]
    fn test_read_rpc_vrq() {
        test_read_rpc(RpcVRQ(VoteReq {
            term: 3,
            candidate_id: 5,
            last_log_index: 1,
            last_log_term: 4,
            uuid: Uuid::new(Version4Random).unwrap(),
        }));
    }

    #[test]
    fn test_read_rpc_vrs() {
        test_read_rpc(RpcVRS(VoteRes {
            term: 3,
            vote_granted: true,
            uuid: Uuid::new(Version4Random).unwrap(),
        }));
    }
}
