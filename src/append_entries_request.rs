#[allow(deprecated_owned_vector)]
extern crate serialize;

use std::vec_ng::Vec;

use log_entry::LogEntry;

pub mod log_entry;

#[deriving(Decodable, Encodable)]
pub struct AppendEntriesRequest {
    term: u64,
    prev_log_idx: u64,
    prev_log_term: u64,
    commit_idx: u64,
    leader_name: ~str,  // TODO: change to Id of type uint?
    entries: Vec<LogEntry>,
}

// how goraft creates it
// return newAppendEntriesResponse(s.currentTerm, false, s.log.currentIndex(), s.log.CommitIndex()), false


pub struct AppendEntriesResponse {
    term: u64,
    curr_index: u64,
    // commit_idx: u64,    // TODO: do we need this?  Not in the cheat sheet of the raft.pdf
    success: bool,
}

// pub struct AppendEntriesResponseInfo {
// 	// pb     *protobuf.AppendEntriesResponse  // FIXME: don't know what this is for in goraft
// 	peer:  ~str,   // for now, do the 
// 	append: bool,  // from go-raft: what is it for?
//     response: AppendEntriesResponse,
// }

#[cfg(test)]
mod test {
    use serialize::{json, Decodable};
    use log_entry::LogEntry;

    #[test]
    fn test_json_encode_of_AppendEntriesRequest() {
        let logentry1 = LogEntry{index: 155, term: 2, command_name: ~"wc", command: None};
        let logentry2 = LogEntry{index: 156, term: 2, command_name: ~"ps -ef", command: None};
        let logentry3 = LogEntry{index: 157, term: 2, command_name: ~"nop", command: Some(~"nop")};
        let entries = vec!(logentry1, logentry2, logentry3);
        let apreq = super::AppendEntriesRequest{term: 66,
                                                prev_log_idx: 13,
                                                prev_log_term: 1,
                                                commit_idx: 88,
                                                leader_name: ~"kong",
                                                entries: entries};

        let jstr = json::Encoder::str_encode(&apreq);
        assert!(jstr.len() > 0);
        assert_eq!(~"{\"term\":66,\"prev_log_idx\":13,\"prev_log_term\":1,\"commit_idx\":88,\"leader_name\":\"kong\",\"entries\":[{\"index\":155,\"term\":2,\"command_name\":\"wc\",\"command\":null},{\"index\":156,\"term\":2,\"command_name\":\"ps -ef\",\"command\":null},{\"index\":157,\"term\":2,\"command_name\":\"nop\",\"command\":\"nop\"}]}", jstr);
    }
    
    #[test]
    fn test_json_decode_of_AppendEntriesRequest() {
        let jstr = ~r##"{"term": 14, "prev_log_idx": 130, "prev_log_term": 13,
                         "commit_idx": 77, "leader_name": "locutus",
                         "entries": [{"index": 200, "term": 4,"command_name": "foo", "command": null}]}"##;
        let jobj = json::from_str(jstr);
        assert!( jobj.is_ok() );

        let mut decoder = json::Decoder::new(jobj.unwrap());
        let appendreq: super::AppendEntriesRequest = Decodable::decode(&mut decoder);

        assert_eq!(14, appendreq.term);
        assert_eq!(130, appendreq.prev_log_idx);
        assert_eq!(13, appendreq.prev_log_term);
        assert_eq!(77, appendreq.commit_idx);
        assert_eq!(~"locutus", appendreq.leader_name);
        assert_eq!(1, appendreq.entries.len());

        let logentry = appendreq.entries.get(0);
        
        assert_eq!(200, logentry.index);
        assert_eq!(4, logentry.term);
        assert_eq!(~"foo", logentry.command_name);
        assert_eq!(None, logentry.command);        
    }
}
