Schooner
========

An implementation of the
[Raft distributed consensus algorithm](https://ramcloud.stanford.edu/wiki/download/attachments/11370504/raft.pdf)
in Rust.

We set out to implement the Raft Consensus Algorithm in Rust. We
joined an existing project by Michael, called Schooner. Raft is a
Distributed Consensus Algorithm - this means that it is a system that
can establish a consistent log across multiple machines in an
asynchronous, lossy network (like those in the real world).

Raft was designed with understandability (and pedagogy) in mind, as
opposed to Paxos, another consensus algorithm that is renowned for its
obscure definition. Raft is new, but as a testament to its
understandability, there are many more Open Source implementations of
it than Paxos, despite Raft being about 20 years younger than Paxos.

Almost all the project documentation is in the
[project wiki](https://github.com/lenary/cs4414-project/wiki),
including a Glossary of terms, a condensed Raft guide, and an overview
of the design of Schooner.

### Background

Other links about the Raft protocol:

* http://raftconsensus.github.io/
* https://www.youtube.com/watch?v=YbZ3zDzDnrw
* http://www.infoq.com/presentations/raft
  * [Slides for this presentation](https://speakerdeck.com/benbjohnson/raft-the-understandable-distributed-consensus-protocol)
* http://www.reddit.com/comments/1jm6c8

The wikipedia pages, describing:

* The Consensus Problem: http://en.wikipedia.org/wiki/Consensus_(computer_science)
* Paxos: http://en.wikipedia.org/wiki/Paxos_(computer_science)
* Raft: http://en.wikipedia.org/wiki/Raft_consensus_algorithm

### LICENSE

The library will licensed under the permissive
[MIT License](http://opensource.org/licenses/MIT), as defined in the
`LICENCE` file.

