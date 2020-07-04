
use noisefunge::jack::*;
use noisefunge::server::*;
use noisefunge::server::FungeRequest::*;
use noisefunge::befunge::*;
use noisefunge::config::*;
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
}

impl FungedServer {

    fn handle(&mut self, request: FungeRequest) {
        match request {
            StartProcess(prog, rspndr) =>
                rspndr.respond(match Prog::parse(&prog) {
                    Ok(p) => Ok(self.engine.make_process(p)),
                    Err(e) => Err(e.to_string())
                }),
            GetState(prev, rspndr) => {
                let bytes = Arc::new(to_vec(&self.engine.state()).unwrap());
                rspndr.respond(bytes);
            },
            r => panic!("Failed to handle: {:?}", r),
        };
    }

}

fn main() {

    let mut server = FungedServer {
        config: FungedConfig::read_config(&read_args()),
        engine: Engine::new()
    };

    let mut handle = JackHandle::new(&server.config);
    let http_serv = ServerHandle::new(&server.config);
    let mut prev_i = 0;

    loop {
        select! {
            recv(handle.beat_channel) -> msg => {
                let i = msg.expect("Failed to read from beat channel.");
                for j in prev_i..i {
                    if i % server.config.period == 0 {
                        let log = server.engine.step();
                        for n in log {
                            println!("log: {:?}", n);
                        };
                    }
                };
                prev_i = i;
            },
            recv(http_serv.channel) -> msg => {
                println!("Here: {:?}", msg);
                match msg {
                    Ok(req) => server.handle(req),
                    Err(e) => println!("Unknown error: {:?}", e),
                };
            }
        }
    }
}
