all: server

clean:
	rm -f server

server:
	rustc server.rs

test:
	rustc --test server.rs
	export RUST_TEST_TASKS=1 && ./server
