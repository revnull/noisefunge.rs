/*
    Noisefunge Copyright (C) 2021 Rev. Johnny Healey <rev.null@gmail.com>

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use arr_macro::arr;

use jack::*;
use log::*;
use std::cmp;
use std::collections::HashMap;
use std::fmt;
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
    Program(u8, Option<u16>, Option<u8>),
    Pan(u8, u8),
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

#[derive(Copy, Clone, Debug)]
pub enum JackError {
    UnknownChannel(u8),
    WriteFailed,
}

impl fmt::Display for JackError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            JackError::UnknownChannel(ch) =>
                write!(f, "unknown channel: {}", ch),
            JackError::WriteFailed =>
                write!(f, "write failed"),
        }
    }
}

pub struct JackHandle {
    pub beat_channel: Receiver<u64>,
    pub err_channel: Receiver<JackError>,
    missed_beats: Arc<AtomicU64>,
    note_channel: Sender<MidiMsg>,
    connect_handle: Option<ConnectHandle>,
    deactivate: Box<dyn FnOnce()>,
}

struct Handler {
    beat_channel: Sender<u64>,
    err_channel: Sender<JackError>,
    note_channel: Receiver<MidiMsg>,
    missed_beats: Arc<AtomicU64>,
    ports: PortMap,
    beats_in: Port<MidiIn>,
    beat: u64,
}

impl Handler {
    fn new(beat_channel: Sender<u64>, err_channel: Sender<JackError>,
           note_channel: Receiver<MidiMsg>, missed_beats: Arc<AtomicU64>,
           ports: PortMap, beats_in: Port<MidiIn>) -> Handler {

        Handler {
            beat_channel: beat_channel,
            err_channel: err_channel,
            note_channel: note_channel,
            missed_beats: missed_beats,
            ports: ports,
            beats_in: beats_in,
            beat: 0
        }
    }
}

unsafe impl Send for Handler {}

impl ProcessHandler for Handler {
    fn process(&mut self, _cl: &Client, ps: &ProcessScope) -> Control {
        let mut wtrs = self.ports.writers(ps);

        for bin in self.beats_in.iter(ps) {
            if bin.bytes[0] == 248 {
                let t = bin.time;
                self.beat += 1;
                match self.beat_channel.try_send(self.beat) {
                    Ok(_) => (),
                    Err(e) if e.is_full() => {
                        self.missed_beats.fetch_add(1, Ordering::Relaxed); },
                    _ => panic!("try_send failed: disconnected")
                }
                for msg in self.note_channel.try_iter() {
                    match msg {
                        MidiMsg::On(ch, pch, vel) => {
                            let (ch, wtr) = match wtrs.get_writer(ch) {
                                Some(tup) => tup,
                                None => {
                                    self.err_channel.try_send(
                                        JackError::UnknownChannel(ch))
                                        .expect("failed to write error");
                                    continue;
                                },
                            };
                            if wtr.write(
                                &jack::RawMidi {
                                    time: t,
                                    bytes: &[144 + ch, pch, vel]
                                }).is_err() {
                                    self.err_channel.try_send(
                                        JackError::WriteFailed)
                                        .expect("failed to write error.");
                            }
                        },
                        MidiMsg::Off(ch, pch) => {
                            let (ch, wtr) = match wtrs.get_writer(ch) {
                                Some(tup) => tup,
                                None => {
                                    self.err_channel.try_send(
                                        JackError::UnknownChannel(ch))
                                        .expect("failed to write error");
                                    continue;
                                },
                            };
                            if wtr.write(
                                &jack::RawMidi {
                                    time: t,
                                    bytes: &[128 + ch, pch, 0]
                                }).is_err() {
                                    self.err_channel.try_send(
                                        JackError::WriteFailed)
                                    .expect("failed to write error");
                            }
                        },
                        MidiMsg::Program(ch, bank, patch) => {
                            let (ch, wtr) = match wtrs.get_writer(ch) {
                                Some(tup) => tup,
                                None => {
                                    self.err_channel.try_send(
                                        JackError::UnknownChannel(ch))
                                        .expect("failed to write error");
                                    continue;
                                },
                            };
                            if let Some(bank) = bank {
                                if wtr.write(
                                    &jack::RawMidi {
                                        time: t,
                                        bytes: &[176 + ch, 0, (bank >> 7) as u8]
                                    }).is_err() {
                                    self.err_channel.try_send(
                                        JackError::WriteFailed)
                                    .expect("failed to write error");
                                }
                                if wtr.write(
                                    &jack::RawMidi {
                                        time: t,
                                        bytes: &[176 + ch, 32,
                                                 (bank & 127) as u8]
                                    }).is_err() {
                                    self.err_channel.try_send(
                                        JackError::WriteFailed)
                                    .expect("failed to write error");
                                }
                            }
                            if let Some(patch) = patch {
                                if wtr.write(
                                    &jack::RawMidi {
                                        time: t,
                                        bytes: &[192 + ch, patch]
                                    }).is_err() {
                                    self.err_channel.try_send(
                                        JackError::WriteFailed)
                                    .expect("failed to write error");
                                }
                            }
                        },
                        MidiMsg::Pan(ch, pan) => {
                            let (ch, wtr) = match wtrs.get_writer(ch) {
                                Some(tup) => tup,
                                None => {
                                    self.err_channel.try_send(
                                        JackError::UnknownChannel(ch))
                                        .expect("failed to write error");
                                    continue;
                                },
                            };
                            if wtr.write(
                                &jack::RawMidi {
                                    time: t,
                                    bytes: &[176 + ch, 10,
                                             (pan & 127) as u8]
                                }).is_err() {
                                self.err_channel.try_send(
                                    JackError::WriteFailed)
                                .expect("failed to write error");
                            }
                        },
                    }
                }
            }
        }
        Control::Continue
    }
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

        for name in &conf.locals {
            let port = client.register_port(name, MidiOut::default())
                             .expect("Failed to register port");
            locals2.insert(name.clone(), port.clone_unowned());
            locals.insert(name.clone(), port);
        }

        //let mut portmap = PortMap::new(&conf.channels, locals);

        let (snd1, rcv1) = bounded(128);
        let (snd2, rcv2) = bounded(4);
        let (snd3, rcv3) = bounded(128);

        let handler = Handler::new(snd2, snd3, rcv1, missed.clone(),
                                   PortMap::new(&conf.channels, locals),
                                   beats_in);
        let active = client.activate_async((),handler)
                           .expect("Failed to activate client.");

        let mut connections = Vec::new();

        for (src, dst) in &conf.connections {
            let src_name = &locals2.get(src).unwrap().name().unwrap();
            connections.push((String::from(src_name), String::from(dst)));
        }
        connections.push((conf.beat_source.to_string(), String::from(bi_name)));
        connections.extend(conf.extra_connections.clone());

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
            if let Some(pan) = cc.pan {
                instrs.push(MidiMsg::Pan(i as u8, pan));
            }
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
                     err_channel: rcv3,
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
