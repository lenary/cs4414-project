// Raft State Machine Implementation

use std::comm::Select;

use super::traits::{RaftState, RaftNextState, RaftFollower};
use super::traits::{NextState, Stop, Continue};
use super::events::*;
use super::follower::Follower;

pub fn sm_loop(init: Follower,
                       timer_rec: &Receiver<()>,
                       peer_rec:  &Receiver<RaftPeerMsg>,
                       endpoint_rec: &Receiver<RaftAppMsg>,
                       app_rec: &Receiver<RaftAppMsg>) {

    let mut current_state : RaftNextState = RaftFollower(init);
    let mut is_setup : bool = false;

    // Use every time an invocation may transition the state machine
    // Macro Guide: http://static.rust-lang.org/doc/master/guide-macros.html
    macro_rules! may_transition (
        ($e:expr) => (
            match $e {
                NextState(new_state) => {
                    debug!("Trans {:?} ==> {:?}", current_state, new_state);

                    current_state.handle_teardown();

                    current_state = new_state;
                    is_setup = false;

                    continue;
                },
                Stop => {
                    debug!("Stop {:?} ==> ()", current_state);
                    
                    current_state.handle_teardown();

                    break;
                },
                Continue => {}
            }
        )
    )

    let select   = Select::new();
    let mut timer_handle    = select.handle(timer_rec);
    let mut peer_handle     = select.handle(peer_rec);
    let mut endpoint_handle = select.handle(endpoint_rec);
    let mut app_handle      = select.handle(app_rec);

    unsafe {
        timer_handle.add();
        peer_handle.add();
        endpoint_handle.add();
        app_handle.add();
    }

    loop {
        if !is_setup {
            may_transition!(current_state.handle_setup())
            is_setup = true;
        }

        let ready_id = select.wait();
        if timer_handle.id() == ready_id {
            timer_handle.recv();
            may_transition!(current_state.handle_timeout());
        }
        else if peer_handle.id() == ready_id {
            // An Event Message from a Peer
            let event : RaftPeerMsg = peer_handle.recv();
            match event {
                ARQ(ae_req) => 
                    may_transition!(current_state.handle_append_entries_req(ae_req)),
                ARS(ae_res) =>
                    may_transition!(current_state.handle_append_entries_res(ae_res)),
                VRQ(vote_req) => 
                    may_transition!(current_state.handle_vote_req(vote_req)),
                VRS(vote_res) =>
                    may_transition!(current_state.handle_vote_res(vote_res)),
                HRQ(handoff_req) =>
                    may_transition!(current_state.handle_handoff_req(handoff_req)),
                HRS(handoff_res) =>
                    may_transition!(current_state.handle_handoff_res(handoff_res)),
                StopReq => {
                    may_transition!(Stop);
                }
            }
        }
        else if endpoint_handle.id() == ready_id {
            // An Event Message from an Endpoint
            let event : RaftAppMsg = endpoint_handle.recv();
            match event {
                APRQ(app_req) =>
                    may_transition!(current_state.handle_application_req(app_req)),
                APRS(app_res) =>
                    may_transition!(current_state.handle_application_res(app_res)),
            }
        }
        else if app_handle.id() == ready_id {
            // An Event Message from the Application State Machine Task
            let event : RaftAppMsg = app_handle.recv();
            match event {
                APRQ(app_req) =>
                    may_transition!(current_state.handle_application_req(app_req)),
                APRS(app_res) =>
                    may_transition!(current_state.handle_application_res(app_res)),
            }
        }
    }

    unsafe {
        timer_handle.remove();
        peer_handle.remove();
        endpoint_handle.remove();
        app_handle.remove();
    }
}
