extern crate serialize;

use serialize::{json, Decodable};

#[deriving(Decodable, Encodable, Clone, Show)]
pub struct LogEntry {
    pub idx:  u64,  // raft idx in log
    pub term: u64,  // raft term in log
    pub data: ~str, // data or "command" to log
    pub uuid: ~str, // unique id from client, recommended (but not reqd) to be of UUID format
}

pub fn decode_log_entry(json_str: &str) -> Result<LogEntry, json::Error> {
    match json::from_str(json_str) {
        Ok(jobj) => {
            let mut decoder = json::Decoder::new(jobj);
            Decodable::decode(&mut decoder)
        },
        Err(e) => Err(e)
    }
}



#[cfg(test)]
mod test {
    use serialize::{json, Decodable};
    use uuid::Uuid;

    #[test]
    fn test_json_encode_of_LogEntry() {
        let uuidstr = Uuid::new_v4().to_hyphenated_str();
        let logentry = super::LogEntry{idx: 155, term: 2, data: ~"wc", uuid: uuidstr.clone()};

        let jstr = json::Encoder::str_encode(&logentry);
        assert!(jstr.len() > 0);
        assert!(jstr.contains("\"idx\":155"));
        assert!(jstr.contains("\"data\":\"wc\""));
        assert!(jstr.contains(format!("\"uuid\":\"{}\"", uuidstr)));
    }

    #[test]
    fn test_json_decode_of_LogEntry() {
        let jstr = ~r##"{"idx": 200, "term": 4, "data": "foo", "uuid": "4343"}"##;
        let jobj = json::from_str(jstr);
        assert!( jobj.is_ok() );

        let mut decoder = json::Decoder::new(jobj.unwrap());
        let logentry: super::LogEntry = Decodable::decode(&mut decoder).unwrap();

        assert_eq!(200, logentry.idx);
        assert_eq!(4, logentry.term);
        assert_eq!(~"foo", logentry.data);
        assert_eq!(~"4343", logentry.uuid);
    }

    #[test]
    fn test_decode_log_entry_fn_happy_path() {
        let jstr = ~r##"{"idx": 200, "term": 4, "data": "foo", "uuid": "deadbeef"}"##;
        let result = super::decode_log_entry(jstr);
        assert!(result.is_ok());

        let logentry = result.unwrap();
        assert_eq!(200, logentry.idx);
        assert_eq!(4, logentry.term);
        assert_eq!(~"foo", logentry.data);
        assert_eq!(~"deadbeef", logentry.uuid);
    }

    #[test]
    fn test_decode_log_entry_fn_error_path() {
        let jstr = ~"{abc}";
        let result = super::decode_log_entry(jstr);
        assert!(result.is_err());
    }
}
