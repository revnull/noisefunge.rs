
use rand::Rng;
use super::process::{Process, Dir};

macro_rules! pop {
    ($proc : ident) => {
        match $proc.pop() {
            Some(u) => u,
            None => return
        }
    }
}

pub trait Syscalls {
    // The interface between the engine and ops.
}

pub struct Op(Box<Fn(&mut Process, &mut dyn Syscalls)>);

struct OpSet([Option<Op>; 256]);

impl OpSet {
    fn new() -> OpSet {
        let mut ops : [Option<Op>; 256] = [
            // I guess some 3rd party crates solve this problem...
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
            None, None, None, None, None, None, None, None,
        ];
        ops[32] = Some(Op(Box::new(noop))); // Space
        ops[60] = Some(set_direction(Dir::L)); // >
        ops[62] = Some(set_direction(Dir::R)); // <
        ops[94] = Some(set_direction(Dir::U)); // ^
        ops[118] = Some(set_direction(Dir::D)); // v
        ops[63] = Some(Op(Box::new(rand_direction))); // ?
        OpSet(ops)
    }
}

fn noop(proc: &mut Process, eng: &mut Syscalls) {
    proc.step()
}

fn set_direction(dir: Dir) -> Op {
    let set_dir = move |proc: &mut Process, eng: &mut Syscalls| {
        proc.set_direction(dir);
        proc.step()
    };
    Op(Box::new(set_dir))
}

fn rand_direction(proc: &mut Process, eng: &mut Syscalls) {
    let mut rng = rand::thread_rng();
    let dir = match rng.gen_range(0,4) {
        0 => Dir::L,
        1 => Dir::R,
        2 => Dir::U,
        3 => Dir::D,
        _ => panic!("Random number out of range [0,4)")
    };
    proc.set_direction(dir);
    proc.step();
}

fn sleep(proc: &mut Process, eng: &mut Syscalls) {
    let beats = pop!(proc);
    
}
