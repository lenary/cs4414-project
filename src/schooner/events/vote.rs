#[deriving(Decodable, Encodable)]
pub struct VoteReq {
    // Not sure if votes need this, but they *do* need at least one field
    // to use the #[deriving(...)] stuff.
    pub id: uint,
    // TODO: fill this out
}

#[deriving(Decodable, Encodable)]
pub struct VoteRes {
    // Not sure if votes need this, but they *do* need at least one field
    // to use the #[deriving(...)] stuff.
    pub id: uint,
    // TODO: fill this out
}
