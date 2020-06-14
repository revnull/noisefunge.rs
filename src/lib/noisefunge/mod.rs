
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::rc::Rc;
use crate::noisefunge::befunge::{Process, Prog};

mod befunge;

enum MessageQueue {
    Empty { },
    ReadBlocked { queue : Vec<u64> },
    WriteBlocked { queue : Vec<(u8, u64)> }
}

struct Engine {
    next_pid: u64,
    pids: BTreeMap<u64,Process>,
    buffers: BTreeMap<String, MessageQueue>,
    active: BTreeSet<u64>
}

impl Engine {
    pub fn new() -> Engine {
        Engine { next_pid : 1,
                 pids : BTreeMap::new() ,
                 buffers : BTreeMap::new(),
                 active : BTreeSet::new() }
    }

    fn make_process(&mut self, input: &str, output: &str,
                    prog: Prog) ->
                    &Process {
        let pid = self.next_pid;
        let proc = Process::new(pid, input, output, prog);

        self.next_pid += 1;
        self.pids.insert(pid, proc);
        self.active.insert(pid);
        self.pids.get(&pid).unwrap()
    }
}
