use uuid::Uuid;

#[deriving(Decodable, Encodable, Eq, Clone, Show)]
pub struct VoteReq {
    pub term: uint,
    pub candidate_id: uint,
    pub last_log_index: uint,
    pub last_log_term: uint,
    pub uuid: Uuid,
}

#[deriving(Decodable, Encodable, Eq, Clone, Show)]
pub struct VoteRes {
    pub term: uint,
    pub vote_granted: bool,
    pub uuid: Uuid,
}
