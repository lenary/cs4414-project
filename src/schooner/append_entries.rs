#[allow(deprecated_owned_vector)]
extern crate serialize;

use std::vec::Vec;
use serialize::{json, Decodable};

use schooner::log_entry::LogEntry;

#[deriving(Decodable, Encodable, Clone, Eq, Show)]
pub enum Cmd {
    APND,    // append: has logentries with data
    PING,    // leader still alive, no entries to log
    STOP,    // shutdown, no entries to log
}

#[deriving(Decodable, Encodable, Clone)]
pub struct AppendEntriesRequest {
    cmd: Cmd,
    term: u64,
    prev_log_idx: u64,
    prev_log_term: u64,
    commit_idx: u64,
    leader_id: ~str,  // TODO: change to Id of type uint?
    entries: Vec<LogEntry>,
}


// how goraft creates it
// return newAppendEntriesResponse(s.currentTerm, false, s.log.currentIndex(), s.log.CommitIndex()), false

#[deriving(Decodable, Encodable, Clone)]
pub struct AppendEntriesResponse {
    term: u64,
    curr_idx: u64,
    // commit_idx: u64,    // TODO: do we need this?  Not in the cheat sheet of the raft.pdf
    success: bool,
}

pub fn decode_append_entries_request(json_str: &str) -> Result<AppendEntriesRequest, json::Error> {
    match json::from_str(json_str) {
        Ok(jobj) => {
            let mut decoder = json::Decoder::new(jobj);
            let aereq: AppendEntriesRequest = Decodable::decode(&mut decoder);
            Ok(aereq)
        },
        Err(e) => Err(e)
    }
}

pub fn decode_append_entries_response(json_str: &str) -> Result<AppendEntriesResponse, json::Error> {
    match json::from_str(json_str) {
        Ok(jobj) => {
            let mut decoder = json::Decoder::new(jobj);
            let aeresp: AppendEntriesResponse = Decodable::decode(&mut decoder);
            Ok(aeresp)
        },
        Err(e) => Err(e)
    }
}


#[cfg(test)]
mod test {
    use serialize::{json, Decodable};
    use schooner::log_entry::LogEntry;
    use schooner::append_entries::APND;

    #[test]
    fn test_json_encode_of_AppendEntriesResponse() {
        let aeresp = super::AppendEntriesResponse{term: 33,
                                                  curr_idx: 22,
                                                  success: true};

        let jstr = json::Encoder::str_encode(&aeresp);
        assert!(jstr.len() > 0);
        assert!(jstr.contains("33"));
        assert!(jstr.contains("\"success\":true"));
        assert_eq!(~"{\"term\":33,\"curr_idx\":22,\"success\":true}", jstr);
    }

    #[test]
    fn test_json_decode_of_AppendEntriesResponse() {
        let jstr = ~"{\"term\":777,\"curr_idx\":0,\"success\":false}";

        let jobj = json::from_str(jstr);
        assert!( jobj.is_ok() );

        let mut decoder = json::Decoder::new(jobj.unwrap());
        let aeresp: super::AppendEntriesResponse = Decodable::decode(&mut decoder);

        assert_eq!(777, aeresp.term);
        assert_eq!(0, aeresp.curr_idx);
        assert_eq!(false, aeresp.success);
    }

    #[test]
    fn test_json_decode_of_AppendEntriesResponse_via_api() {
        let jstr = ~"{\"term\":777,\"curr_idx\":99,\"success\":true}";

        let result = super::decode_append_entries_response(jstr);
        assert!(result.is_ok());

        let aeresp = result.unwrap();

        assert_eq!(777, aeresp.term);
        assert_eq!(99, aeresp.curr_idx);
        assert_eq!(true, aeresp.success);
    }


    #[test]
    fn test_json_encode_of_AppendEntriesRequest() {
        let logentry1 = LogEntry{idx: 155, term: 2, data: ~"wc"};
        let logentry2 = LogEntry{idx: 156, term: 2, data: ~"ps -ef"};
        let logentry3 = LogEntry{idx: 157, term: 2, data: ~""};
        let entries = vec!(logentry1, logentry2, logentry3);
        let aereq = super::AppendEntriesRequest{cmd: APND, term: 66, 
                                                prev_log_idx: 13,
                                                prev_log_term: 1,
                                                commit_idx: 88,
                                                leader_id: ~"kong",
                                                entries: entries};

        let jstr = json::Encoder::str_encode(&aereq);
        assert!(jstr.len() > 0);
        assert_eq!(~"{\"cmd\":\"APND\",\"term\":66,\"prev_log_idx\":13,\"prev_log_term\":1,\"commit_idx\":88,\"leader_id\":\"kong\",\"entries\":[{\"idx\":155,\"term\":2,\"data\":\"wc\"},{\"idx\":156,\"term\":2,\"data\":\"ps -ef\"},{\"idx\":157,\"term\":2,\"data\":\"\"}]}", jstr);
    }

    #[test]
    fn test_json_decode_of_AppendEntriesRequest() {
        let jstr = ~r##"{"cmd":"APND", "term": 14, "prev_log_idx": 130, "prev_log_term": 13,
                         "commit_idx": 77, "leader_id": "locutus",
                         "entries": [{"idx": 200, "term": 4, "data": "foo"}]}"##;
        let jobj = json::from_str(jstr);
        assert!( jobj.is_ok() );

        let mut decoder = json::Decoder::new(jobj.unwrap());
        let appendreq: super::AppendEntriesRequest = Decodable::decode(&mut decoder);

        assert_eq!(APND, appendreq.cmd);
        assert_eq!(14, appendreq.term);
        assert_eq!(130, appendreq.prev_log_idx);
        assert_eq!(13, appendreq.prev_log_term);
        assert_eq!(77, appendreq.commit_idx);
        assert_eq!(~"locutus", appendreq.leader_id);
        assert_eq!(1, appendreq.entries.len());

        let logentry = appendreq.entries.get(0);

        assert_eq!(200, logentry.idx);
        assert_eq!(4, logentry.term);
        assert_eq!(~"foo", logentry.data);
    }
}
