
extern crate clap;
extern crate config;
#[macro_use]
extern crate crossbeam_channel;

use noisefunge::jack::*;
use noisefunge::server::*;
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

                match i % 8 {
                    0 => { handle.send_midi(MidiMsg::On(0, 70, 99));} ,
                    5 => { handle.send_midi(MidiMsg::Off(0, 70)); } ,
                    _ => {}
                };
            },
            recv(serv.channel) -> msg => {
                println!("Here: {:?}", msg);
                match msg {
                    Ok(FungeRequest::StartProcess(inp, outp, prog, rspndr)) => {
                        rspndr.respond(match Prog::parse(&prog) {
                            Ok(p) => Ok(eng.make_process(&inp, &outp, p)),
                            Err(e) => Err(e.to_string())
                        });
                    },
                    Err(e) => println!("Error: {}", e),
                    s => println!("Unparsed: {:?}", s),
                };
            }
        }
    }
}
