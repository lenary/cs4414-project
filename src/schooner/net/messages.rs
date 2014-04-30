use std::str;
use std::io::net::ip::SocketAddr;
use std::io::net::tcp::{TcpListener,TcpStream};
use std::io::Acceptor;
use std::io::IoError;
use std::io::{BufferedReader,InvalidInput,IoError,IoResult,Listener};
use serialize::{Encodable,Decodable};
use serialize::json;
use serialize::json::{Encoder,Decoder,Error};
use super::super::Event;
use super::super::events::append_entries::{AppendEntriesReq, AppendEntriesRes};

static STOP_MSG: &'static str = "STOP";

pub fn serialize<'a, T: Encodable<Encoder<'a>, IoError>>(to_encode_object: &T) -> ~str {
    Encoder::str_encode(to_encode_object)
}

pub fn deserialize(s: &str) -> Result<AppendEntriesRes, json::Error> {
    match json::from_str(s) {
        Ok(jobj) => {
            let mut decoder = Decoder::new(jobj);
            Decodable::decode(&mut decoder)
        },
        Err(e) => Err(e)
    }
}

/// The network listener sets up a socket listener loop to accept incoming TCP connections.
/// When a network msg comes in, an Event is created with the string contents of the "message"
/// and a Sender channel is put on the Event (why??) and the event is sent.
/// The serve_loop will read from that channel and process the Event.
/// Events can be any incoming information, such as STOP messages, AEReqs, AEResponses
/// or client commands
/// Param:
///  - conx_str: info to create SocketAddr for listening on
///  - chan: Event channel in the Server struct.
///
pub fn network_listener(conx_str: ~str, chan: Sender<~Event>, svr_id: uint) {
    let addr = from_str::<SocketAddr>(conx_str).expect("Address error.");
    let mut acceptor = TcpListener::bind(addr).unwrap().listen().unwrap();
    info!("server <{}> listening on {:}", svr_id, addr);

    // TODO: document what this channel is for
    let (chsend, chrecv): (Sender<~str>, Receiver<~str>) = channel();
    let mut stop_signalled = false;

    debug!("NL: DEBUG 00: svr: {}", svr_id);
    for stream in acceptor.incoming() {
        debug!("NL: DEBUG 0: svr: {}", svr_id);

        let mut stream = stream.unwrap();

        // TODO: only handling one request at a time for now => spawn threads later?
        match read_network_msg(stream.clone(), svr_id) {
            Ok(input)  => {
                let ev = ~Event{msg: input.clone(), ch: chsend.clone()};
                chan.send(ev);

                if is_stop_msg(input) {
                    stop_signalled = true;  // TODO: do I need to set this bool var or can I just break out here?
                    info!("NL: INFO 1: stop msg received at svr: {}", svr_id);

                } else {
                    info!("NL: sent Event to event-loop; now waiting on response for svr: {}", svr_id);

                    // Once the Event is sent to serve-loop task it awaits a response (string)
                    // and the response will be send back to the network caller.
                    // Since the response is just a string, all logic of what is in the request
                    // & response is handled by the serve-loop
                    let resp = chrecv.recv();
                    info!("NL: sending response: {:?}", resp);
                    let result = stream.write_str(resp);
                    if result.is_err() {
                        error!("ERROR: Unable to respond to sender over network: {:?} for svr: {}", result.err(), svr_id);
                    }
                    let _ = stream.flush();
                }
                debug!("NL: DEBUG 2 for svr {}", svr_id);
            },
            Err(ioerr) => error!("ERROR: {:?}", ioerr)
        }
        if stop_signalled {
            debug!("NL: DEBUG 4 for svr {}", svr_id);
            break;
        }
    }

    debug!("network listener shutting down ... for svr {}", svr_id);
}

///
/// TODO: can this fn deal with HTTP style requests? If not, what should the client request look like for this to work?
///
pub fn read_network_msg(stream: TcpStream, svr_id: uint) -> IoResult<~str> {
    let mut reader = BufferedReader::new(stream);

    let length_hdr = try!(reader.read_line());
    let result = parse_content_length(length_hdr);
    if result.is_none() {
        return Err(IoError{kind: InvalidInput,
                           desc: "Length not parsable in network message",
                           detail: Some(format!("length line parsed: {:s}", length_hdr))});
    }
    let length = result.unwrap();
    debug!("** read_network_msg: length of msg {:u} at svr {}", length, svr_id);

    let mut buf: Vec<u8> = Vec::from_elem(length, 0u8);
    let nread = try!(reader.read(buf.as_mut_slice()));

    if nread != length {
        return Err(IoError{kind: InvalidInput,
                           desc: "Network message read of specified bytes failed",
                           detail: Some(format!("Expected {} bytes, but read {} bytes", length, nread))});
    }

    match str::from_utf8(buf.as_slice()) {
        Some(s) => Ok(s.to_owned()),
        None    => Err(IoError{kind: InvalidInput,
                               desc: "Conversion of Network message from bytes to str failed",
                               detail: None})
    }
}

///
/// Expects content-length string of format
///   `Length: NN`
/// where NN is an integer >= 0.
/// Returns the length as a uint or None if the string is not
/// of the specified format.
///
pub fn parse_content_length(len_line: &str) -> Option<uint> {
    if ! len_line.starts_with("Length:") {
        return None;
    }

    let parts: Vec<&str> = len_line.split(':').collect();
    if parts.len() != 2 {
        return None;
    }

    let lenstr = parts.get(1).trim();
    from_str::<uint>(lenstr)
}

pub fn is_stop_msg(s: &str) -> bool {
    s == STOP_MSG
}
