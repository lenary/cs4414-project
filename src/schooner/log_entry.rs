#[allow(deprecated_owned_vector)]
extern crate serialize;

use serialize::{json, Decodable};

#[deriving(Decodable, Encodable, Clone)]
pub struct LogEntry {
    idx:  u64,
    term: u64,
    data: ~str,
}

pub fn decode_log_entry(json_str: &str) -> Result<LogEntry, json::Error> {
    match json::from_str(json_str) {
        Ok(jobj) => {
            let mut decoder = json::Decoder::new(jobj);
            let logentry: LogEntry = Decodable::decode(&mut decoder);
            Ok(logentry)
        },
        Err(e) => Err(e)
    }
}



#[cfg(test)]
mod test {
    use serialize::{json, Decodable};
    
    #[test]
    fn test_json_encode_of_LogEntry() {
        let logentry = super::LogEntry{idx: 155, term: 2, data: ~"wc"};

        let jstr = json::Encoder::str_encode(&logentry);
        assert!(jstr.len() > 0);
        assert!(jstr.contains("\"idx\":155"));
        assert!(jstr.contains("\"data\":\"wc\""));
    }
    
    #[test]
    fn test_json_decode_of_LogEntry() {
        let jstr = ~r##"{"idx": 200, "term": 4, "data": "foo"}"##;
        let jobj = json::from_str(jstr);
        assert!( jobj.is_ok() );

        let mut decoder = json::Decoder::new(jobj.unwrap());
        let logentry: super::LogEntry = Decodable::decode(&mut decoder);
        
        assert_eq!(200, logentry.idx);
        assert_eq!(4, logentry.term);
        assert_eq!(~"foo", logentry.data);
    }

    #[test]
    fn test_decode_log_entry_fn_happy_path() {
        let jstr = ~r##"{"idx": 200, "term": 4, "data": "foo"}"##;
        let result = super::decode_log_entry(jstr);
        assert!(result.is_ok());

        let logentry = result.unwrap();
        assert_eq!(200, logentry.idx);
        assert_eq!(4, logentry.term);
        assert_eq!(~"foo", logentry.data);
    }

    #[test]
    fn test_decode_log_entry_fn_error_path() {
        let jstr = ~"{abc}";
        let result = super::decode_log_entry(jstr);
        assert!(result.is_err());
    }    
}
