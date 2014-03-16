clean:
	rm -f *lib server log log_entry append_entries_request serror

server:
	rustc server.rs


# ---[ checks for compilation success ]--- #
compile: log-check logentry-check appendentries-check server


log-check:
	rustc --crate-type=lib log.rs

logentry-check:
	rustc --crate-type=lib log_entry.rs

appendentries-check:
	rustc --crate-type=lib append_entries.rs


# ---[ tests ]--- #
test: log-test logentry-test appendentries-test server-test

log-test:
	rustc --test log.rs
	./log

logentry-test:
	rustc --test log_entry.rs
	./log_entry

appendentries-test:
	rustc --test append_entries.rs
	./append_entries

server-test:
	rustc --test server.rs
	./server

