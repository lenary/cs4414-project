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

fn parse_server_id(id_hdr: &str) -> IoResult<uint> {
    to_result(regex!(r"Server-Id: (\d+)")
        .captures(id_hdr)
        .map(|c| c.at(1))
        .and_then(|i| from_str(i)), "Failed parsing server ID")
}

pub fn read_network_msg(mut reader: BufferedReader<TcpStream>) -> IoResult<~str> {
    let length  = try!(parse_content_length(try!(reader.read_line())));
    let id      = try!(parse_server_id(     try!(reader.read_line())));
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
