/*
    Noisefunge Copyright (C) 2021 Rev. Johnny Healey <rev.null@gmail.com>

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

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

    let mut subs = SubprocessHandle::new(server.config.subprocesses.clone());

    let mut handle = JackHandle::new(&server.config);
    let mut connect_handle = handle.take_connect_handle();
    let mut prev_missed = 0;
    let mut bridge = MidiBridge::new(&server.config, &handle);
    let http_serv = ServerHandle::new(&server.config);
    let mut prev_i = 0;

    loop {
        let mut attempt_cleanup = false;
        select! {
            recv(handle.beat_channel) -> msg => {
                let i = msg.expect("Failed to read from beat channel.");
                for j in prev_i..i {
                    if j % server.config.period == 0 {
                        let (beat, log) = server.engine.step();
                        bridge.step(beat, &log);
                    }
                    if i % 100 == 0 {
                        attempt_cleanup = true;
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
            recv(handle.err_channel) -> msg => {
                let msg = msg.expect("Failed to read from error channel.");
                error!("Error from jack thread: {:?}", msg);
            }
            recv(http_serv.channel) -> msg => {
                match msg {
                    Ok(req) => server.handle(req),
                    Err(e) => panic!("Server error: {:?}", e),
                };
            }
        }
        if attempt_cleanup {
            connect_handle.join();
            subs.check_children();
        }
    }
}
