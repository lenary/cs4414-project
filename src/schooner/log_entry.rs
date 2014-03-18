#[allow(deprecated_owned_vector)]
extern crate serialize;

use serialize::{json, Decodable};

#[deriving(Decodable, Encodable, Clone)]
pub struct LogEntry {
    index: u64,
    term: u64,
    command_name: ~str,
    command: Option<~str>,  // optional, for nop-command
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
        let logentry = super::LogEntry{index: 155, term: 2, command_name: ~"wc", command: None};

        let jstr = json::Encoder::str_encode(&logentry);
        assert!(jstr.len() > 0);
        assert!(jstr.contains("\"index\":155"));
        assert!(jstr.contains("\"command_name\":\"wc\""));
        assert!(jstr.contains("\"command\":null"));
    }
    
    #[test]
    fn test_json_decode_of_LogEntry() {
        let jstr = ~r##"{"index": 200, "term": 4,
                         "command_name": "foo", "command": null}"##;
        let jobj = json::from_str(jstr);
        assert!( jobj.is_ok() );

        let mut decoder = json::Decoder::new(jobj.unwrap());
        let logentry: super::LogEntry = Decodable::decode(&mut decoder);
        
        assert_eq!(200, logentry.index);
        assert_eq!(4, logentry.term);
        assert_eq!(~"foo", logentry.command_name);
        assert_eq!(None, logentry.command);
    }

    #[test]
    fn test_decode_log_entry_fn_happy_path() {
        let jstr = ~r##"{"index": 200, "term": 4,
                         "command_name": "foo", "command": null}"##;
        let result = super::decode_log_entry(jstr);
        assert!(result.is_ok());

        let logentry = result.unwrap();
        assert_eq!(200, logentry.index);
        assert_eq!(4, logentry.term);
        assert_eq!(~"foo", logentry.command_name);
        assert_eq!(None, logentry.command);
    }

    #[test]
    fn test_decode_log_entry_fn_error_path() {
        let jstr = ~"{abc}";
        let result = super::decode_log_entry(jstr);
        assert!(result.is_err());
    }    
}
