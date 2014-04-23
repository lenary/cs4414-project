#### Case 1: Terms match, Leader behind follower

Log status time 1:

    Leader  : | ... | 3/10 | 3/11 |
    Follower: | ... | 3/10 | 3/11 | 3/12 |

Leader sends AERequest (heartbeat):

    prev_log_idx : 11
    prev_log_term: 3
    entries      : []
    
Follower:
* truncates to idx 11
* sends AEResponse:

    success: false
    idx    : 11
    term   : 3

Log status time 2:

    Leader  : | ... | 3/10 | 3/11 |
    Follower: | ... | 3/10 | 3/11 |


Leader
* sees indexes and terms match
* sends AERequest:

    prev_log_idx : 11
    prev_log_term: 3
    entries      : []
    

Follower:
* sends AEResponse:

    success: success
    idx    : 11
    term   : 3



<br>
#### Case 2a: terms match, leader ahead of follower, heartbeat

Log status time 1:

    Leader  : | ... | 3/10 | 3/11 | 3/12 | 3/13 |
    Follower: | ... | 3/10 | 3/11 |

Leader sends AERequest (heartbeat):

    prev_log_idx : 13
    prev_log_term: 3
    entries      : []


Follower:
* sees it is behind so rejects log request
* sends AEResponse:

    success: false
    idx    : 11
    term   : 3

Leader:
* calculates that it needs to send 12,13
* sends AERequest

    prev_log_idx : 11
    prev_log_term: 3
    entries      : [3/12,3/13]


Follower:
* logs three new entries
* sends AEResponse:

    success: success
    idx    : 13
    term   : 3



<br>
#### Case 2b: terms match, leader ahead of follower, with new entry

Log status time 1:

    Leader  : | ... | 3/10 | 3/11 | 3/12 | 3/13 |
    Follower: | ... | 3/10 | 3/11 |

Leader sends AERequest (with new entry):

    prev_log_idx : 13
    prev_log_term: 3
    entries      : [3/14]


Follower:
* sees it is behind so rejects log request
* sends AEResponse:

    success: false
    idx    : 11
    term   : 3

Leader:
* calculates that it needs to send 12,13 and the new 14
* sends AERequest

    prev_log_idx : 11
    prev_log_term: 3
    entries      : [3/12,3/13,3/14]


Follower:
* logs three new entries
* sends AEResponse:

    success: success
    idx    : 14
    term   : 3


**Note**!! Not handling this correctly because 3/14 is *not* in the the leader's log yet, so would have either put it the in-memory log (uncommited)
or hold it in a pending "register" during the window to respond to the client





<br>
#### Case 3: terms do not match - leader term ahead of follower, indexes the same

Log status time 1:

    Leader  : | 2/9 | 3/10 | 3/11 |
    Follower: | 2/9 | 2/10 | 2/11 |

Leader sends AERequest (heartbeat):

    prev_log_idx : 11
    prev_log_term: 3
    entries      : []


Follower:
* sees that leader term > follower term for idx 11, so truncates that entry
* sends AEResponse:

    success: false
    idx    : 10
    term   : 2

Leader:
* could optimize by noting that terms of 10 don't match, but takes simple approach: 
 * back up to the idx of follower (ignoring that terms of idx 10 are different)
* sends AERequest

    prev_log_idx : 10
    prev_log_term: 3
    entries      : [3/11]


Follower:
* sees that leader term > follower term for idx 10, so truncates that entry
* sends AEResponse:

    success: false
    idx    : 9
    term   : 2


Log status time 2:

    Leader  : | 2/9 | 3/10 | 3/11 |
    Follower: | 2/9 |


Leader:
* sends AERequest

    prev_log_idx : 9
    prev_log_term: 2
    entries      : [3/10,3/11]


Follower:
* has match, applies two entries
* sends AEResponse:

    success: term
    idx    : 11
    term   : 3

Log status time 3:

    Leader  : | 2/9 | 3/10 | 3/11 |
    Follower: | 2/9 | 3/10 | 3/11 |




<br>
#### Case 4: terms do not match - leader term ahead of follower, leader ahead of follower

Log status time 1:

    Leader  : | ... | 2/10 | 3/11 | 3/12
    Follower: | ... | 2/10 | 2/11 |

Leader sends AERequest (heartbeat):

    prev_log_idx : 12
    prev_log_term: 3
    entries      : []


Follower:
* sees that leader is ahead
* sends AEResponse:

    success: false
    idx    : 11
    term   : 2

*(the above is the same as case 2a)*


Leader:
* could optimize by noting that terms of 11 don't match, but takes simple approach: 
 * back up to the idx of follower (ignoring that terms of idx 10 are different)
* sends AERequest

    prev_log_idx : 11
    prev_log_term: 3
    entries      : [3/12]

Follower:
* sees that leader term > follower term for idx 11, so truncates that entry
* sends AEResponse:

    success: false
    idx    : 10
    term   : 2


Log status time 2:

    Leader  : | ... | 2/10 | 3/11 | 3/12
    Follower: | ... | 2/10 |

*(the above is the same as case 3)*

This finishes like case 3




#### Summary:

Really there are 2 responses for the leader when follower replies `false`:

1. If the follower is behind (idx), send all missing entries
2. If the follower has the same (idx), send heartbeat of latest status

###### Issues / Questions

* The current implementation does **not** use the peer.next_idx field, so it does not follow the Raft spec, which says the server should maintain that.  Schooner currently maintains it, but doesn't need it.  Schooner leader entirely goes off the aeresp.idx -> the next_idx is one after that.  Will this be a problem for some later use case?

* When a new client cmd comes in, is sent to a follower as an AERequest, and the follower rejects the write: The current implementation does not remember that there is a new request to send, because it is not in the follower log.
 * This implies that we'll need to follow the Raft spec and log the entry before it is committed in the leader
 * One suggestion is to log it only in memory, but don't commit it to log (and stateMachine) until (majority-1) followers have logged it


