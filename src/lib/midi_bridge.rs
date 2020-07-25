
use arr_macro::arr;
use rand::Rng;
use std::collections::{BTreeMap, VecDeque};
use std::mem;
use std::rc::Rc;
use crate::config::{FungedConfig};
use crate::befunge::{EventLog, Note};
use crate::jack::{JackHandle, MidiMsg};

pub enum FilterSpec {
    Basic,
    Solo,
    RandomArp(Rc<[u64]>),
}

impl FilterSpec {
    fn parse(input: &str) -> Result<Self, String> {
        let v : Vec<&str> = input.split(':').collect();

        if v.len() == 0 {
            return Err("Empty note_filter spec".to_string());
        }

        if v[0] == "solo" {
            if v.len() != 1 {
                return Err("solo does not take arguments".to_string());
            }
            return Ok(FilterSpec::Solo)
        }

        if v[0] == "random" {
            if v.len() == 1 {
                return Err("random needs at least one argument".to_string());
            }
            let mut durs = Vec::new();

            for s in v.iter().skip(1) {
                match s.parse::<u64>() {
                    Ok(d) => durs.push(d),
                    Err(e) => return Err(format!("Bad argument: {:?}", e))
                }
            }

            return Ok(FilterSpec::RandomArp(durs.into()))
        }

        Err(format!("Unrecognized note filter: {}", input))
    }

    fn to_filter(&self, channel: u8) -> Box<dyn Filter> {
        match self {
            FilterSpec::Basic => Box::new(Basic::new(channel)),
            FilterSpec::Solo => Box::new(Solo::new(channel)),
            FilterSpec::RandomArp(durs) =>
                Box::new(RandomArp::new(channel, durs.clone())),
        }
    }
}

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


// Random Arpeggiator - takes a slice of u64 as a cycle for durtions.
struct RandomArp {
    channel: u8,
    durations: Rc<[u64]>,
    next_dur: usize,
    next_change: Option<(u64, u8)>, // Change beat, current pch
    active: Vec<(u64, u8, u8)>, // endbeat, pch, vel
    current: Option<u64>,
}

impl RandomArp {
    fn new (channel: u8, durations: Rc<[u64]>) -> Self {
        RandomArp {
            channel: channel,
            durations: durations,
            next_dur: 0,
            next_change: None,
            active: Vec::new(),
            current: None
        }
    }
}

impl Filter for RandomArp {
    fn activate(&mut self, beat: u64, _handle: &JackHandle) {
        if self.current.is_some() {
            panic!("RandomArp::activate without resolve.")
        }
        self.current = Some(beat);
    }

    fn push(&mut self, note: &Note, _handle: &JackHandle) {
        let beat = self.current.expect("RandomArp::push without activate.");
        self.active.push((beat + note.dur as u64, note.pch, note.vel));
    }

    fn resolve(&mut self, handle: &JackHandle) -> bool {
        let beat = self.current.expect("RandomArp::resolve without activate.");
        self.current = None;

        let change = match self.next_change {
            None => true,
            Some((change_beat, pch)) => {
                if change_beat == beat {
                    handle.send_midi(MidiMsg::Off(self.channel, pch));
                    true
                } else {
                    false
                }
            }
        };

        if change {
            let dur = self.durations[self.next_dur];
            self.next_dur = (self.next_dur + 1) % self.durations.len();
            let active = mem::take(&mut self.active);
            for tup in active {
                if tup.0 <= beat { continue; }
                self.active.push(tup);
            }
            if self.active.len() == 0 { return false; }
            let mut rng = rand::thread_rng();
            let i = rng.gen_range(0, self.active.len());
            let (_, pch, vel) = self.active[i];
            handle.send_midi(MidiMsg::On(self.channel, pch, vel));
            self.next_change = Some((beat + dur, pch));
        }

        true
    }

}

pub struct MidiBridge<'a> {
    handle: &'a JackHandle,
    beat: u64,
    filter_specs: [FilterSpec; 256],
    filters: BTreeMap<u8, Box<dyn Filter>>
}

impl<'a> MidiBridge<'a> {
    pub fn new(conf: &FungedConfig, handle: &'a JackHandle) -> Self {
        let mut specs = arr![FilterSpec::Basic; 256];

        for ch in 0..=255 {
            let filt = conf.channels[ch as usize].as_ref()
                            .and_then(|cc| cc.note_filter.as_ref());
            let spec = match filt {
                None => continue,
                Some(s) => {
                    match FilterSpec::parse(s) {
                        Ok(spec) => spec,
                        Err(err) => panic!(err),
                    }
                }
            };

            specs[ch as usize] = spec;
        }

        MidiBridge {
            handle: handle,
            beat: 0,
            filter_specs: specs,
            filters: BTreeMap::new(),
        }
    }

    fn step_i(&mut self, beat: u64, log: &Vec<EventLog>) {
        let mut filters = mem::take(&mut self.filters);
        let handle = self.handle;

        for filt in filters.values_mut() {
            filt.activate(beat, handle);
        }

        for ev in log {
            let note = match ev {
                EventLog::Play(n) => n,
                _ => continue
            };
            if note.pch > 127 { continue }
            if note.dur < 1 { continue }

            let act = filters.entry(note.cha).or_insert_with(|| {
                    let mut f = self.filter_specs[note.cha as usize]
                                    .to_filter(note.cha);
                    f.activate(beat, handle);
                    f
                });
            act.push(note, self.handle);
        }

        let mut dead = Vec::new();
        for (ch, filter) in filters.iter_mut() {
            if !filter.resolve(handle) {
                dead.push(*ch);
            }
        }
        for d in dead {
            filters.remove(&d);
        }

        self.beat = beat;
        self.filters = filters;
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
