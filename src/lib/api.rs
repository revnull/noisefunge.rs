
use crate::befunge::{CrashReason, Note};

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, Condvar};
use std::thread;
use std::time::Duration;

use reqwest::blocking::Client;
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
    pub call_stack: usize,
    pub play: Option<Note>,
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


pub struct FungeClient(
    Arc<(Mutex<Option<Result<EngineState, String>>>, Condvar)>);

impl FungeClient {
    pub fn new(baseuri: &str) -> Self {
        let mtx = Mutex::new(None);
        let cond = Condvar::new();
        let arc = Arc::new((mtx, cond));
        let arc2 = Arc::clone(&arc);
        let basereq = format!("{}state", baseuri);

        thread::spawn(move || {
            let lock = &arc.0;
            let cond = &arc.1;
            let mut prev = 0;
            let mut delay = false;
            let client = Client::builder().user_agent("nfbuffer")
                                          .build()
                                          .expect("Failed to build client");
            loop {
                if delay {
                    delay = false;
                    thread::sleep(Duration::from_secs(1));
                };

                let request = client.get(&basereq)
                                    .query(&[("prev", prev.to_string())])
                                    .timeout(Duration::from_secs(4))
                                    .build()
                                    .expect("Failed to build client");
                let response = client.execute(request);
                let msg = match response {
                    Ok(response) => {
                        if response.status().is_success() {
                            response.json().map_err(|e|
                                format!("Serialization error: {:?}", e))
                                .map(|s: EngineState| { prev = s.beat; s })
                        } else {
                            delay = true;
                            prev = 0;
                            Err(format!("Bad status code: {:?}",
                                        response.status()))
                        }
                    }
                    Err(e) => {
                        delay = true;
                        Err(format!("HTTP request failed: {:?}", e))
                    }
                };
                let mut val = lock.lock().unwrap();
                while val.is_some() {
                    val = cond.wait(val).unwrap();
                };
                *val = Some(msg);
                cond.notify_one();
            }
        });

        FungeClient(arc2)
    }

    pub fn get_state(&self, sleep_dur: Duration)
        -> Option<Result<EngineState,String>> {

        let lock = &(self.0).0;
        let cond = &(self.0).1;

        let (mut val, timeout) = cond.wait_timeout_while(
                lock.lock().unwrap(),
                sleep_dur,
                |value| value.is_none()).unwrap();

        if timeout.timed_out() { return None }

        cond.notify_one();
        val.take()
    }

}
