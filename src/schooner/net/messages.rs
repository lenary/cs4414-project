/*
pub fn serialize<'a, T: Encodable<Encoder<'a>, IoError>>(to_encode_object: &T) -> ~str {
    Encoder::str_encode(to_encode_object)
}
 */

/*
 * Contains all the information on what peers we have, and manages Schooner's interactions
 * with them.
 */
struct NetListener {
    peers: Vec<NetPeer>
}

impl NetListener {
    fn spawn_listener(from_peers_send: Sender<RaftCmd>) {

    }
}
