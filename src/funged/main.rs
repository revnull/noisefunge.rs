
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
        thread::sleep(time::Duration::from_secs(1));
    }
}
