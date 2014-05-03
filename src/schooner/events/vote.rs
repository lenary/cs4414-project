use uuid::Uuid;

#[deriving(Decodable, Encodable, Eq)]
pub struct VoteReq {
    // Not sure if votes need this, but they *do* need at least one field
    // to use the #[deriving(...)] stuff.
    pub id: uint,
    pub uuid: Uuid,
    // TODO: fill this out
}

#[deriving(Decodable, Encodable, Eq)]
pub struct VoteRes {
    // Not sure if votes need this, but they *do* need at least one field
    // to use the #[deriving(...)] stuff.
    pub id: uint,
    pub uuid: Uuid,
    // TODO: fill this out
}
