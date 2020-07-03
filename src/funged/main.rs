
use noisefunge::jack::*;
use noisefunge::server::*;
use noisefunge::server::FungeRequest::*;
use noisefunge::befunge::*;
use noisefunge::config::*;
use std::{thread, time};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crossbeam_channel::select;

use clap::{Arg, App};

fn read_args() -> String {
    let matches = App::new("funged")
                          .arg(Arg::with_name("CONFIG")
                               .help("Config file to use")
                               .required(true))
                          .get_matches();
    String::from(matches.value_of("CONFIG").unwrap())
}

fn handle_server_request(engine: &mut Engine, request: FungeRequest) {
    match request {
        StartProcess(prog, rspndr) =>
            rspndr.respond(match Prog::parse(&prog) {
                Ok(p) => Ok(engine.make_process(p)),
                Err(e) => Err(e.to_string())
            }),
        r => panic!("Failed to handle: {:?}", r),
    };
}

fn main() {

    let conf = FungedConfig::read_config(&read_args());

    let mut handle = JackHandle::new(&conf);

    let serv = ServerHandle::new(&conf);
    let mut eng = Engine::new();

    loop {
        select! {
            recv(handle.beat_channel) -> msg => {
                let i = msg.expect("Failed to read from beat channel.");
                println!("next_beat: {}", i);
                let notes = eng.step();
                for n in notes {
                    println!("log: {:?}", n);
                };
                match i % 8 {
                    0 => { handle.send_midi(MidiMsg::On(0, 70, 99));} ,
                    5 => { handle.send_midi(MidiMsg::Off(0, 70)); } ,
                    _ => {}
                };
            },
            recv(serv.channel) -> msg => {
                println!("Here: {:?}", msg);
                match msg {
                    Ok(req) => handle_server_request(&mut eng, req),
                    Err(e) => println!("Unknown error: {:?}", e),
                };
            }
        }
    }
}
