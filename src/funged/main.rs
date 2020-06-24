
use noisefunge::jack::*;
use std::{thread, time};

fn main() {

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
