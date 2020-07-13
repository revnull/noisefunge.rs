
use std::collections::BTreeMap;
use crate::config::{FungedConfig, ChannelConfig};
use crate::befunge::EventLog;
use crate::jack::{JackHandle, MidiMsg};

pub struct MidiBridge<'a> {
    handle: &'a JackHandle,
    beat: u64,
    events: BTreeMap<u64, Vec<MidiMsg>>,
}

impl<'a> MidiBridge<'a> {
    pub fn new(conf: &FungedConfig, handle: &'a JackHandle) -> Self {
        MidiBridge {
            handle: handle,
            beat: 0,
            events: BTreeMap::new()
        }
    }

    pub fn step(&mut self, beat: u64, log: &Vec<EventLog>) {
        if beat < self.beat {
            panic!("Beat went back in time!!!");
        }
        let mut dead = Vec::new();
        for (k,evs) in self.events.range(self.beat..=beat) {
            dead.push(*k);
            for ev in evs {
                self.handle.send_midi(*ev);
                println!("Event: {:?}", ev);
            }
        }
        for d in dead {
            self.events.remove(&d);
        }
        for ev in log {
            let note = match ev {
                EventLog::Play(n) => n,
                _ => continue,
            };
            self.handle.send_midi(MidiMsg::On(note.cha, note.pch, note.vel));
            self.events.entry(beat + note.dur as u64)
                       .or_insert_with(|| Vec::new())
                       .push(MidiMsg::Off(note.cha, note.pch));
            println!("Note: {:?}", note);
        }
    }
}
