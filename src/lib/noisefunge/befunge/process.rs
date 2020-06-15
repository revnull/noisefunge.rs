
use std::cmp::max;

pub enum Dir { U, D, L, R }

pub struct PC { pub row : u8, pub col :u8 }

impl PC {
    pub fn new(r: u8, c: u8) -> PC {
        PC { row : r, col : c }
    }
}

pub struct Prog { width : u8, data : Vec<u8> }

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
        Ok(Prog { width : longest as u8, data : mem })
    }

    pub fn rows(&self) -> u8 {
        (self.data.len() / self.width as usize) as u8
    }

    pub fn cols(&self) -> u8 {
        self.width
    }

    pub fn lookup(&self, pc : &PC) -> u8 {
        let i = pc.row as usize * self.width as usize + pc.col as usize;
        self.data[i]
    }
}

pub struct ProcessState {
    memory: Prog,
    pc: PC,
    dir: Dir
}

pub struct Process {
    pid: u64,
    input: String,
    output: String,
    state: Vec<ProcessState>
}

impl Process {
    pub fn new(pid: u64, input: &str, output: &str, prog: Prog) ->
               Process {
        let st = ProcessState { memory: prog,
                                pc: PC { col : 0, row : 0 },
                                dir: Dir::R };
        let mut stvec = Vec::new();
        stvec.push(st);
        Process { pid : pid,
                  input : String::from(input),
                  output : String::from(output),
                  state : stvec }
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
        assert_eq!(pr.lookup(&PC::new(0,0)), 49);
        assert_eq!(pr.lookup(&PC::new(1,1)), 55);
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
