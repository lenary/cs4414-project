extern crate rand;
extern crate serialize;

use std::io::{BufferedReader,BufferedWriter,File,IoResult,IoError,InvalidInput,EndOfFile,Append,Open,Read,Write};
use std::io::fs;
use std::vec::Vec;
use uuid::{Uuid, UuidVersion, Version4Random};
use serialize::{json, Decodable};

use super::events::append_entries::AppendEntriesReq;


///
/// Log maintains the current state of the log and has functionality associated with
/// manipulating both the in-memory and on-disk version of the log.
///
/// The first valid term and idx is 1.
/// A term or idx of 0 means "UKNOWN".
///
pub struct Log {
    pub file: File,      // open File with Append/Write state
    pub path: Path,      // path to log
    pub idx: u64,        // idx  of last entry in the logs, may be ahead of from commit_idx
    pub term: u64,       // term of last entry in the logs
    pub logentries: Vec<LogEntry>,  // cache of all entries logged to file (TODO: future keep only last 100? 1000?)
}

#[deriving(Decodable, Encodable, Clone, Show, Eq)]
pub struct LogEntry {
    pub idx:  u64,  // raft idx in log
    pub term: u64,  // raft term in log
    pub data: ~str, // data or "command" to log
    pub uuid: ~str, // unique id from client, recommended (but not reqd) to be of UUID format
}

impl Log {
    pub fn new(path: Path) -> IoResult<~Log> {
        let mut start_idx = 0;
        let mut term = 0;
        if path.exists() {
            let last_entry = try!(read_last_entry(&path));
            match LogEntry::decode(last_entry) {
                Ok(logentry) => {
                    start_idx = logentry.idx;
                    term = logentry.term;
                },
                Err(_) => ()
            }
        }

        // TODO: will need a log rotation strategy later
        let file = try!(File::open_mode(&path, Append, Write));

        let lg = ~Log {
            file: file,
            path: path,
            idx: start_idx,
            term: term,
            logentries: Vec::with_capacity(4096),
        };

        Ok(lg)
    }

    ///
    /// Writes multiple log entries to the end of the log or truncates the log to the first_idx
    /// in the aereq if there is a mismatch between terms for the same indexed entry.
    /// This method should only be called when the AER has log entries (not heartbeat)
    ///
    pub fn append_entries(&mut self, aereq: &AppendEntriesReq) -> IoResult<()> {
        if aereq.prev_log_idx < self.idx {
            try!(self.truncate(aereq.prev_log_idx + 1));
            return Err(IoError{kind: InvalidInput,
                               desc: "prev_log_idx < self.log.idx mismatch",
                               detail: Some(format!("aereq.prev_log_idx: {:u}; folower log idx {:u}",
                                                    aereq.prev_log_idx, self.idx))});
        }

        if aereq.prev_log_idx > self.idx {
            return Err(IoError{kind: InvalidInput,
                               desc: "prev_log_idx < self.log.idx mismatch",
                               detail: Some(format!("aereq.prev_log_idx: {:u}; folower log idx {:u}",
                                                    aereq.prev_log_idx, self.idx))});
        }

        /////////////
        if aereq.prev_log_term != self.term && self.term != 0 {
            try!(self.truncate(aereq.prev_log_term));
            return Err(IoError{kind: InvalidInput,
                               desc: "term mismatch at prev_log_idx",
                               detail: Some(format!("term in follower is {:u}", self.term))});
        }
        for e in aereq.entries.iter() {
            try!(self.append_entry(e));
        }
        Ok(())
    }

