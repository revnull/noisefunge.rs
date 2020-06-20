
use std::cmp::max;

#[derive(Copy, Clone)]
pub enum Dir { U, D, L, R }

#[derive(Copy, Clone)]
pub struct PC(usize);

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
}

pub struct ProcessStack {
    memory: Prog,
    pc: PC,
    dir: Dir
}

#[derive(Copy, Clone)]
pub enum Syscall {
    Fork,
    Sleep(u8)
}

#[derive(Copy, Clone)]
pub enum ProcessState {
    Running(bool),
    Trap(Syscall),
    Blocked,
    Finished,
    Crashed(&'static str),
}

pub struct Process {
    pid: u64,
    input: String,
    output: String,
    data_stack: Vec<u8>,
    call_stack: Vec<ProcessStack>,
    state: ProcessState
}

impl Process {
    pub fn new(pid: u64, input: &str, output: &str, prog: Prog) ->
               Process {
        let st = ProcessStack { memory: prog,
                                pc: PC(0),
                                dir: Dir::R };
        let mut stvec = Vec::new();
        stvec.push(st);
        Process { pid : pid,
                  input : String::from(input),
                  output : String::from(output),
                  data_stack : Vec::new(),
                  call_stack : stvec,
                  state : ProcessState::Running(false) }
    }

    pub fn state(&self) -> ProcessState {
        self.state
    }

    pub fn set_state(&mut self, st: ProcessState) {
        self.state = st
    }

    fn top(&mut self) -> Option<&mut ProcessStack> {
        let i = self.call_stack.len();
        if i == 0 {
            return None
        }

        self.call_stack.get_mut(i - 1)
    }

    pub fn call(&mut self, prog: Prog) {
        self.call_stack.push(
            ProcessStack { memory : prog,
                           pc: PC(0),
                           dir: Dir::R });
    }

    pub fn r#return(&mut self) {
        self.call_stack.pop();
        if self.call_stack.len() == 0 {
            self.set_state(ProcessState::Finished);
        }
    }

    pub fn dir(&mut self) -> Option<Dir> {
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
        self.top().map(|top| top.dir = dir);
    }

    pub fn step(&mut self) {
        match self.top() {
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
                        top.pc = PC(i - w);
                    },
                }
            }
        }
    }

    pub fn trap(&mut self, sys: Syscall) {
        self.set_state(ProcessState::Trap(sys));
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
        assert_eq!(pr.data, vec![49,50,51,52,53,54,55,56,57,48,97,32,32,32,32]);
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
