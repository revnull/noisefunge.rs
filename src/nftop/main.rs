/*
    Noisefunge Copyright (C) 2021 Rev. Johnny Healey <rev.null@gmail.com>

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use clap::{Arg, App};
use noisefunge::api::*;
use pancurses::{initscr, cbreak, noecho, endwin, Input, has_colors,
                start_color, init_pair, curs_set};
use std::cmp::{Ordering};
use std::mem;
use std::time::Duration;
use std::collections::BinaryHeap;

fn read_args() -> String {
    let matches = App::new("nftop")
                          .arg(Arg::with_name("HOST")
                               .help("Noisefunge server host")
                               .required(false)
                               .env("NOISEFUNGE_HOST")
                               .default_value("localhost"))
                          .arg(Arg::with_name("PORT")
                               .help("Noisefunge server port")
                               .required(false)
                               .env("NOISEFUNGE_PORT")
                               .default_value("1312"))
                          .get_matches();

    format!("http://{}:{}/", matches.value_of("HOST").unwrap(),
                             matches.value_of("PORT").unwrap())
}

#[derive(Eq, PartialEq, Ord, PartialOrd)]
enum OrderBy {
    DataStack,
    CallStack
}

#[derive(Eq, PartialEq)]
struct OrderedProcess {
    pid: u64,
    stack_size: usize,
}

impl PartialOrd for OrderedProcess {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OrderedProcess {
    fn cmp(&self, other: &Self) -> Ordering {
        self.stack_size.cmp(&other.stack_size)
    }
}

fn main() {

    let baseuri = read_args();

    let window = initscr();
    cbreak();
    noecho();
    curs_set(0);
    if has_colors() {
        start_color();
        init_pair(1, pancurses::COLOR_WHITE, pancurses::COLOR_RED);
        init_pair(2, pancurses::COLOR_BLACK, pancurses::COLOR_WHITE);
    }

    window.nodelay(true);
    let client = FungeClient::new(&baseuri);
    let sleep_dur = Duration::from_millis(10);
    let mut state : Option<EngineState> = None;
    let mut err = None;
    let mut needs_redraw = true;
    let mut ordering = OrderBy::DataStack;

    'outer: loop {

        if needs_redraw {
            needs_redraw = false;
            let (mut y, minx) = window.get_beg_yx();
            let (maxy, maxx) = window.get_max_yx();
            let width = maxx - minx;
            window.clear();
            if err.is_some() {
                let errs = mem::take(&mut err).unwrap();
                window.color_set(1);
                window.mvaddnstr(y, 0, errs, width);
                window.color_set(0);
                y += 1;
            }
            if let Some(st) = state.as_ref() {
                let mut rcount = 0;
                let mut wcount = 0;
                let active = st.procs.iter().filter(|p| p.1.active).count();
                let sleeping = st.sleeping;

                for v in st.buffers.values() {
                    if *v < 0 {
                        rcount += v.abs();
                    } else {
                        wcount += v;
                    }
                }

                window.mvaddstr(y, 0,
                                format!("{:<8} A:{:6} S:{:6} R{:6} W{:6}",
                                        st.beat, active, sleeping,
                                        rcount, wcount));
                y += 1;
                window.color_set(2);
                window.mvaddstr(y, 0, "PID        NAME                 DATA    CALL    ");
                window.color_set(0);

                y += 1;
                let mut heap = BinaryHeap::with_capacity(st.procs.len());
                for (pid, proc) in st.procs.iter() {
                    heap.push(OrderedProcess {
                        pid: *pid,
                        stack_size: match &ordering {
                            OrderBy::DataStack => proc.data_stack,
                            OrderBy::CallStack => proc.call_stack.len(),
                        },
                    });
                }

                while y < maxy {
                    let pid = match heap.pop() {
                        None => break,
                        Some(op) => op.pid,
                    };
                    let proc = st.procs.get(&pid).unwrap();
                    let name = st.names.get(proc.name).unwrap();

                    window.color_set(0);
                    window.mvaddstr(y, 0, format!("{:X}", pid));
                    window.mvaddnstr(y, 11, format!("{}", name), 20);
                    if proc.data_stack > 32 { window.color_set(1); }
                    window.mvaddstr(y, 32, format!("{}", proc.data_stack));
                    window.color_set(0);
                    if proc.call_stack.len() > 32 { window.color_set(1); }
                    window.mvaddstr(y, 40, format!("{}", proc.call_stack.len()));
                    window.color_set(0);

                    window.color_set(0);
                    y += 1;
                }

            }
            window.refresh();
        }

        loop {
            match window.getch() {
                None => break,
                Some(Input::KeyResize) => {
                    needs_redraw = true;
                    continue 'outer;
                }
                Some(Input::Character('c')) | Some(Input::Character('C')) => {
                    ordering = OrderBy::CallStack;
                    needs_redraw = true;
                    continue 'outer;
                }
                Some(Input::Character('d')) | Some(Input::Character('D')) => {
                    ordering = OrderBy::DataStack;
                    needs_redraw = true;
                    continue 'outer;
                }
                Some(Input::Character('q')) | Some(Input::Character('Q')) => {
                    break 'outer;
                },
                _ => ()
            }
        }

        match client.get_state(sleep_dur) {
            None => {},
            Some(Ok(st)) => {
                state = Some(st);
                err = None;
                needs_redraw = true;
            },
            Some(Err(s)) => {
                err = Some(s);
                needs_redraw = true;
            }
        }
    }

    endwin();
}
