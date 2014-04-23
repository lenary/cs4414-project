// Raft State Machine Implementation

use std::comm::*;
use std::io::timer::Timer;

use super::traits::RaftState;
use super::leader::Leader;
use super::candidate::Candidate;
use super::follower::Follower;

use super::events::*;

pub enum RaftNextState {
    RaftLeader(Leader),
    RaftCandidate(Candidate),
    RaftFollower(Follower)
}

pub enum RaftStateTransition {
    NextState(RaftNextState),
    Continue,
    Stop
}


// Save some time by using a macro
// Macro Guide: http://static.rust-lang.org/doc/master/guide-macros.html
macro_rules! transition_proxy(
    ($meth:ident) => (
        match *self {
            RaftLeader(ref mut leader)       => leader.$meth(),
            RaftCandidate(ref mut candidate) => candidate.$meth(),
            RaftFollower(ref mut follower)   => follower.$meth()
        }    
        );
    ($meth:ident, $arg:ident) => (
        match *self {
            RaftLeader(ref mut leader)       => leader.$meth($arg),
            RaftCandidate(ref mut candidate) => candidate.$meth($arg),
            RaftFollower(ref mut follower)   => follower.$meth($arg)
        }    
    )
)

impl RaftState for RaftNextState {
    fn handle_setup(&mut self) -> RaftStateTransition {
        transition_proxy!(handle_setup)
    }

    fn handle_teardown(&mut self) {
        transition_proxy!(handle_teardown)
    }

    fn handle_timeout(&mut self) -> RaftStateTransition {
        transition_proxy!(handle_timeout)
    }

    fn handle_append_entries_req(&mut self, req: AppendEntriesReq) -> RaftStateTransition {
        transition_proxy!(handle_append_entries_req, req)
    }

    fn handle_append_entries_res(&mut self, res: AppendEntriesRes) -> RaftStateTransition {
        transition_proxy!(handle_append_entries_res, res)
    }

    fn handle_vote_req(&mut self, req: VoteReq) -> RaftStateTransition {
        transition_proxy!(handle_vote_req, req)
    }

    fn handle_vote_res(&mut self, res: VoteRes) -> RaftStateTransition {
        transition_proxy!(handle_vote_res, res)
    }

    fn handle_handoff_req(&mut self, req: HandoffReq) -> RaftStateTransition {
        transition_proxy!(handle_handoff_req, req)
    }

    fn handle_handoff_res(&mut self, res: HandoffRes) -> RaftStateTransition {
        transition_proxy!(handle_handoff_res, res)
    }

    fn handle_application_req(&mut self, req: ApplicationReq) -> RaftStateTransition {
        transition_proxy!(handle_application_req, req)
    }

    fn handle_application_res(&mut self, res: ApplicationRes) -> RaftStateTransition {
        transition_proxy!(handle_application_res, res)
    }
}


pub fn sm_loop(init: Follower, event_rec: Receiver<RaftEventMsg>) {

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

    let timer : Receiver<()> = Timer::new().unwrap().periodic(150);

    let select   = Select::new();
    let mut event_handle = select.handle(&event_rec);
    let mut timer_handle = select.handle(&timer);

    unsafe {
        event_handle.add();
        timer_handle.add();
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
        else if event_handle.id() == ready_id {
            let event : RaftEventMsg = event_handle.recv();
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
                APRQ(app_req) =>
                    may_transition!(current_state.handle_application_req(app_req)),
                APRS(app_res) =>
                    may_transition!(current_state.handle_application_res(app_res)),
                SRQ(_) => {
                    current_state.handle_teardown();
                    break;
                }
            }
        }
        // TODO: Channels to/from application parts.
    }

    unsafe {
        event_handle.remove();
        timer_handle.remove();
    }
}
