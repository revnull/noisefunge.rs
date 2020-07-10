
use std::cmp::max;
use std::rc::Rc;
use serde::{Serialize, Deserialize};
use super::charmap::CharMap;

#[derive(Copy, Clone, Serialize, Deserialize)]
pub enum Dir { U, D, L, R }

#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct PC(pub usize);

#[derive(Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Prog { width : usize, data : Vec<u8> }

impl Prog {

    pub fn parse(prog: &str) -> Result<Prog, &'static str> {
        if prog.len() == 0 {
            return Err("Empty program.");
        }
        let mut lines = Vec::new();
        let mut longest = 0;
        for line in prog.split("\n") {
            lines.push(line);
            longest = max(longest, line.bytes().count());
        }
        if longest == 0 {
            return Err("Program is empty.");
        }
        if lines.len() > 255 {
            return Err("Too many lines in program.");
        }
        if longest > 255 {
            return Err("Longest line is too long.");
        }
        let mut mem = Vec::new();
        for line in lines {
            let mut count = 0;
            for ch in line.bytes() {
                mem.push(ch);
                count += 1;
            }
            while count < longest {
                mem.push(32); // Pad with space
                count += 1
            }
        }
        Ok(Prog { width : longest, data : mem })
    }

    pub fn rows(&self) -> usize {
        self.data.len() / self.width
    }

    pub fn cols(&self) -> usize {
        self.width
    }

    pub fn lookup(&self, pc : PC) -> u8 {
        let PC(i) = pc;
        self.data[i as usize]
    }

    pub fn xy_to_pc(&self, x: usize, y: usize) -> Option<PC> {
        let i = self.width * y + x;
        if i < self.data.len() {
            Some(PC(i))
        } else {
            None
        }
    }

    pub fn state_tuple(&self, cm: &CharMap) -> (usize, String) {
        let mut res = String::new();
        for c in &self.data {
            res.push(cm[*c]);
        }
        (self.width, res)
    }
}

#[derive(Clone)]
pub struct ProcessStack {
    pub memory: Rc<Prog>,
    pub pc: PC,
    pub dir: Dir
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Syscall {
    Fork,
    Sleep(u8),
    Pause,
    PrintChar(u8),
    PrintNum(u8),
    Send(u8,u8),
    Receive(u8),
    Defop(u8),
    Call(u8),
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ProcessState {
    Running(bool),
    Trap(Syscall),
    Finished,
    Crashed(&'static str),
}

#[derive(Clone)]
pub struct Process {
    pub pid: u64,
    data_stack: Vec<u8>,
    call_stack: Vec<ProcessStack>,
    state: ProcessState
}

impl Process {
    pub fn new(pid: u64, prog: Rc<Prog>) -> Process {
        let st = ProcessStack { memory: prog,
                                pc: PC(0),
                                dir: Dir::R };
        let mut stvec = Vec::new();
        stvec.push(st);
        Process { pid : pid,
                  data_stack : Vec::new(),
                  call_stack : stvec,
                  state : ProcessState::Running(false) }
    }

    pub fn is_running(&self) -> bool {
        match self.state {
            ProcessState::Running(_) => true,
            _ => false
        }
    }

    pub fn state(&self) -> &ProcessState {
        &self.state
    }

    pub fn set_state(&mut self, st: ProcessState) {
        self.state = st
    }

    pub fn top(&self) -> Option<&ProcessStack> {
        let i = self.call_stack.len();
        if i == 0 {
            return None
        }

        self.call_stack.get(i - 1)
    }

    pub fn top_mut(&mut self) -> Option<&mut ProcessStack> {
        let i = self.call_stack.len();
        if i == 0 {
            return None
        }

        self.call_stack.get_mut(i - 1)
    }

    pub fn call(&mut self, prog: Rc<Prog>, pc: PC, dir: Dir) {
        self.call_stack.push(
            ProcessStack { memory : prog,
                           pc: pc,
                           dir: dir });
    }

    pub fn r#return(&mut self) {
        self.call_stack.pop();
        if self.call_stack.len() == 0 {
            self.set_state(ProcessState::Finished);
        }
    }

    pub fn dir(&self) -> Option<Dir> {
        let top = self.top()?;
        Some(top.dir)
    }

    pub fn push(&mut self, i: u8) {
        self.data_stack.push(i)
    }

    pub fn pop(&mut self) -> Option<u8> {
        self.data_stack.pop()
    }

    pub fn die(&mut self, msg: &'static str) {
        self.state = ProcessState::Crashed(msg)
    }

    pub fn set_direction(&mut self, dir: Dir) {
        self.top_mut().map(|top| top.dir = dir);
    }

    pub fn step(&mut self) {
        match self.top_mut() {
            None => self.state = ProcessState::Finished,
            Some(top) => {
                let PC(i) = top.pc;
                let w = top.memory.cols();
                let h = top.memory.rows();
                match top.dir {
                    Dir::L => {
                        if i % w == 0 {
                            self.die("Exited off left edge");
                            return;
                        }
                        top.pc = PC(i - 1);
                    },
                    Dir::R => {
                        if i % w == (w - 1) {
                            self.die("Exited off right edge");
                            return;
                        }
                        top.pc = PC(i + 1);
                    }
                    Dir::U => {
                        if i / w == 0 {
                            self.die("Exited off top edge");
                            return;
                        }
                        top.pc = PC(i - w);
                    },
                    Dir::D => {
                        if i / w == h - 1 {
                            self.die("Exited off bottom edge");
                            return;
                        }
                        top.pc = PC(i + w);
                    },
                }
            }
        }
    }

    pub fn trap(&mut self, sys: Syscall) {
        self.set_state(ProcessState::Trap(sys));
    }

    pub fn resume(&mut self, push: Option<u8>) {
        match push {
            None => {},
            Some(c) => self.data_stack.push(c)
        };
        self.set_state(ProcessState::Running(false));
    }

    pub fn apply(&mut self, op: &Op) {
        let Op(ref f) = op;
        f(self)
    }

    pub fn peek(&self) -> Option<u8> {
        let top = self.top()?;
        let PC(i) = top.pc;
        Some(top.memory.data[i])
    }

    pub fn fork(&self, newpid: u64) -> Self {
        let mut new = self.clone();
        new.pid = newpid;
        return new
    }
}

#[derive(Clone)]
pub struct Op(Rc<dyn Fn(&mut Process)>);

impl Op {
    pub fn new(f: Rc<dyn Fn(&mut Process)>) -> Self {
        Op(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_prog() {
        let pr = Prog::parse("12345\n\
                              67890\n\
                              a").unwrap();
        assert_eq!(pr.cols(), 5);
        assert_eq!(pr.rows(), 3);
        let v = vec![49,50,51,52,53,54,55,56,57,48,97,32,32,32,32];
        assert_eq!(pr.data, v);
        assert_eq!(pr.lookup(PC(0)), 49);
        assert_eq!(pr.lookup(PC(6)), 55);
    }

    #[test]
    fn bad_prog() {
        assert!(Prog::parse("").is_err(), "Empty program");
        assert!(Prog::parse("\n\n\n\n\n").is_err(), "Only newlines");

        let mut long_line = String::with_capacity(512);
        let mut too_many = String::with_capacity(512);

        for i in 0..256 {
            long_line.push_str("a");
            too_many.push_str("a\n");
        }

        assert!(Prog::parse(&long_line).is_err(), "Long line");
        assert!(Prog::parse(&too_many).is_err(), "Too many lines.");
    }

}
