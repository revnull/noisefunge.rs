
use arr_macro::arr;
use std::collections::BTreeMap;
use std::mem;
use crate::config::{FungedConfig, ChannelConfig};
use crate::befunge::{EventLog, Note};
use crate::jack::{JackHandle, MidiMsg};

struct NoteFilter {
    channel: u8,
    active: [bool; 127],
    events: BTreeMap<u64, Vec<u8>>
}

struct ActiveFilter {
    beat: u64,
    filter: NoteFilter,
    beat_events: Vec<MidiMsg>
}

impl ActiveFilter {
    fn new(mut filter: NoteFilter, beat: u64) -> Self {
        let mut beat_events = Vec::new();

        match filter.events.remove(&beat) {
            None => {},
            Some(evs) => {
                for pch in evs {
                    beat_events.push(MidiMsg::Off(filter.channel, pch));
                    filter.active[pch as usize] = false;
                }
            }
        }

        ActiveFilter {
            beat: beat,
            filter: filter,
            beat_events: beat_events }
    }

    fn push(&mut self, note: &Note) {
        let i = note.pch as usize;
        if self.filter.active[i] { return; }

        self.filter.active[i] = true;
        self.beat_events.push(MidiMsg::On(note.cha, note.pch, note.vel));
        self.filter.events.entry(self.beat + note.dur as u64)
                          .or_insert_with(|| Vec::new())
                          .push(note.pch);
    }

    fn resolve(self, handle: &JackHandle) -> Option<NoteFilter> {

        for ev in &self.beat_events {
            handle.send_midi(*ev);
        }

        if self.filter.events.is_empty() {
            None
        } else {
            Some(self.filter)
        }
    }
}

impl NoteFilter {
    fn new(ch: u8) -> Self {
        NoteFilter { channel: ch,
                     active: arr![false; 127],
                     events: BTreeMap::new() }
    }

    fn activate(mut self, beat: u64) -> ActiveFilter {
        ActiveFilter::new(self, beat)
    }
}

pub struct MidiBridge<'a> {
    handle: &'a JackHandle,
    beat: u64,
    events: BTreeMap<u64, Vec<MidiMsg>>,
    filters: BTreeMap<u8, NoteFilter>
}

impl<'a> MidiBridge<'a> {
    pub fn new(conf: &FungedConfig, handle: &'a JackHandle) -> Self {
        MidiBridge {
            handle: handle,
            beat: 0,
            events: BTreeMap::new(),
            filters: BTreeMap::new(),
        }
    }

    fn step_i(&mut self, beat: u64, log: &Vec<EventLog>) {
        let filters = mem::take(&mut self.filters);
        let mut active = BTreeMap::new();
        for (ch, filt) in filters {
            active.insert(ch, filt.activate(beat));
        }

        for ev in log {
            let note = match ev {
                EventLog::Play(n) => n,
                _ => continue
            };
            if note.pch > 127 { continue }
            if note.dur < 1 { continue }

            let act = active.entry(note.cha).or_insert_with(||
                    NoteFilter::new(note.cha).activate(beat)
                );
            act.push(note);
        }

        for (ch, act) in active {
            match act.resolve(self.handle) {
                None => (),
                Some(filt) => { self.filters.insert(ch, filt); }
            }
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
