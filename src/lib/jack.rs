
use arr_macro::arr;

use jack::*;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use crossbeam_channel::{bounded, Sender, Receiver};

use crate::config::{FungedConfig, ChannelConfig};

#[derive(Copy, Clone, Debug)]
pub enum MidiMsg {
    On(u8, u8, u8),
    Off(u8, u8),
    Program(u8, Option<u8>, Option<u8>),
}

unsafe impl Send for MidiMsg {}

#[derive(Copy, Clone)]
struct FakeWriter(u64);

struct PortMap {
    ports: Box<[Port<MidiOut>]>,
    mapping: [Option<(u8, usize)>; 256]
}

struct Writers<'a> {
    mapping: &'a[Option<(u8, usize)>; 256],
    writers: [Option<MidiWriter<'a>>; 16],
}

impl<'a> Writers<'a> {
    fn get_writer(&mut self, ch: u8) -> Option<(u8, &mut MidiWriter<'a>)> {
        let (st, i) = self.mapping[ch as usize]?;
        self.writers[i].as_mut().map( |w| (ch - st, w) )
    }
}

impl PortMap {
    fn new(channels: &[Option<ChannelConfig>; 256],
           ports: HashMap<Rc<str>, Port<MidiOut>>) -> Self {
        let mut name_map = HashMap::new();
        let mut port_vec = Vec::new();
        for (k,v) in ports {
            name_map.insert(k, port_vec.len());
            port_vec.push(v);
        }
        let mut mapping = arr![None; 256];
        for i in 0..=255 {
            let cc = match &channels[i] {
                None => continue,
                Some(cc) => cc
            };
            let vec_index = name_map.get(&cc.local).unwrap();
            mapping[i] = Some((cc.starting, *vec_index));
        }
        PortMap {
            ports: port_vec.into_boxed_slice(),
            mapping: mapping,
        }
    }

}

impl<'a> PortMap {
    fn writers(&'a mut self, ps: &'a ProcessScope) -> Writers<'a> {
        let mut writers = arr![None; 16];
        for (i, port) in self.ports.iter_mut().enumerate() {
            if i > 15 { panic!("Too many ports.") };
            writers[i] = Some(port.writer(ps));
        }
        Writers {
            mapping: &self.mapping,
            writers: writers,
        }
    }
}

pub struct JackHandle {
    pub beat_channel: Receiver<u64>,
    missed_beats: Arc<AtomicU64>,
    note_channel: Sender<MidiMsg>,
    deactivate: Box<dyn FnOnce()>,
}

impl JackHandle {
    pub fn new(conf : &FungedConfig) -> JackHandle {
        let (client, _status) =
            jack::Client::new("noisefunge",
                              ClientOptions::NO_START_SERVER)
                             .expect("Failed to start jack client.");

        let beats_in = client.register_port("beats_in", MidiIn::default())
                             .unwrap();
        let bi_name = &beats_in.name().unwrap();
        let mut locals = HashMap::new();
        let mut locals2 = HashMap::new();
        let missed = Arc::new(AtomicU64::new(0));
        let missed2 = missed.clone();

        for name in &conf.locals {
            let port = client.register_port(name, MidiOut::default())
                             .expect("Failed to register port");
            locals2.insert(name.clone(), port.clone_unowned());
            locals.insert(name.clone(), port);
        }

        let mut portmap = PortMap::new(&conf.channels, locals);

        let (snd1, rcv1) = bounded(128);
        let (snd2, rcv2) = bounded(1);

        let handler = {
            let r1 = rcv1;
            let mut i :u64 = 0;
            ClosureProcessHandler::new(
                move |_cl: &Client, ps: &ProcessScope| -> Control {
                    let mut wtrs = portmap.writers(ps);
                    for bin in beats_in.iter(ps) {
                        if bin.bytes[0] == 248 {
                            let t = bin.time;
                            i += 1;
                            match snd2.try_send(i) {
                                Ok(_) => (),
                                Err(e) if e.is_full() => { missed2.fetch_add(1, Ordering::Relaxed); },
                                _ => panic!("try_send failed due to disconnect.")
                            }
                            for msg in r1.try_iter() {
                                match msg {
                                    MidiMsg::On(ch, pch, vel) => {
                                        let (ch, wtr) = wtrs.get_writer(ch)
                                                            .unwrap();
                                        wtr.write(&jack::RawMidi {
                                            time: t,
                                            bytes: &[
                                                144 + ch, pch, vel
                                            ] }).expect("write failed");
                                    },
                                    MidiMsg::Off(ch, pch) => {
                                        let (ch, wtr) = wtrs.get_writer(ch)
                                                            .unwrap();
                                        wtr.write(&jack::RawMidi {
                                            time: t,
                                            bytes: &[
                                                144 + ch, pch, 0
                                            ] }).expect("write failed");
                                    }
                                    MidiMsg::Program(ch, bank, patch) => {
                                        let (ch, wtr) = wtrs.get_writer(ch)
                                                            .unwrap();
                                        if let Some(bank) = bank {
                                            wtr.write(&jack::RawMidi {
                                                time: t,
                                                bytes: &[
                                                    176 + ch, 00, bank
                                                ] }).expect("write failed");
                                        }
                                        if let Some(patch) = patch {
                                            wtr.write(&jack::RawMidi {
                                                time: t,
                                                bytes: &[
                                                    192 + ch, patch
                                                ] }).expect("write failed");
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Control::Continue
                })
            };

        let active = client.activate_async((),handler)
                           .expect("Failed to activate client.");
        let client = active.as_client();
        for (src, dst) in &conf.connections {
            let src_name = &locals2.get(src).unwrap().name().unwrap();
            for name in client.ports(Some(dst), None, PortFlags::IS_INPUT) {
                println!("{} -> {}: {:?}", src_name, name,
                         client.connect_ports_by_name(src_name, &name));
            }
        }

        for name in client.ports(Some(&conf.beat_source), None,
                                 PortFlags::IS_OUTPUT) {
            println!("{} -> {}: {:?}", &name, bi_name,
                     client.connect_ports_by_name(&name, bi_name));
        }

        let deact = Box::new(|| { active.deactivate().unwrap(); });

        for i in 0..=255 {
            let cc = match &conf.channels[i] {
                Some(cc) => cc,
                None => continue
            };
            if cc.bank.is_some() || cc.program.is_some() {
                snd1.send(MidiMsg::Program(i as u8, cc.bank, cc.program))
                    .expect("Sender::send failed");
            }
        }

        JackHandle { beat_channel: rcv2,
                     missed_beats: missed,
                     note_channel: snd1,
                     deactivate: deact }
    }

    pub fn missed(&self) -> u64 {
        self.missed_beats.load(Ordering::Relaxed)
    }

    pub fn send_midi(&self, msg: MidiMsg) -> bool {
        self.note_channel.try_send(msg).is_ok()
    }

    pub fn shutdown(self) {
        (self.deactivate)();
    }

}
