
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct NewProcessReq {
    pub input: String,
    pub output: String,
    pub program: String
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewProcessResp { pub pid: u64 }

