
use std::comm::*;
use super::traits::RaftState;
use super::events::RaftEvent;

struct StateMachine<'a> {
    select: ~Select,
    event_rec: Handle<'a, ~RaftEvent>,
    timer_rec: Handle<'a, ()>,
    application_rec: Handle<'a, ~RaftEvent>,
}

// TODO: how the fuck do we stop?
fn go_go_gadget(init_state: ~RaftState, mut sm: ~StateMachine) {
    let mut current_state = init_state;
    'outer: loop {
        match current_state.handle_setup() {
            Some(new_state) => {
                current_state.handle_teardown();
                current_state = new_state;
                continue 'outer;
            },
            None => {
                'inner: loop {
                    let ready_id = sm.select.wait();
                    if (ready_id == sm.timer_rec.id()) {
                        sm.timer_rec.recv();
                        match current_state.handle_timeout() {
                            Some(new_state) => {
                                current_state.handle_teardown();
                                current_state = new_state;
                                continue 'outer
                            },
                            None => {}
                        };
                    }
                    else if (ready_id == sm.event_rec.id()) {
                        // workout which one to call, get its result
                        let event = sm.event_rec.recv();
                        match current_state.handle_event(event) {
                            Some(new_state) => {
                                current_state.handle_teardown();
                                current_state = new_state;
                                continue 'outer
                            },
                            None => {}
                        };
                    }
                    else if (ready_id == sm.application_rec.id()) {
                        let event = sm.application_rec.recv();
                        match current_state.handle_event(event) {
                            Some(new_state) => {
                                current_state.handle_teardown();
                                current_state = new_state;
                                continue 'outer
                            },
                            None => {}
                        };
                    }
                }
            }
        }
    }
}

