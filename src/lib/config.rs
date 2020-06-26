
extern crate config;

use arr_macro::arr;
use std::collections::{HashSet, HashMap};
use std::net::IpAddr;
use std::rc::Rc;

pub struct ChannelConfig {
}

pub struct FungedConfig {
    ip: IpAddr,
    port: u16,
    beat_source: Rc<str>,
    locals: HashSet<Rc<str>>,
    connections: Vec<(Rc<str>,Rc<str>)>,
    channels: [Option<ChannelConfig>; 256]
}

impl FungedConfig {
    pub fn read_config(file: &str) -> FungedConfig {
        let mut settings = config::Config::default();

        settings.set_default("ip", "0.0.0.0").unwrap();
        settings.set_default("port", 1312).unwrap();

        settings.merge(config::File::with_name(&file)).unwrap();
        println!("{:?}", settings);
        let ip = settings.get_str("ip").expect("IP Address not set")
                                       .parse().expect("Could not parse IP");
        let port = settings.get_int("port").expect("Port not set") as u16;
        let bi = settings.get_str("beats_in").expect("Beats in not found.");

        let mut locals = HashSet::new();
        let mut channels = arr![None; 256];
        let mut connections = Vec::new();

        for t in settings.get_table("ports").unwrap_or(HashMap::new()) {
            let (local, block) = t;
            let local = Rc::from(local);
            let local2 = Rc::clone(&local);
            let table = block.into_table()
                             .expect(&format!("Could not parse section: {}",
                                              local));
            locals.insert(local);
        }

        FungedConfig { ip: ip,
                       port: port,
                       beat_source: Rc::from(bi),
                       locals: locals,
                       connections: connections,
                       channels: channels }
    }
}
