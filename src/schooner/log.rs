extern crate serialize;

use std::io::{BufferedReader,File,IoResult,IoError,InvalidInput,EndOfFile,Append,Write};
use std::vec;
use std::vec_ng::Vec;
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
            // TODO: can we use try! here?
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

    /// TODO: REMOVE
    /// Writes multiple log entry to the end of the log.
    ///
    pub fn append_entries2(&mut self, entries: Vec<LogEntry>) -> IoResult<()> {
        for e in entries.move_iter() {
            try!(self.append_entry(e));
        }
        Ok(())
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
            try!(self.append_entry(e.clone()));
        }
        Ok(())
    }

    ///
    /// Writes a single log entry to the end of the log.
    ///
    pub fn append_entry(&mut self, entry: LogEntry) -> IoResult<()> {
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

        let jstr = json::Encoder::str_encode(&entry) + "\n";
        try!(self.file.write_str(jstr));
        self.start_idx = entry.index;
        self.start_term = entry.term;
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
