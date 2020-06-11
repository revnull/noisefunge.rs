

pub struct Process<'a> {
    pid: u64,
    input: &'a str,
    output: &'a str
}

impl<'a> Process<'a> {
    pub fn new(pid : u64, input: &'a str, output: &'a str) ->
               Process<'a> {
        Process { pid : pid, input : input, output : output }
    }
}
