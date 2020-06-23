
use jack::*;
use std::collections::HashSet;

#[derive(Clone)]
pub struct PortConfig {
    beat_source: String,
    locals: HashSet<String>,
    connections: Vec<(String, String)>
}

impl PortConfig {
    pub fn new(beat_source: &str) -> Self {
        PortConfig { beat_source : String::from(beat_source),
                     locals : HashSet::new(),
                     connections : Vec::new() }
    }

    pub fn connect(&mut self, local: &str, remote: &str) {
        self.locals.insert(String::from(local));
        self.connections.push((String::from(local), String::from(remote)));
    }
}

struct MidiHandler {

}

impl ProcessHandler for MidiHandler {
    fn process(&mut self, client: &Client, scope: &ProcessScope) -> Control {
        Control::Continue
    }
}

pub struct JackHandle {
    client:AsyncClient<(), MidiHandler>,
}

impl JackHandle {
    pub fn new(conf : &PortConfig) -> JackHandle {
        let (client, status) =
            jack::Client::new("noisefunge",
                              ClientOptions::NO_START_SERVER)
                             .expect("Failed to start jack client.");

        let beats_in = client.register_port("beats_in", MidiIn::default());
        let mut locals = Vec::new();

        for name in &conf.locals {
            locals.push(client.register_port(name, MidiOut::default()));
        }

        let handler = MidiHandler { };
        let active = client.activate_async((), handler)
                           .expect("Failed to activate client.");
        JackHandle { client : active }
    }
}
