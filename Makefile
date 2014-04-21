all: server

.PHONY : test clean

clean:
	rm -f server

server:
	rustc -o server src/schooner/mod.rs

test: server
	rustc --test -o server src/schooner/mod.rs
	export RUST_TEST_TASKS=1 && ./server
