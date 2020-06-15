
use super::process::Process;

pub trait Syscalls {
    // The interface between the engine and ops.
}

pub struct Op(Fn(&mut Process, &mut Syscalls));

struct OpSet<'a>([Option<&'a Op>; 255]);

impl<'a> OpSet<'a> {
    fn new() -> OpSet<'a> {
        let mut ops = [None; 255];

        OpSet(ops)
    }
}

fn noop(proc: &mut Process, eng: &mut Syscalls) {
    proc.step()
}