    ///
    /// Writes a single log entry to the end of the log.
    ///
    pub fn append_entry(&mut self, entry: &LogEntry) -> IoResult<()> {
        // TODO: may need locking here later (goraft uses it -> does schooner need it?)
        if entry.term < self.term {
            // TODO: should this state kick off a change from follower to candidate?
            let errmsg = format!("schooner.Log: Term of entry ({:u}) is earlier than current term ({:u})",
                                 entry.term, self.term);
            return Err(IoError{kind: InvalidInput,
                               desc: "term violation",
                               detail: Some(errmsg)});

        } else if entry.term == self.term && entry.idx <= self.idx && entry.idx != 0 {
            // TODO: does this stage need a truncation? (I think not)
            let errmsg = format!("schooner.Log: Cannot append entry with earlier index in the same term ({:u}:{:u} <= {:u}:{:u})",
                                 entry.term, entry.idx,
                                 self.term, self.idx);
            return Err(IoError{kind: InvalidInput,
                               desc: "index violation",
                               detail: Some(errmsg)});
        }

        let jstr = json::Encoder::str_encode(entry) + "\n";
        try!(self.file.write_str(jstr));

        self.idx = entry.idx;
        self.term = entry.term;
        self.logentries.push(entry.clone());
        // self.logentries.push(LogEntry{idx: entry.idx, term: entry.term, data: entry.data.to_owned(), uuid: entry.uuid.to_owned()});

        Ok(())
    }

    ///
    /// truncate_log finds the entry in the log that matches the entry passed in
    /// and removes it and all subsequent entries from the log, effectively
    /// truncating the log on file.  It also truncates the in-memory idx-term
    /// vector to match what is on file.
    ///
    pub fn truncate(&mut self, entry_idx: u64) -> IoResult<()> {
        let r: u64 = rand::random();
        let tmppath = Path::new(format!("{}-{:u}", self.path.display(), r));

        {
            let file = try!(File::open_mode(&self.path, Open, Read));
            let tmpfile = try!(File::open_mode(&tmppath, Open, Write));

            let mut br = BufferedReader::new(file);
            let mut bw = BufferedWriter::new(tmpfile);
            for ln in br.lines() {
                let line = ln.unwrap();
                match LogEntry::decode(line.trim()) {
                    Ok(curr_ent) => {
                        if curr_ent.idx == entry_idx {
                            break;  // TODO: can you break out of an iterator loop?
                        } else {
                            try!(bw.write_str(line));
                        }
                    },
                    Err(e) => fail!("schooner.log.truncate_log: The log {} is corrupted: {:?}",
                                    self.path.display().to_str(), e)
                }
            }
        }
        // truncate in-memory term/idx vector
        // NOTE: assumes we keep all log entries in place => otherwise have to offset by first idx entry in logentries
        // FIXME: dangerous => converting u64 to uint, so can only cache up to uint entries
        let truncate_idx = (entry_idx - 1) as uint;  // have to subtract bcs idx is 1-based, not 0-based
        if truncate_idx < self.logentries.len() {
            self.logentries.truncate(truncate_idx);
        }

        if self.logentries.len() == 0 {
            self.idx = 0;
            self.term = 0;
        } else {
            let last_entry = self.logentries.get(self.logentries.len() - 1);
            self.idx = last_entry.idx;
            self.term = last_entry.term;
        }

        // now rename/swap files
        try!(fs::unlink(&self.path));
        try!(fs::rename(&tmppath,&self.path));

        // restore self.file to append to end of newly truncated file
        self.file = try!(File::open_mode(&self.path, Append, Write));
        Ok(())
    }
}

impl LogEntry {
    fn decode(json_str: &str) -> Result<LogEntry, json::Error> {
        match json::from_str(json_str) {
            Ok(jobj) => {
                let mut decoder = json::Decoder::new(jobj);
                Decodable::decode(&mut decoder)
            },
            Err(e) => Err(e)
        }
    }

    pub fn encode(&self) -> ~str {
        json::Encoder::str_encode(self)
    }
}

// TODO: somewhere better?
fn read_last_entry(path: &Path) -> IoResult<~str> {
    let file = try!(File::open(path));
    let mut br = BufferedReader::new(file);

    let mut last_line: ~str = ~"";
    loop {
        match br.read_line() {
            Ok(ln) => last_line = ln,
            Err(ref e) if e.kind == EndOfFile => break,
            Err(e) => return Err(e)
        }
    }
    Ok(last_line)
}


