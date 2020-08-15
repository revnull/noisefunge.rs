
use arr_macro::arr;

use jack::*;
use log::*;
use std::cmp;
use std::collections::HashMap;
use std::mem;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::thread::{JoinHandle};
use std::time::Duration;
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

pub struct ConnectHandle {
    done: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>
}

impl ConnectHandle {
    pub fn new(done: Arc<AtomicBool>, handle: JoinHandle<()>) -> Self {
        ConnectHandle {
            done: done,
            handle: Some(handle),
        }
    }

    pub fn join(&mut self) {
        if self.handle.is_none() { return }
        if self.done.load(Ordering::Relaxed) {
            let handle = self.handle.take().unwrap();
            debug!("Joining ConnectHandle");
            match handle.join() {
                Ok(_) => { },
                Err(e) => { error!("Connect thread panicked: {:?}", e) },
            }
            self.handle = None;
            debug!("Joined ConnectHandle");
        }
    }
}

pub struct JackHandle {
    pub beat_channel: Receiver<u64>,
    missed_beats: Arc<AtomicU64>,
    note_channel: Sender<MidiMsg>,
    connect_handle: Option<ConnectHandle>,
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

        let mut connections = Vec::new();

        for (src, dst) in &conf.connections {
            let src_name = &locals2.get(src).unwrap().name().unwrap();
            connections.push((String::from(src_name), String::from(dst)));
        }
        connections.push((conf.beat_source.to_string(), String::from(bi_name)));

        let instr_snd = snd1.clone();
        let mut instrs = Vec::new();
        for i in 0..=255 {
            let cc = match &conf.channels[i] {
                Some(cc) => cc,
                None => continue
            };
            if cc.bank.is_some() || cc.program.is_some() {
                instrs.push(MidiMsg::Program(i as u8, cc.bank, cc.program));
            };
        }

        let connect_done = Arc::new(AtomicBool::new(false));
        let connect_done2 = connect_done.clone();
        let connect_thread = thread::spawn(move || {
            let (connector, _status) =
                jack::Client::new("connect",ClientOptions::NO_START_SERVER)
                             .expect("Failed to start jack client.");
            let mut connections = connections;
            let mut sleep_dur = Duration::from_millis(250);
            let mut attempts = 4;
            while !connections.is_empty() {
                let mut temp_connections = mem::take(&mut connections);
                let mut new_conn = false;

                for (src, dst) in temp_connections.drain(..) {
                    match connector.connect_ports_by_name(&src, &dst) {
                        Ok(_) => {
                            info!("Connected: {} -> {}", src, dst);
                            new_conn = true;
                        },
                        Err(e) => {
                            warn!("Failed to connect: {} -> {}: {}",
                                  src, dst, e);
                            connections.push((src, dst));
                        }
                    }
                }

                if new_conn {
                    for m in &instrs {
                        instr_snd.send(*m).expect("Sender::send failed");
                    }
                }

                thread::sleep(sleep_dur);
                attempts -= 1;
                if attempts == 0 {
                    attempts = 4;
                    sleep_dur = cmp::min(2 * sleep_dur, Duration::from_secs(6));
                }
            }
            debug!("All connections established.");
            connect_done2.store(true, Ordering::Relaxed);
        });

        let deact = Box::new(|| { active.deactivate().unwrap(); });

        JackHandle { beat_channel: rcv2,
                     missed_beats: missed,
                     note_channel: snd1,
                     connect_handle: Some(
                        ConnectHandle::new(connect_done, connect_thread)),
                     deactivate: deact }
    }

    pub fn missed(&self) -> u64 {
        self.missed_beats.load(Ordering::Relaxed)
    }

    pub fn send_midi(&self, msg: MidiMsg) -> bool {
        self.note_channel.try_send(msg).is_ok()
    }

    pub fn take_connect_handle(&mut self) -> ConnectHandle {
        self.connect_handle.take()
                           .expect("take_connect_handle already called.")
    }

    pub fn shutdown(self) {
        (self.deactivate)();
    }

}
