extern crate rand;
extern crate serialize;

use std::io::{BufferedReader,BufferedWriter,File,IoResult,IoError,InvalidInput,EndOfFile,Append,Open,Read,Write};
use std::io::fs;
use std::vec;
use std::vec_ng::Vec;

//use rand::random;
use serialize::json;

use schooner::append_entries::AppendEntriesRequest;
use schooner::log_entry::LogEntry;

pub mod append_entries;
pub mod log_entry;

pub struct Log {
    file: File,      // open File with Append/Write state
    path: Path,      // path to log // TODO: dir? file?
    entries: ~[LogEntry],  // TODO: should this be ~[~LogEntry] ??
    commit_idx: u64, // last committed index
    start_idx: u64,  // idx  of last entry in the logs (before first entry in latest issue) (may not be committed)
    start_term: u64, // term of last entry in the logs (before first entry in latest issue)
}

impl Log {
    #[allow(deprecated_owned_vector)]  // TODO: remove later
    pub fn new(path: Path) -> IoResult<~Log> {
        let mut start_idx = 0;
        let mut term = 0;
        if path.exists() {
            let last_entry = try!(read_last_entry(&path));
            match log_entry::decode_log_entry(last_entry) {
                Ok(logentry) => {
                    start_idx = logentry.index;
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
            entries: vec::with_capacity(8),  // necessary?
            commit_idx: start_idx, // TODO: do we know for sure it's committed just bcs it's in the file log?
            start_idx: start_idx,
            start_term: term,
        };

        return Ok(lg);
    }

    ///
    /// Writes multiple log entry to the end of the log.
    ///
    pub fn append_entries(&mut self, aereq: &AppendEntriesRequest) -> IoResult<()> {
        if aereq.prev_log_idx != self.start_idx {  // TODO: this may not be an error condition => need to research
            return Err(IoError{kind: InvalidInput,
                               desc: "prev_log_idx and start_idx mismatch",
                               detail: Some(format!("start_idx in follower is {:u}", self.start_idx))});

        }
        if aereq.prev_log_term != self.start_term {
            return Err(IoError{kind: InvalidInput,
                               desc: "term mismatch at prev_log_idx",
                               detail: Some(format!("start_term in follower is {:u}", self.start_term))});
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
        // TODO: may need locking here later
        if entry.term < self.start_term {
            let errmsg = format!("schooner.Log: Term of entry ({:u}) is earlier than current term ({:u})",
                                 entry.term, self.start_term);
            return Err(IoError{kind: InvalidInput,
                               desc: "term violation",
                               detail: Some(errmsg)});

        } else if entry.term == self.start_term && entry.index <= self.start_idx {
            let errmsg = format!("schooner.Log: Cannot append entry with earlier index in the same term ({:u}:{:u} <= {:u}:{:u})",
                                 entry.term, entry.index,
                                 self.start_term, self.start_idx);
            return Err(IoError{kind: InvalidInput,
                               desc: "index violation",
                               detail: Some(errmsg)});
        }

        let jstr = json::Encoder::str_encode(entry) + "\n";
        try!(self.file.write_str(jstr));
        self.start_idx = entry.index;
        self.start_term = entry.term;
        Ok(())
    }

    ///
    /// truncate_log finds the entry in the log that matches the entry passed in
    /// and removes it and all subsequent entries from the log, effectively
    /// truncating the log on file.
    /// 
    pub fn truncate_log(&mut self, entry: &LogEntry) -> IoResult<()> {
        // let r: u64 = random();
        let r: u64 = 123456789;  // FIXME
        // let tmppath = Path::new(&self.path.display().to_str().to_owned() + r.to_str().to_owned());
        let tmppath = Path::new("foo" + r.to_str());

        {
            let file = try!(File::open_mode(&self.path, Open, Read));
            let tmpfile = try!(File::open_mode(&self.path, Open, Write));

            let mut br = BufferedReader::new(file);
            let mut bw = BufferedWriter::new(tmpfile);
            for ln in br.lines() {
                // TODO: does trim remove newline?
                let line = ln.unwrap();
                match log_entry::decode_log_entry(line.trim()) {
                    Ok(curr_ent) => {
                        if curr_ent.index == entry.index {
                            break;  // TODO: can you break out of an iterator loop?
                        } else {
                            try!(bw.write_str(line));
                        }
                    },
                    Err(e) => fail!("schooner.log.truncate_log: The log {} is corrupted.",
                                    self.path.display().to_str())
                }
            }
        }
        // now rename/swap files
        try!(fs::unlink(&self.path));
        try!(fs::rename(&tmppath,&self.path));
        
        // restore self.file to append to end of newly truncated file
        self.file = try!(File::open_mode(&self.path, Append, Write));
        Ok(())
    }
}

// TODO: need to make private?
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
    // FAIL: imports but has wrong "path"
    // use super::append_entries::AppendEntriesRequest;
    // use super::log_entry::LogEntry;
    // use append_entries::AppendEntriesRequest;
    // use log_entry::LogEntry;
    // use super::Log;

    // FAIL: cannot reference mod directly
    // pub mod append_entries;
    // pub mod log_entry;

    #[test]
    fn test_truncate() {
        // TODO: remove command from the logentry !!!!!
        // need to write some logs first
        let logent1 = super::log_entry::LogEntry{index: 1, term: 1, command_name: ~"a", command: None};
        let logent2 = super::log_entry::LogEntry{index: 2, term: 1, command_name: ~"b", command: None};
        let logent3 = super::log_entry::LogEntry{index: 3, term: 1, command_name: ~"c", command: None};
        let logent4 = super::log_entry::LogEntry{index: 4, term: 1, command_name: ~"d", command: None};
        let logent5 = super::log_entry::LogEntry{index: 5, term: 2, command_name: ~"e", command: None};
        let logent6 = super::log_entry::LogEntry{index: 6, term: 2, command_name: ~"f", command: None};

        let rlog = super::Log::new(Path::new(~"datalog/log.test"));
        let mut aer = super::append_entries::AppendEntriesRequest{term: 1, prev_log_idx: 0, prev_log_term: 0,
                                                                  commit_idx: 0, leader_name: ~"fred",
                                                                  entries: vec!(logent1, logent2, logent3, logent4)};

        let result = rlog.append_entries(aer);
        assert!(result.is_ok());
        
        aer = super::append_entries::AppendEntriesRequest{term: 1, prev_log_idx: 0, prev_log_term: 0,
                                                          commit_idx: 0, leader_name: ~"fred",
                                                          entries: vec!(logent5, logent6)};

        let result = rlog.append_entries(aer);
        assert!(result.is_ok());
        // now truncate
        // LEFT OFF => not done
    }
}
