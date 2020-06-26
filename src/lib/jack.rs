
use jack::*;
use std::collections::HashSet;
use std::collections::HashMap;
use crossbeam_channel::{bounded, Sender, Receiver};

use crate::config::{FungedConfig};

#[derive(Copy, Clone, Debug)]
pub enum MidiMsg {
    On(u8, u8, u8),
    Off(u8, u8),
}

unsafe impl Send for MidiMsg {}

pub struct JackHandle {
    beat_channel: Receiver<u64>,
    note_channel: Sender<MidiMsg>,
    deactivate: Box<FnOnce()>
}

impl JackHandle {
    pub fn new(conf : &FungedConfig) -> JackHandle {
        let (client, status) =
            jack::Client::new("noisefunge",
                              ClientOptions::NO_START_SERVER)
                             .expect("Failed to start jack client.");

        let beats_in = client.register_port("beats_in", MidiIn::default())
                             .unwrap();
        let bi_name = &beats_in.name().unwrap();
        let mut locals = HashMap::new();

        for name in &conf.locals {
            locals.insert(name.clone(),
                          client.register_port(name, MidiOut::default())
                          .expect("Failed to register port"));
        }

        let (snd1, rcv1) = bounded(128);
        let (snd2, rcv2) = bounded(1);

        let mut px = client.register_port("outoutout", MidiOut::default())
                           .expect("foo");
        let handler = {
            let r1 = rcv1;
            let mut i :u64 = 0;
            ClosureProcessHandler::new(
                move |cl: &Client, ps: &ProcessScope| -> Control {
                    for bin in beats_in.iter(ps) {
                        if bin.bytes[0] == 248 {
                            let t = bin.time;
                            i += 1;
                            snd2.try_send(i);
                            let mut wtr = px.writer(ps);
                            for msg in r1.try_iter() {
                                match msg {
                                    MidiMsg::On(ch, pch, vel) => {
                                        wtr.write(&jack::RawMidi {
                                            time: t,
                                            bytes: &[
                                                144 + ch, pch, vel
                                            ] });
                                    },
                                    MidiMsg::Off(ch, pch) => {
                                        wtr.write(&jack::RawMidi {
                                            time: t,
                                            bytes: &[
                                                144 + ch, pch, 0
                                            ] });
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
            println!("{} -> {}", src, dst);
            let src_name = &locals.get(src).unwrap().name().unwrap();
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

        JackHandle { beat_channel: rcv2,
                     note_channel: snd1,
                     deactivate: deact }
    }

    pub fn next_beat(&self) -> u64 {
        self.beat_channel.recv().expect("Failed to receive from jack thread.")
    }

    pub fn send_midi(&self, msg: MidiMsg) -> bool {
        self.note_channel.try_send(msg).is_ok()
    }

}
