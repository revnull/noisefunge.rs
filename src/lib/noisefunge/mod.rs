
use std::collections::BTreeMap;
use crate::noisefunge::befunge::Process;

struct Engine<'a> {
    next_pid: u64,
    pids: BTreeMap<u64,Process<'a>>,
}

mod befunge;

impl<'a> Engine<'a> {
    pub fn new() -> Engine<'a> {
        Engine { next_pid : 1, pids : BTreeMap::new() }
    }

    fn make_process(&mut self, input: &'a str, output: &'a str) ->
                    &Process {
        let pid = self.next_pid;
        let proc = Process::new(pid, input, output);
        self.next_pid += 1;
        self.pids.insert(pid, proc);

        self.pids.get(&pid).unwrap()
    }
}
