clean:
	rm -f *lib server log log_entry append_entries_request serror

server:
	rustc server.rs


# ---[ checks for compilation success ]--- #
compile: log-compile logentry-compile appendentries-compile server


log-compile:
	rustc --crate-type=lib schooner/log.rs

logentry-compile:
	rustc --crate-type=lib schooner/log_entry.rs

appendentries-compile:
	rustc --crate-type=lib schooner/append_entries.rs


# ---[ tests ]--- #
test: log-test logentry-test appendentries-test server-test

log-test:
	rustc --test schooner/log.rs
	./log

logentry-test:
	rustc --test schooner/log_entry.rs
	./log_entry

appendentries-test:
	rustc --test schooner/append_entries.rs
	./append_entries

server-test:
	rustc --test server.rs
	./server

