use std::io::{TcpStream, BufferedReader, IoResult, IoError, InvalidInput};
use regex::*;
use std::path::BytesContainer;
use std::slice::bytes;
use std::str::from_utf8;
use std::char::*;

static ID_TOKEN: &'static str = "Server-Id";
static LENGTH_TOKEN: &'static str = "Content-Length";

/*
 * Wrap a string in the right headers for it to be sent over the network.
 */
pub fn frame_msg(s: &str, server_id: uint) -> ~str {
    (make_content_length(s) + "\n" +
     make_id_hdr(server_id) + "\n" +
     s)
}

fn make_content_length(s: &str) -> ~str {
    LENGTH_TOKEN + ": " + s.as_bytes().len().to_str()
}

fn make_id_hdr(id: uint) -> ~str{
    ID_TOKEN + ": " + id.to_str()
}

fn parse_content_length(len_hdr: &str) -> IoResult<uint> {
    let re = regex!(r"Content-Length: (?P<length>\d+)");
    let strlen = re.captures(len_hdr).and_then(|c| Some(c.name("length")));
    let len: Option<uint> = strlen.and_then(|l| from_str(l));
    match len {
        Some(len) => Ok(len),
        None => Err(IoError{
            kind: InvalidInput,
            desc: "Failed parsing content length",
            detail: None,
        }),
    }
}

fn parse_server_id(id_hdr: &str) -> IoResult<uint> {
    let re = regex!(r"Server-Id: (?P<id>\d+)");
    let strid = re.captures(id_hdr).and_then(|c| Some(c.name("id")));
    let id: Option<uint> = strid.and_then(|i| from_str(i));
    match id {
        Some(uid) => Ok(uid),
        None => Err(IoError {
            kind: InvalidInput,
            desc: "Failed parsing server ID",
            detail: None,
        }),
    }
}

pub fn read_network_msg(mut reader: BufferedReader<TcpStream>) -> IoResult<~str> {
    let mut length: uint;
    let length = try!(reader.read_line().and_then(|l| parse_content_length(l)));
    let id_hdr = reader.read_line().map_err(|e| "Reading failed");
    let id = id_hdr.map(|i| parse_server_id(i));
    let mut buf: Vec<u8> = Vec::from_elem(length, 0u8);
    let content = try!(reader.read_exact(length));
    match content.container_as_str() {
        Some(s) => Ok(s.to_owned()),
        None => Err(IoError{
            kind: InvalidInput,
            desc: "Conversion of Network message from bytes to str failed",
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
    
    use super::{parse_content_length, parse_server_id, read_network_msg,
                LENGTH_TOKEN, make_id_hdr, frame_msg};

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
            let msg = frame_msg("Hello world", 14);
            client_stream.write(msg.as_bytes());
        });
        for server_stream in acceptor.incoming() {
            let reader = BufferedReader::new(server_stream.unwrap());
            let result = read_network_msg(reader);
            assert!(result.is_ok());
            assert!(result.unwrap() == ~"Hello world");
            break;
        }
        drop(acceptor);
    }
}
