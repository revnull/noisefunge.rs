
use jack::*;
use std::collections::HashSet;
use std::collections::HashMap;

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

impl<'a> JackHandle {
    pub fn new(conf : &PortConfig) -> JackHandle {
        let (client, status) =
            jack::Client::new("noisefunge",
                              ClientOptions::NO_START_SERVER)
                             .expect("Failed to start jack client.");

        let beats_in = client.register_port("beats_in", MidiIn::default())
                             .unwrap();
        let mut locals = HashMap::new();

        for name in &conf.locals {
            locals.insert(name.clone(),
                          client.register_port(name, MidiOut::default())
                          .expect("Failed to register port"));
        }

        let handler = MidiHandler { };
        let active = client.activate_async((), handler)
                           .expect("Failed to activate client.");
        let client = active.as_client();
        for (src, dst) in &conf.connections {
            println!("{} -> {}", src, dst);
            let src_name = &locals.get(src).unwrap().name().unwrap();
            for name in client.ports(Some(dst), None, PortFlags::IS_INPUT) {
                println!("{} -> {}: {:?}", src_name, name,
                         client.connect_ports_by_name(src_name, &name));
            }
        }

        for name in client.ports(Some(&conf.beat_source), None,
                                 PortFlags::IS_OUTPUT) {
            let bi_name = &beats_in.name().unwrap();
            println!("{} -> {}: {:?}", &name, bi_name,
                     client.connect_ports_by_name(&name, bi_name));
        }
        JackHandle { client : active }
    }
}
