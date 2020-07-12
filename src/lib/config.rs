
use arr_macro::arr;
use std::collections::{HashSet, HashMap};
use std::rc::Rc;

pub struct ChannelConfig {
    pub local: Rc<str>,
    pub starting: u8,
    pub bank: Option<u8>,
    pub program: Option<u8>
}

pub struct FungedConfig {
    pub host: String,
    pub port: u16,
    pub beat_source: Rc<str>,
    pub period: u64,
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

        settings.set_default("host", "127.0.0.1").unwrap();
        settings.set_default("port", 1312).unwrap();
        settings.set_default("period", 24).unwrap();

        settings.merge(config::File::with_name(&file)).unwrap();
        println!("{:?}", settings);
        let host = settings.get_str("host").unwrap();
        let port = settings.get_int("port").expect("Port not set") as u16;
        let bi = settings.get_str("beats_in").expect("Beats in not found.");
        let period = settings.get_int("period").unwrap();

        let mut locals = HashSet::new();
        let mut channels = arr![None; 256];
        let mut connections = Vec::new();

        for t in settings.get_table("out").unwrap_or(HashMap::new()) {
            let (local, block) = t;
            let local = Rc::from(local);
            let table = block.into_table()
                             .expect(&format!("Could not parse section: {}",
                                              local));
            connections.extend_from_slice(&get_connections(&local, &table));
            match table.get("starting").and_then(
                |v| v.clone().into_int().ok()) {

                Some(ch) => {
                    let end = table.get("ending")
                                   .and_then(|v| v.clone().into_int().ok())
                                   .unwrap_or(ch + 15);
                    for i in ch..=end {
                        channels[i as usize] = Some(
                            ChannelConfig { local: Rc::clone(&local),
                                            starting: ch as u8,
                                            bank: None,
                                            program: None });
                    }
                }
                _ => { println!("No starting channel for {}", &local); }
            };
            locals.insert(local);
        }

        for t in settings.get_table("channel").unwrap_or(HashMap::new()) {
            let (name, block) = t;

            let i = name.parse::<u8>().expect(
                &format!("channel.{} is invalid. must be int.", name));
            let ch = channels[i as usize].as_mut().expect(
                &format!("channel.{} has no output port", name));
            let table = block.into_table().expect(
                &format!("Could not parse [channel.{}]", name));
            table.get("bank").and_then(|v| v.clone().into_int().ok())
                             .map(|b| ch.bank = Some(b as u8));
            table.get("program").and_then(|v| v.clone().into_int().ok())
                                .map(|b| ch.program = Some(b as u8));
        }

        FungedConfig { host: host,
                       port: port,
                       beat_source: Rc::from(bi),
                       period: period as u64,
                       locals: locals,
                       connections: connections,
                       channels: channels }
    }
}
