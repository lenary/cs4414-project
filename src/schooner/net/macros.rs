#![macro_escape]

use std::comm::Disconnected;

#[macro_export]
macro_rules! may_shutdown(
    ($p: ident) => {
        match $p.try_recv() {
            Ok(exitcode) => {
                debug!("Received shutdown signal {} on subsystem.", exitcode);
                break;
            },
            Err(_) => {
            },
        }
    };
    ($s: ident, $p: ident) => {
        match $s.$p.try_recv() {
            Ok(exitcode) => {
                debug!("Received shutdown signal {} on NetListener.", exitcode);
                for sender in $s.shutdown_senders.iter() {
                    sender.send(exitcode);
                }
                break;
            },
            Err(_) => {
            },
        }
    };
)


#[macro_export]
macro_rules! try_update (
    ($s: ident, $p: ident, $m: ident) => {
        match $s.$p.try_recv() {
            Ok((incoming_id, incoming_stream)) => {
                let msg_port = $s.$m.get(&incoming_id);
                msg_port.send((incoming_id, incoming_stream));
            }
            Err(Disconnected) => {
                fail!("Channel broke.");
            }
            _ => {},
        }
    };
)
