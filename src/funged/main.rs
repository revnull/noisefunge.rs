
use noisefunge::jack::*;
use noisefunge::server::*;
use noisefunge::server::FungeRequest::*;
use noisefunge::befunge::*;
use noisefunge::config::*;
use noisefunge::api::*;
use noisefunge::midi_bridge::*;
use std::{thread, time};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crossbeam_channel::select;
use serde_json::{to_vec};

use clap::{Arg, App};

fn read_args() -> String {
    let matches = App::new("funged")
                          .arg(Arg::with_name("CONFIG")
                               .help("Config file to use")
                               .required(true))
                          .get_matches();
    String::from(matches.value_of("CONFIG").unwrap())
}

struct FungedServer {
    config: FungedConfig,
    engine: Engine,
    state: EngineState,
    state_vec: Arc<Vec<u8>>,
    waiting: Vec<(u64, Responder<Option<Arc<Vec<u8>>>>)>
}

impl FungedServer {

    fn new(conf: FungedConfig) -> Self {
        let engine = Engine::new(conf.period);
        let state = engine.state();
        let state_vec = Arc::new(to_vec(&state).unwrap());
        FungedServer {
            config: conf,
            engine: engine,
            state: state,
            state_vec: state_vec,
            waiting: Vec::new()
        }
    }

    fn handle(&mut self, request: FungeRequest) {
        match request {
            StartProcess(prog, rspndr) =>
                rspndr.respond(match Prog::parse(&prog) {
                    Ok(p) => Ok(self.engine.make_process(p)),
                    Err(e) => Err(e.to_string())
                }),
            GetState(prev, rspndr) => {
                let prev = prev.unwrap_or(0);
                if prev < self.state.beat {
                    rspndr.respond(Some(Arc::clone(&self.state_vec)));
                } else if prev > self.state.beat {
                    rspndr.respond(None);
                } else {
                    self.waiting.push((prev, rspndr));
                }
            },
            r => panic!("Failed to handle: {:?}", r),
        };
    }

    fn update_state(&mut self) {
        self.state = self.engine.state();
        self.state_vec = Arc::new(to_vec(&self.state).unwrap());
        let state_vec = &self.state_vec;
        let beat = self.state.beat;
        self.waiting.retain(|(prev, rspndr)|
            if *prev < beat {
                rspndr.respond(Some(Arc::clone(state_vec)));
                false
            } else {
                true
            }
        );
    }
}

fn main() {

    let mut server = FungedServer::new(
        FungedConfig::read_config(&read_args()));

    let mut handle = JackHandle::new(&server.config);
    let mut bridge = MidiBridge::new(&server.config, &handle);
    let http_serv = ServerHandle::new(&server.config);
    let mut prev_i = 0;

    loop {
        select! {
            recv(handle.beat_channel) -> msg => {
                let i = msg.expect("Failed to read from beat channel.");
                for j in prev_i..i {
                    if j % server.config.period == 0 {
                        let (beat, log) = server.engine.step();
                        bridge.step(beat, &log);
                        for n in log {
                            println!("log: {:?}", n);
                        };
                    }
                };
                server.update_state();
                prev_i = i;
            },
            recv(http_serv.channel) -> msg => {
                match msg {
                    Ok(req) => server.handle(req),
                    Err(e) => println!("Unknown error: {:?}", e),
                };
            }
        }
    }
}