#[cfg(test)]
mod test {
    use std::io::fs;
    use std::io::{BufferedReader,File};
    
    use serialize::{json, Decodable};
    use uuid::{Uuid, UuidVersion, Version4Random};

    use super::LogEntry;
    use super::super::events::append_entries::AppendEntriesReq;

    static testlog: &'static str = "datalog/log.test";

    fn cleanup() {
        let p = Path::new(testlog);
        let _ = fs::unlink(&p);
    }

    fn num_entries_in_test_log() -> uint {
        let p = Path::new(testlog);
        let f = File::open(&p);
        let mut br = BufferedReader::new(f);
        let mut count: uint = 0;
        for _ in br.lines() {
            count += 1;
        }
        count
    }

    #[test]
    fn test_truncate() {
        cleanup();
        // need to write some logs first
        let logent1 = LogEntry{idx: 1, term: 1, data: ~"a", uuid: ~"u01"};
        let logent2 = LogEntry{idx: 2, term: 1, data: ~"b", uuid: ~"u02"};
        let logent3 = LogEntry{idx: 3, term: 1, data: ~"c", uuid: ~"u03"};
        let logent4 = LogEntry{idx: 4, term: 1, data: ~"d", uuid: ~"u04"};
        let logent5 = LogEntry{idx: 5, term: 2, data: ~"e", uuid: ~"u05"};
        let logent6 = LogEntry{idx: 6, term: 2, data: ~"f", uuid: ~"u06"};

        let rlog = super::Log::new(Path::new(testlog));
        let mut aer = AppendEntriesReq{term: 1, prev_log_idx: 0, prev_log_term: 0,
                                       commit_idx: 2, leader_id: 9,
                                       uuid: Uuid::new(Version4Random).unwrap(),
                                       entries: vec!(logent1.clone(), logent2, logent3.clone(), logent4.clone())};
        assert!(rlog.is_ok());
        let mut log = rlog.unwrap();

        let result = log.append_entries(&aer);
        println!("1: {:?}", result);
        assert!(result.is_ok());

        aer = AppendEntriesReq{term: 2, prev_log_idx: 4, prev_log_term: 1,
                                commit_idx: 2, leader_id: 9,
                                uuid: Uuid::new(Version4Random).unwrap(),
                                entries: vec!(logent5.clone(), logent6)};

        let result = log.append_entries(&aer);
        println!("2: {:?}", result);
        assert!(result.is_ok());

        assert_eq!(6, num_entries_in_test_log());

        // now truncate
        let result = log.truncate(logent4.idx);
        println!("3: {:?}", result);
        assert!(result.is_ok());
        assert_eq!(3, num_entries_in_test_log());

        let result = log.truncate(logent3.idx);
        println!("4: {:?}", result);
        assert!(result.is_ok());
        assert_eq!(2, num_entries_in_test_log());

        // try to truncate one that isn't there
        let result = log.truncate(logent5.idx);
        println!("5: {:?}", result);
        assert!(result.is_ok());
        assert_eq!(2, num_entries_in_test_log());  // num entries didn't change

        // now truncate the very first entry
        let result = log.truncate(logent1.idx);
        println!("6: {:?}", result);
        assert!(result.is_ok());
        assert_eq!(0, num_entries_in_test_log());
    }

    #[test]
    fn test_json_encode_of_LogEntry() {
        let uuidstr = Uuid::new_v4().to_hyphenated_str();
        let logentry = super::LogEntry{idx: 155, term: 2, data: ~"wc", uuid: uuidstr.clone()};

        let jstr = logentry.encode();
        assert!(jstr.len() > 0);
        assert!(jstr.contains("\"idx\":155"));
        assert!(jstr.contains("\"data\":\"wc\""));
        assert!(jstr.contains(format!("\"uuid\":\"{}\"", uuidstr)));
    }

    // TODO: what does this test even test? Surely we should always be
    // using the "right" decode/encode methods?
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
        let result = LogEntry::decode(jstr);
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
        let result = LogEntry::decode(jstr);
        assert!(result.is_err());
    }
}

