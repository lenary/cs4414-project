// TODO: this should probably be merged with
// schooner/events/append_entries.rs somehow
// Though lots of the serialisation/deserialisation should go into
// schooner/net/ somewhere

extern crate serialize;

use std::vec::Vec;
use serialize::{json, Decodable};

use super::consistent_log::LogEntry;

#[deriving(Decodable, Encodable, Clone, Show)]
pub struct AppendEntriesRequest {
    pub term: u64,          // current term of leader
    pub prev_log_idx: u64,  // idx of leader's log entry immediately before first entry in this AER
    pub prev_log_term: u64, // term of leader's log entry immediately before first entry in this AER
    pub commit_idx: u64,    // last idx of log committed to leader's state machine
    pub leader_id: uint,    // id for leader (based on config file)
    pub entries: Vec<LogEntry>, // entries to log; may be empty (hearbeat msg)
}

impl AppendEntriesRequest {
    pub fn encode(&self) -> ~str {
        json::Encoder::str_encode(&self)
    }
    
    pub fn decode(json_str: &str) -> Result<AppendEntriesRequest, json::Error> {
        match json::from_str(json_str) {
            Ok(jobj) => {
                let mut decoder = json::Decoder::new(jobj);
                Decodable::decode(&mut decoder)
            },
            Err(e) => Err(e)
        }
    }
}

#[deriving(Decodable, Encodable, Clone, Show)]
pub struct AppendEntriesResponse {
    // required by Raft protocol
    pub success: bool,   // whether follower has agreed to log entries in last AEReq
    pub term: u64,       // term of last log entry in follower's log

    // additional Schooner info
    pub idx: u64,        // idx of last log entry in follower's log after processing last AEReq
    pub commit_idx: u64, // idx of last log entry committed in follower's log after processing last AEReq
    pub peer_id: uint,   // id of the peer (follower) sending the response
}

impl AppendEntriesResponse {
    pub fn encode(&self) -> ~str {
        json::Encoder::str_encode(&self)
    }
    
    pub fn decode(json_str: &str) -> Result<AppendEntriesResponse, json::Error> {
        match json::from_str(json_str) {
            Ok(jobj) => {
                let mut decoder = json::Decoder::new(jobj);
                Decodable::decode(&mut decoder)
            },
            Err(e) => Err(e)
        }
    }
}

#[cfg(test)]
mod test {
    use serialize::{json, Decodable};
    use super::super::consistent_log::LogEntry;
    
    use super::{AppendEntriesRequest, AppendEntriesResponse};

    #[test]
    fn test_json_encode_of_AppendEntriesResponse() {
        let aeresp = super::AppendEntriesResponse{success: true,
                                                  term: 33,
                                                  idx: 22,
                                                  commit_idx: 5,
                                                  peer_id: 2};

        let jstr = json::Encoder::str_encode(&aeresp);
        assert!(jstr.len() > 0);
        assert!(jstr.contains("\"success\":true"));
        assert_eq!(~"{\"success\":true,\"term\":33,\"idx\":22,\"commit_idx\":5,\"peer_id\":2}", jstr);
    }

    #[test]
    fn test_json_decode_of_AppendEntriesResponse() {
        let jstr = ~"{\"success\":false,\"term\":777,\"idx\":1,\"commit_idx\":1,\"peer_id\":2}";

        let jobj = json::from_str(jstr);
        assert!( jobj.is_ok() );

        let mut decoder = json::Decoder::new(jobj.unwrap());
        let aeresp: super::AppendEntriesResponse = Decodable::decode(&mut decoder).unwrap();

        assert_eq!(false, aeresp.success);
        assert_eq!(777, aeresp.term);
        assert_eq!(1, aeresp.idx);
        assert_eq!(1, aeresp.commit_idx);
        assert_eq!(2, aeresp.peer_id);
    }

    #[test]
    fn test_json_decode_of_AppendEntriesResponse_via_api() {
        let jstr = ~"{\"success\":true,\"term\":777,\"idx\":99,\"commit_idx\":33,\"peer_id\":2}";

        let result = AppendEntriesResponse::decode(jstr);
        assert!(result.is_ok());

        let aeresp = result.unwrap();

        assert_eq!(true, aeresp.success);
        assert_eq!(777, aeresp.term);
        assert_eq!(99, aeresp.idx);
        assert_eq!(33, aeresp.commit_idx);
        assert_eq!(2, aeresp.peer_id);
    }


    #[test]
    fn test_json_encode_of_AppendEntriesRequest() {
        let logentry1 = LogEntry{idx: 155, term: 2, data: ~"wc", uuid: ~"uuid111"};
        let logentry2 = LogEntry{idx: 156, term: 2, data: ~"ps -ef", uuid: ~"uuid222"};
        let logentry3 = LogEntry{idx: 157, term: 2, data: ~"", uuid: ~""};
        let entries = vec!(logentry1, logentry2, logentry3);
        let aereq = super::AppendEntriesRequest{term: 66,
                                                prev_log_idx: 13,
                                                prev_log_term: 1,
                                                commit_idx: 88,
                                                leader_id: 32,
                                                entries: entries};

        let jstr = json::Encoder::str_encode(&aereq);
        assert!(jstr.len() > 0);
        assert_eq!(~"{\"term\":66,\"prev_log_idx\":13,\"prev_log_term\":1,\"commit_idx\":88,\"leader_id\":32,\"entries\":[{\"idx\":155,\"term\":2,\"data\":\"wc\",\"uuid\":\"uuid111\"},{\"idx\":156,\"term\":2,\"data\":\"ps -ef\",\"uuid\":\"uuid222\"},{\"idx\":157,\"term\":2,\"data\":\"\",\"uuid\":\"\"}]}", jstr);
    }

    #[test]
    fn test_json_decode_of_AppendEntriesRequest() {
        let jstr = ~r##"{"term": 14, "prev_log_idx": 130, "prev_log_term": 13,
                         "commit_idx": 77, "leader_id": 19,
                         "entries": [{"idx": 200, "term": 4, "data": "foo", "uuid": "uuid444"}]}"##;
        let jobj = json::from_str(jstr);
        assert!( jobj.is_ok() );

        let mut decoder = json::Decoder::new(jobj.unwrap());
        let appendreq: super::AppendEntriesRequest = Decodable::decode(&mut decoder).unwrap();

        assert_eq!(14, appendreq.term);
        assert_eq!(130, appendreq.prev_log_idx);
        assert_eq!(13, appendreq.prev_log_term);
        assert_eq!(77, appendreq.commit_idx);
        assert_eq!(19, appendreq.leader_id);
        assert_eq!(1, appendreq.entries.len());

        let logentry = appendreq.entries.get(0);

        assert_eq!(200, logentry.idx);
        assert_eq!(4, logentry.term);
        assert_eq!(~"foo", logentry.data);
        assert_eq!(~"uuid444", logentry.uuid);
    }
}
