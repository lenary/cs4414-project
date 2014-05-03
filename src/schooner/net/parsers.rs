use std::io::{TcpStream, BufferedReader, IoResult, IoError, InvalidInput};
use std::slice::bytes;
use std::str::from_utf8;
use std::char::*;

static ID_TOKEN: &'static str = "Server-Id";
static LENGTH_TOKEN: &'static str = "Content-length";

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
    if !len_hdr.starts_with(LENGTH_TOKEN + ":") {
        return Err(IoError{kind: InvalidInput,
                           desc: "Length field not in message header.",
                           detail: Some(format!("length line parsed: {:s}", len_hdr))});
    }

    let parts: Vec<&str> = len_hdr.split(':').collect();
    if parts.len() != 2 {
        return Err(IoError{kind: InvalidInput,
                           desc: "More than one colon in length header.",
                           detail: Some(format!("length line parsed: {:s}", len_hdr))});
    }

    let lenstr = parts.get(1).trim();
    let result: IoResult<uint> = match from_str::<uint>(lenstr) {
        Some(len) => Ok(len),
        None => Err(IoError{kind: InvalidInput,
                           desc: "More than one colon in length header.",
                           detail: Some(format!("length line parsed: {:s}", len_hdr))}),
    };
    result
}

fn parse_server_id(id_hdr: &str) -> IoResult<uint> {
    if !id_hdr.starts_with(ID_TOKEN + ":") {
        return Err(IoError{kind: InvalidInput,
                           desc: "Id field not in message header.",
                           detail: Some(format!("length line parsed: {:s}", id_hdr))});
    }

    let parts: Vec<&str> = id_hdr.split(':').collect();
    if parts.len() != 2 {
        return Err(IoError{kind: InvalidInput,
                           desc: "More than one colon in id header.",
                           detail: Some(format!("length line parsed: {:s}", id_hdr))});
    }

    let idstr = parts.get(1).trim();
    let result: IoResult<uint> = match from_str::<uint>(idstr) {
        Some(id) => Ok(id),
        None => Err(IoError{kind: InvalidInput,
                            desc: "More than one colon in id header.",
                            detail: Some(format!("length line parsed: {:s}", id_hdr))}),
    };
    result
}

pub fn read_network_msg(mut reader: BufferedReader<TcpStream>) -> IoResult<~str> {
    let mut length: uint;
    let length_hdr = try!(reader.read_line());
    let length = try!(parse_content_length(length_hdr));
    let id_hdr = try!(reader.read_line());
    let id = try!(parse_server_id(id_hdr));
    let mut buf: Vec<u8> = Vec::from_elem(length, 0u8);
    let nread = try!(reader.read(buf.as_mut_slice()));
    if nread != length {
        return Err(IoError{kind: InvalidInput,
                           desc: "Network message read of specified bytes failed",
                           detail: Some(
                                   format!("Expected {} bytes, but read {} bytes",
                                   length, nread))});
    }
    match from_utf8(buf.as_slice()) {
        Some(s) => Ok(s.to_owned()),
        None => Err(IoError{kind: InvalidInput,
                            desc: "Conversion of Network message from bytes to str failed",
                            detail: None})
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
            debug!("Client sending msg: {}", msg);
            client_stream.write(msg.as_bytes());
        });
        for server_stream in acceptor.incoming() {
            assert!(read_network_msg(BufferedReader::new(server_stream.unwrap())).unwrap() == ~"Hello world");
            break;
        }
        drop(acceptor);
    }
}
