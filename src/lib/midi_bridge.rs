
use arr_macro::arr;
use std::collections::{BTreeMap, VecDeque};
use std::mem;
use crate::config::{FungedConfig};
use crate::befunge::{EventLog, Note};
use crate::jack::{JackHandle, MidiMsg};

pub trait Filter {
    fn activate(&mut self, beat: u64, handle: &JackHandle);
    fn push(&mut self, note: &Note, handle: &JackHandle);
    fn resolve(&mut self, handle: &JackHandle) -> bool;
}

// Basic - prevents a note from playing if it is already playing.
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

// Solo - Only let one note play at once. Most recent note supercedes existing
// notes, but will fall back to longer-held notes.
struct Solo {
    channel: u8,
    active: VecDeque<(u64, u8, u8)>, // until_beat, pch, vel
    playing: Option<u8>,
    current: Option<u64>,
}

impl Solo {
    fn new(channel: u8) -> Self {
        Solo {
            channel: channel,
            active: VecDeque::new(),
            playing: None,
            current: None
        }
    }
}

impl Filter for Solo {

    fn activate(&mut self, beat: u64, _handle: &JackHandle) {
        if self.current.is_some() {
            panic!("Solo::activate on unresolved filter");
        }
        self.current = Some(beat);

        loop {
            match self.active.front() {
                None => break,
                Some((end, _, _)) => {
                    if *end > beat { break }
                }
            }
            self.active.pop_front();
        }

        loop {
            match self.active.back() {
                None => break,
                Some((end, _, _)) => {
                    if *end > beat { break }
                }
            }
            self.active.pop_back();
        }
    }

    fn push(&mut self, note: &Note, _handle: &JackHandle) {
        let beat = self.current.expect("Solo::push without activate");

        self.active.push_back((beat + note.dur as u64, note.pch, note.vel));
    }

    fn resolve(&mut self, handle: &JackHandle) -> bool {
        if self.current.is_none() {
            panic!("Solo::resolve without activate");
        }
        self.current = None;
        if self.active.is_empty() {
            if let Some(oldpch) = self.playing {
                handle.send_midi(MidiMsg::Off(self.channel, oldpch));
                self.playing = None;
            }
            return false
        }

        let (_, pch, vel) = self.active.back().unwrap();
        let pch = *pch;
        let vel = *vel;

        match self.playing {
            None => {
                handle.send_midi(MidiMsg::On(self.channel, pch, vel));
            },
            Some(oldpch) => {
                if oldpch != pch {
                    handle.send_midi(MidiMsg::Off(self.channel, oldpch));
                    handle.send_midi(MidiMsg::On(self.channel, pch, vel));
                }
            }
        }

        self.playing = Some(pch);
        true
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
