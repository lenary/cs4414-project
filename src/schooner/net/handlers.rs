use serialize::json;
use std::io::net::tcp::TcpStream;
use std::io::IoResult;
use std::io::net::ip::SocketAddr;
use super::peer::Peer;
use super::super::events::append_entries::{AppendEntriesReq,AppendEntriesRes};

pub fn leader_peer_handler(peer: Peer, chsend: Sender<IoResult<AppendEntriesRes>>, aereq: AppendEntriesReq) {
    info!("LEADER PEER_HANDLER for peer {:?}", peer.id);
    let ipaddr = format!("{}:{:u}", &peer.ip, peer.tcpport);
    let addr = from_str::<SocketAddr>(ipaddr).unwrap();
    info!("PHDLR: DEBUG x887: connecting to: {}", ipaddr.clone());

    let aereqstr = json::Encoder::str_encode(&aereq);
    let mut stream = TcpStream::connect(addr.clone());
    // have to add the Length to the network message
    let result = stream.write_str(format!("Length: {:u}\n{:s}", aereqstr.len(), aereqstr));

    debug!("PHDLR DEBUG 888 for peer: {}", peer.id);
    if result.is_err() {
        info!("PHDLR for peer {} ==> WARN: Unable to send message to peer {}", peer.id, peer);
        return;
    }
    let _ = stream.flush();
    debug!("PHDLR DEBUG 890 for peer: {}", peer.id);

    // fn read(&mut self, buf: &mut [u8]) -> IoResult<uint>
    // let mut buf: Vec<u8> = Vec::from_elem(1, 0u8);
    // let response = stream.read(buf.as_mut_slice());
    // TODO: currently responses do not have a length in the message => probably should?
    // FIXME: this is a blocking call => how avoid an eternal wait?
    let response = stream.read_to_str(); // this type of read is only safe if the other end closes the stream (EOF)
    info!(">>===> PEER HDLR {} ==> PEER RESPONSE: {}", peer.id, response);

    match response {
        Ok(aeresp_str) => {
            let aeresp = AppendEntriesRes::decode(aeresp_str).
                ok().expect(format!("leader_peer_handler: decode aeresp failed for: {:?}", aeresp_str));
            chsend.send(Ok(aeresp));
        },
        Err(e) => chsend.send(Err(e))
    }

    drop(stream);  // TODO: probably unneccesary since goes out of scope
    info!("PEER HANDLER FOR {} SHUTTING DOWN", peer.id);
}
