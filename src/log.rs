extern crate serialize;

use std::io::{BufferedReader,File,IoResult,IoError,InvalidInput,EndOfFile,Append,Write};
use std::vec;
use serialize::json;

use log_entry::LogEntry;
use serror::{InvalidArgument,InvalidState,SError};

mod append_entries_request;
mod log_entry;
mod serror;

pub struct Log {
    file: File,      // open File with Append/Write state
    path: Path,      // path to log // TODO: dir? file?
    entries: ~[LogEntry],  // TODO: should this be ~[~LogEntry] ??
    commit_idx: u64, // last committed index
    start_idx: u64,  // idx before first entry in the log entries
    curr_idx: u64,   // idx of most recent entry (Schooner only)
    start_term: u64, // term of last committed entry
    curr_term: u64,  // term of most recent entry (Schooner only)
}

impl Log {
    fn new(path: Path) -> IoResult<~Log> {
        let mut commit_idx = 0;
        let mut term = 0;
        if path.exists() {
            // TODO: can we use try! here?
            let last_entry = try!(read_last_entry(&path));
            match log_entry::decode_log_entry(last_entry) {
                Ok(logentry) => {
                    commit_idx = logentry.index;
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
            commit_idx: commit_idx,
            start_idx: commit_idx,  // TODO: double check this logic
            curr_idx: commit_idx,
            start_term: term,
            curr_term: term
        };

        return Ok(lg);
    }

    ///
    /// Writes a single log entry to the end of the log. 
    /// 
    fn append_entry(&mut self, entry: LogEntry) -> IoResult<()> {
        // TODO: may need locking here later
        if entry.term < self.curr_term {
            let errmsg = format!("schooner.Log: Term of entry ({:u}) is earlier than current term ({:u})",
                                 entry.term, self.curr_term);
            return Err(IoError{kind: InvalidInput,
                               desc: "term violation",
                               detail: Some(errmsg)});
        } else if entry.term == self.curr_term && entry.index <= self.curr_idx {
            let errmsg = format!("schooner.Log: Cannot append entry with earlier index in the same term ({:u}:{:u} <= {:u}:{:u})",
                                 entry.term, entry.index,
                                 self.curr_term, self.curr_idx);
            return Err(IoError{kind: InvalidInput,
                               desc: "index violation",
                               detail: Some(errmsg)});
        }

        let jstr = json::Encoder::str_encode(&entry);
        try!(self.file.write_str(jstr));
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

