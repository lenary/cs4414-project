all: server


clean:
	rm -f *lib server log log_entry append_entries_request serror

server:
	rustc server.rs

test:
	rustc --test server.rs
	export RUST_TEST_TASKS=1 && ./server
