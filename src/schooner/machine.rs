
use std::comm::*;
use super::traits::RaftState;
use super::events::RaftEvent;

struct StateMachine {
    event_rec: Receiver<~RaftEvent>,
    timer_rec: Receiver<()>,
    application_rec: Receiver<~RaftEvent>,
}

// TODO: how the fuck do we stop?
fn go_go_gadget(init_state: ~RaftState, mut sm: ~StateMachine) {
    let mut current_state = init_state;

    let mut selector = Select::new();
    let mut event_handle = selector.handle(&sm.event_rec);
    let mut timer_handle = selector.handle(&sm.timer_rec);
    let mut app_handle   = selector.handle(&sm.application_rec);

    unsafe {
        event_handle.add();
        timer_handle.add();
        app_handle.add();
    }


    'outer: loop {
        match current_state.handle_setup() {
            Some(new_state) => {
                current_state.handle_teardown();
                current_state = new_state;
                continue 'outer;
            },
            None => {
                'inner: loop {
                    let ready_id = selector.wait();
                    if (ready_id == timer_handle.id()) {
                        timer_handle.recv();
                        match current_state.handle_timeout() {
                            Some(new_state) => {
                                current_state.handle_teardown();
                                current_state = new_state;
                                continue 'outer
                            },
                            None => {}
                        };
                    }
                    else if (ready_id == event_handle.id()) {
                        // workout which one to call, get its result
                        let event = event_handle.recv();
                        match current_state.handle_event(event) {
                            Some(new_state) => {
                                current_state.handle_teardown();
                                current_state = new_state;
                                continue 'outer
                            },
                            None => {}
                        };
                    }
                    else if (ready_id == app_handle.id()) {
                        let event = app_handle.recv();
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

