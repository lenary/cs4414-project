use std::io::{BufferedReader, File, InvalidInput, IoError, IoResult};
use std::vec::Vec;

#[deriving(Clone)]
pub struct Peer {
    pub id: uint,
    pub ip: ~str,
    pub tcpport: uint,

    // only used by leader
    pub next_idx: u64,
    pub match_idx: u64,
}

///
/// Parses the Java-properties style config file and create Peer structs
/// from each valid line.  Returns IoError if any parsing errors occur.
/// Returns Vec<Peer> based on the config file if no parsing errors occur.
///
/// Config file should be of format:
/// peer.1.addr = 127.0.0.1:23158
/// peer.2.addr = 127.0.0.1:23159
/// peer.3.addr = 127.0.0.1:23160
///
pub fn parse_config(cfgpath: Path) -> IoResult<Vec<Peer>> {
    let file = try!(File::open(&cfgpath));
    let mut br = BufferedReader::new(file);

    let mut peers: Vec<Peer> = Vec::with_capacity(5);
    for ln in br.lines() {
        match ln {
            Ok(ln) => {
                if ! ln.trim().starts_with("#") {
                    let v: ~[&str] = ln.split('=').collect();
                    if v.len() != 2 {
                        return Err(IoError{kind: InvalidInput,
                                           desc: "badly formed property",
                                           detail: Some(format!("bad line: {}", ln))});
                    }
                    peers.push( try!(create_peer(v)) );
                }
            },
            Err(e) => {
                return Err(e);
            }
        }
    }

    Ok(peers)
}


fn create_peer(cfg_toks: ~[&str]) -> IoResult<Peer> {
    // parse key
    let key = cfg_toks[0].trim();
    if ! key.starts_with("peer") || ! key.ends_with("addr") {
        return Err(bad_key(format!("bad key: {}", key)));
    }

    let key_toks: ~[&str] = key.split('.').collect();
    if key_toks.len() != 3 {
        return Err(bad_key(format!("bad key: {}", key)));
    }

    let id = from_str::<uint>(key_toks[1]);
    if id.is_none() {
        return Err(bad_key(format!("cannot parse id from second field; bad key: {}", key)));
    }
    let id = id.unwrap();  // id of Peer


    // parse value
    let val = cfg_toks[1].trim();
    let val_toks: ~[&str] = val.split(':').collect();
    if val_toks.len() != 2 || val_toks[0].len() == 0 || val_toks[1].len() == 0 {
        return Err(bad_val(format!("bad val: {} for key {}", val, key)));
    }
    let ipaddr = val_toks[0];
    let port = from_str::<uint>(val_toks[1]);
    if port.is_none() {
        return Err(bad_val(format!("bad val: {} port is not an integer. (For key {})", val, key)));
    }
    let port = port.unwrap();

    // create peer
    Ok(Peer {
        id: id,
        ip: ipaddr.to_owned(),  // BOGUS
        tcpport: port,
        next_idx: 0,
        match_idx: 0,
    })
}


fn bad_key(msg: ~str) -> IoError {
    IoError{kind: InvalidInput,
            desc: "badly formed config key",
            detail: Some(msg)}
}

fn bad_val(msg: ~str) -> IoError {
    IoError{kind: InvalidInput,
            desc: "badly formed config value",
            detail: Some(msg)}
}



#[cfg(test)]
mod test {
    use super::Peer;
    use std::io::{fs,File,Open,Write};
    use uuid::Uuid;

    fn write_config(n: uint) -> Path {
        let pathstr = Uuid::new_v4().to_simple_str();
        let path = Path::new(pathstr + ".config");
        let mut file = File::open_mode(&path, Open, Write).unwrap();

        let mut port = 23158;
        let mut id = 1;
        for _ in range(0, n) {
            let entry = format!("peer.{}.addr = 127.0.0.1:{}", id, port);
            id += 1;
            port += 1;
            let result = file.write_line(entry);
            if result.is_err() {
                fail!("Unable to write line to test config file in peer.rs-write_config: {}", result.unwrap_err());
            }
        }
        path
    }

    // this one is bad because the second entry has no port
    fn write_bad_config() -> Path {
        let pathstr = Uuid::new_v4().to_simple_str();
        let path = Path::new(pathstr + ".config");
        let mut file = File::open_mode(&path, Open, Write).unwrap();

        let mut result = file.write_line("peer.1.addr = 127.0.0.1:2133");
        if result.is_err() {
            fail!("Unable to write line to test config fiel in peer.rs: {}", result.unwrap_err());
        }
        result = file.write_line("peer.2.addr = 127.0.0.1");
        if result.is_err() {
            fail!("Unable to write line to test config fiel in peer.rs: {}", result.unwrap_err());
        }
        path
    }

    fn tear_down(path: Path) {
        let result = fs::unlink(&path);
        if result.is_err() {
            fail!("Unable to unlink test config file: {}", result.unwrap_err());
        }
    }


    #[test]
    pub fn test_valid_config_file_with_corrupt_entry() {
        let path = write_bad_config();

        let result = super::parse_config(path.clone());
        assert!(result.is_err());
        tear_down(path);
    }


    #[test]
    pub fn test_valid_config_file_with_one_peer_entry() {
        let path = write_config(1);

        let result = super::parse_config(path.clone());
        assert!(result.is_ok());

        let peers: Vec<Peer> = result.unwrap();
        assert_eq!(1, peers.len());

        let peer1 = peers.get(0);
        assert_eq!(1, peer1.id);
        assert_eq!(~"127.0.0.1", peer1.ip);
        assert_eq!(23158, peer1.tcpport);
        assert_eq!(0, peer1.next_idx);

        tear_down(path);
    }

    #[test]
    pub fn test_valid_config_file_with_five_peer_entries() {
        let path = write_config(5);

        let result = super::parse_config(path.clone());
        assert!(result.is_ok());

        let peers: Vec<Peer> = result.unwrap();
        assert_eq!(5, peers.len());

        let peer1 = peers.get(0);
        assert_eq!(1, peer1.id);
        assert_eq!(~"127.0.0.1", peer1.ip);
        assert_eq!(23158, peer1.tcpport);
        assert_eq!(0, peer1.next_idx);

        let peer2 = peers.get(1);
        assert_eq!(2, peer2.id);
        assert_eq!(~"127.0.0.1", peer2.ip);
        assert_eq!(23159, peer2.tcpport);
        assert_eq!(0, peer2.next_idx);

        let peer3 = peers.get(2);
        assert_eq!(3, peer3.id);
        assert_eq!(~"127.0.0.1", peer3.ip);
        assert_eq!(23160, peer3.tcpport);
        assert_eq!(0, peer3.next_idx);

        let peer4 = peers.get(3);
        assert_eq!(4, peer4.id);
        assert_eq!(~"127.0.0.1", peer4.ip);
        assert_eq!(23161, peer4.tcpport);
        assert_eq!(0, peer4.next_idx);

        let peer5 = peers.get(4);
        assert_eq!(5, peer5.id);
        assert_eq!(~"127.0.0.1", peer5.ip);
        assert_eq!(23162, peer5.tcpport);
        assert_eq!(0, peer5.next_idx);

        tear_down(path);
    }
}
