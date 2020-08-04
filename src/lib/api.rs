
use crate::befunge::CrashReason;

use std::collections::{BTreeMap, HashMap};
use serde::{Serialize, Deserialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct NewProcessReq {
    pub name: Option<String>,
    pub program: String
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NewProcessResp { pub pid: u64 }

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProcState {
    pub name: Option<usize>,
    pub prog: usize,
    pub pc: usize,
    pub active: bool,
    pub output: Option<String>,
    pub data_stack: usize,
    pub call_stack: usize
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct EngineState {
    pub beat: u64,
    pub names: Vec<String>,
    pub progs: Vec<(usize, String)>,
    pub procs: HashMap<u64, ProcState>,
    pub sleeping: usize,
    pub buffers: BTreeMap<u8, i64>,
    pub crashed: Vec<(u64, CrashReason)>
}

impl EngineState {
    pub fn new() -> Self {
        EngineState {
            beat: 0,
            names: Vec::new(),
            progs: Vec::new(),
            procs: HashMap::new(),
            sleeping: 0,
            buffers: BTreeMap::new(),
            crashed: Vec::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum KillReq {
    Pids(Vec<u64>),
    Names(Vec<String>),
    All
}

unsafe impl Send for KillReq {}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct KillResp { }

