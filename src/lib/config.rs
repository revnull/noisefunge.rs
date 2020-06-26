
extern crate config;

use arr_macro::arr;
use std::collections::{HashSet, HashMap};
use std::net::IpAddr;
use std::rc::Rc;

pub struct ChannelConfig {
}

pub struct FungedConfig {
    pub ip: IpAddr,
    pub port: u16,
    pub beat_source: Rc<str>,
    pub locals: HashSet<Rc<str>>,
    pub connections: Vec<(Rc<str>, String)>,
    pub channels: [Option<ChannelConfig>; 256]
}

fn get_connections(local: &Rc<str>, table: &HashMap<String, config::Value>)
                  -> Vec<(Rc<str>,String)> {
    let mut result = Vec::new();

    let val = match table.get("connect") {
        None => { return result }
        Some(v) => v.clone()
    };

    match val.clone().into_str() {
        Ok(s) => {
            result.push((Rc::clone(local), s));
            return result;
        }
        _ => {}
    };

    match val.try_into() {
        Ok(a) => {
            let a : Vec<String> = a;
            for s in a {
                result.push((Rc::clone(local), s.clone()));
            }
        },
        _ => panic!("\"connect\" is not string or array of strings")
    }

    result
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
            let table = block.into_table()
                             .expect(&format!("Could not parse section: {}",
                                              local));
            connections.extend_from_slice(&get_connections(&local, &table));
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
