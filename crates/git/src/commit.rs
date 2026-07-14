use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CloneRequest {
    pub url: String,
    pub destination_parent: String,
    pub directory_name: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CloneResult {
    pub absolute_path: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommitRequest {
    pub message: String,
    pub amend: bool,
    /// Present only for an in-app retry after the configured signer requested
    /// a passphrase. The server must discard this after the operation.
    pub signing_passphrase: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CommitResult {
    pub oid: String,
    pub summary: String,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum CommitOutcome {
    Committed { commit: CommitResult },
    SigningPassphraseRequired { message: String },
}
