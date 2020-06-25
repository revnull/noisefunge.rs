
extern crate config;
extern crate clap;

use noisefunge::jack::*;
use std::{thread, time};
use std::collections::HashMap;

use clap::{Arg, App};

fn read_config<'a>(file: &'a str) -> config::Config {
    let mut settings = config::Config::default();

    settings.merge(config::File::with_name(file)).unwrap();

    settings
}

fn read_args() -> String {
    let matches = App::new("funged")
                          .arg(Arg::with_name("CONFIG")
                               .help("Config file to use")
                               .required(true))
                          .get_matches();
    String::from(matches.value_of("CONFIG").unwrap())
}

fn main() {

    let cfg = read_config(&read_args());
    println!("{:?}", cfg);

    let mut conf = PortConfig::new("jack_midi_clock");
    conf.connect("ports1", "Qsynth1");
    conf.connect("ports2", "Qsynth2");
    conf.connect("ports3", "Qsynth3");
    conf.connect("ports4", "Qsynth4");
    conf.connect("ports5", "Qsynth5");

    let mut handle = JackHandle::new(&conf);

    while true  {
        let i = handle.next_beat();
        println!("next_beat: {}", i);

        match i % 8 {
            0 => { handle.send_midi(MidiMsg::On(0, 70, 99));} ,
            5 => { handle.send_midi(MidiMsg::Off(0, 70)); } ,
            _ => {}
        }
    }
}
