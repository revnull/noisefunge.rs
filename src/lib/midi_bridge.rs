
use arr_macro::arr;
use std::collections::BTreeMap;
use std::mem;
use crate::config::{FungedConfig};
use crate::befunge::{EventLog, Note};
use crate::jack::{JackHandle, MidiMsg};

struct Basic {
    channel: u8,
    active: [bool; 127],
    off_events: BTreeMap<u64, Vec<u8>>,
    current: Option<u64>
}

impl Basic {
    fn new(channel: u8) -> Self {
        Basic {
            channel: channel,
            active: arr![false; 127],
            off_events: BTreeMap::new(),
            current: None
        }
    }
}

pub trait Filter {
    fn activate(&mut self, beat: u64, handle: &JackHandle);
    fn push(&mut self, note: &Note, handle: &JackHandle);
    fn resolve(&mut self, handle: &JackHandle) -> bool;
}

impl Filter for Basic {
    fn activate(&mut self, beat: u64, handle: &JackHandle) {
        if self.current.is_some() {
            panic!("Basic::activate called twice");
        }
        self.current = Some(beat);

        if let Some(evs) = self.off_events.remove(&beat) {
            for pch in evs {
                self.active[pch as usize] = false;
                handle.send_midi(MidiMsg::Off(self.channel, pch));
            }
        }
    }

    fn push(&mut self, note: &Note, handle: &JackHandle) {
        let beat = self.current.expect("Basic::push without activate");
        let i = note.pch as usize;
        if self.active[i] { return }
        self.active[i] = true;

        self.off_events.entry(beat + note.dur as u64)
                       .or_insert_with(|| Vec::new())
                       .push(note.pch);

        handle.send_midi(MidiMsg::On(note.cha, note.pch, note.vel));
    }

    fn resolve(&mut self, _handle: &JackHandle) -> bool {
        self.current = None;
        !self.off_events.is_empty()
    }
}

pub struct MidiBridge<'a> {
    handle: &'a JackHandle,
    beat: u64,
    filters: BTreeMap<u8, Box<dyn Filter>>
}

impl<'a> MidiBridge<'a> {
    pub fn new(_conf: &FungedConfig, handle: &'a JackHandle) -> Self {
        MidiBridge {
            handle: handle,
            beat: 0,
            filters: BTreeMap::new(),
        }
    }

    fn step_i(&mut self, beat: u64, log: &Vec<EventLog>) {
        //let mut filters = mem::take(&mut self.filters);
        let handle = self.handle;

        for filt in self.filters.values_mut() {
            filt.activate(beat, handle);
        }

        for ev in log {
            let note = match ev {
                EventLog::Play(n) => n,
                _ => continue
            };
            if note.pch > 127 { continue }
            if note.dur < 1 { continue }

            let act = self.filters.entry(note.cha).or_insert_with(|| {
                   let mut f = Basic::new(note.cha);
                   f.activate(beat, handle);
                   Box::new(f)
                });
            act.push(note, self.handle);
        }

        let mut dead = Vec::new();
        for (ch, filter) in self.filters.iter_mut() {
            if !filter.resolve(handle) {
                dead.push(*ch);
            }
        }
        for d in dead {
            self.filters.remove(&d);
        }

        self.beat = beat;
    }

    pub fn step(&mut self, beat: u64, log: &Vec<EventLog>) {
        if beat < self.beat {
            panic!("Beat went back in time!!!");
        }
        if beat - self.beat > 1 {
            eprintln!("Warning! Stepped {} beats", beat - self.beat);
            let empty = Vec::new();
            for i in self.beat..beat {
                self.step_i(i, &empty);
            }
        }
        self.step_i(beat, &log);

    }
}
