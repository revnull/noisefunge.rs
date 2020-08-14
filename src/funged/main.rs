
use log::*;
use simplelog::SimpleLogger;
use noisefunge::jack::*;
use noisefunge::server::*;
use noisefunge::server::FungeRequest::*;
use noisefunge::befunge::*;
use noisefunge::config::*;
use noisefunge::api::*;
use noisefunge::midi_bridge::*;
use noisefunge::subprocess::*;
use std::fs;
use std::sync::{Arc};
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
        let mut engine = Engine::new(conf.period);
        let state = engine.state();
        let state_vec = Arc::new(to_vec(&state).unwrap());

        for filename in &conf.preload {
            let prog = fs::read_to_string(filename).expect(
                &format!("Failed to open preload file: {}", filename));
            let prog = match Prog::parse(&prog) {
                Ok(p) => p,
                Err(e) => panic!("Failed to parse preload file: {} - {:?}",
                                 filename, e),
            };
            info!("Preloaded: {} - {}", filename,
                  engine.make_process(Some(filename.clone()), prog));
        }

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
            StartProcess(name, prog, rspndr) =>
                rspndr.respond(match Prog::parse(&prog) {
                    Ok(p) => Ok(self.engine.make_process(name, p)),
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
            Kill(killreq) => { self.engine.kill(killreq) },
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
    let config = FungedConfig::read_config(&read_args());
    SimpleLogger::init(config.log_level, simplelog::Config::default())
        .expect("Failed to initialize logger");

    let mut server = FungedServer::new(config);

    let _subs = SubprocessHandle::new(&server.config.subprocesses);

    let handle = JackHandle::new(&server.config);
    let mut prev_missed = 0;
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
                    }
                };
                server.update_state();
                prev_i = i;
                let missed = handle.missed();
                if missed != prev_missed {
                    error!("Missed {} beats", missed - prev_missed); 
                    prev_missed = missed;
                }
            },
            recv(http_serv.channel) -> msg => {
                match msg {
                    Ok(req) => server.handle(req),
                    Err(e) => panic!("Server error: {:?}", e),
                };
            }
        }
    }
}
