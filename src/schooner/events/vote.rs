use uuid::Uuid;

#[deriving(Decodable, Encodable, Eq, Clone, Show)]
pub struct VoteReq {
    pub term: u64,
    pub candidate_id: u64,
    pub last_log_index: u64,
    pub last_log_term: u64,
    pub uuid: Uuid,
}

#[deriving(Decodable, Encodable, Eq, Clone, Show)]
pub struct VoteRes {
    pub term: u64,
    pub vote_granted: bool,
    pub uuid: Uuid,
}
