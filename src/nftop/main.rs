
use clap::{Arg, App};
use noisefunge::api::*;
use pancurses::{initscr, cbreak, noecho, endwin, Input, has_colors,
                start_color, init_pair, curs_set};
use reqwest::blocking::Client;
use std::cmp::{Ordering};
use std::mem;
use std::thread;
use std::sync::{Arc, Mutex, Condvar};
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

type Handle = Arc<(Mutex<Option<Result<EngineState, String>>>, Condvar)>;

fn start_request_thread(baseuri: &str) -> Handle {
    let mtx = Mutex::new(None);
    let cond = Condvar::new();
    let arc = Arc::new((mtx, cond));
    let arc2 = Arc::clone(&arc);
    let basereq = format!("{}state", baseuri);

    thread::spawn(move || {
        let lock = &arc.0;
        let cond = &arc.1;
        let mut prev = 0;
        let mut delay = false;
        let client = Client::builder().user_agent("nftop")
                                      .build()
                                      .expect("Failed to build client");
        loop {
            if delay {
                delay = false;
                thread::sleep(Duration::from_secs(1));
            };

            let request = client.get(&basereq)
                                .query(&[("prev", prev.to_string())])
                                .timeout(Duration::from_secs(4))
                                .build()
                                .expect("Failed to build client");
            let response = client.execute(request);
            let msg = match response {
                Ok(response) => {
                    if response.status().is_success() {
                        response.json().map_err(|e|
                            format!("Serialization error: {:?}", e))
                            .map(|s: EngineState| { prev = s.beat; s })
                    } else {
                        delay = true;
                        prev = 0;
                        Err(format!("Bad status code: {}",
                                    response.status()))
                    }
                }
                Err(e) => {
                    delay = true;
                    Err(format!("HTTP request failed: {}", e))
                }
            };
            let mut val = lock.lock().unwrap();
            while val.is_some() {
                val = cond.wait(val).unwrap();
            };
            *val = Some(msg);
            cond.notify_one();
        }
    });

    return arc2;
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
    let handle = start_request_thread(&baseuri);
    let sleep_dur = Duration::from_millis(10);
    let mut state : Option<EngineState> = None;
    let mut err = None;
    let mut needs_redraw = true;
    let mut ordering = OrderBy::DataStack;
    let unnamed = String::from("-");

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
                            OrderBy::CallStack => proc.call_stack,
                        },
                    });
                }

                while y < maxy {
                    let pid = match heap.pop() {
                        None => break,
                        Some(op) => op.pid,
                    };
                    let proc = st.procs.get(&pid).unwrap();
                    let name = proc.name.map(|i| &st.names[i]).unwrap_or(&unnamed);

                    window.color_set(0);
                    window.mvaddstr(y, 0, format!("{:X}", pid));
                    window.mvaddnstr(y, 11, format!("{}", name), 20);
                    if proc.data_stack > 32 { window.color_set(1); }
                    window.mvaddstr(y, 32, format!("{}", proc.data_stack));
                    window.color_set(0);
                    if proc.call_stack > 32 { window.color_set(1); }
                    window.mvaddstr(y, 40, format!("{}", proc.call_stack));
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

        let lock = &handle.0;
        let cond = &handle.1;
        let mut val = lock.lock().unwrap();
        if val.is_none() {
            let tup = cond.wait_timeout(val, sleep_dur).unwrap();
            val = tup.0
        }
        if val.is_some() {
            match val.take().unwrap() {
                Ok(st) => {
                    state = Some(st);
                    err = None
                },
                Err(s) => err = Some(s)
            }
            needs_redraw = true;
            cond.notify_one();
        }
    }

    endwin();
}
