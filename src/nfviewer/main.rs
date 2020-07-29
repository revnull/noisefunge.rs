
use noisefunge::api::*;
use clap::{Arg, App};
use std::mem;
use pancurses::{initscr, cbreak, noecho, endwin, Input, has_colors,
                start_color, init_pair, curs_set, Window};
use std::collections::HashSet;
use std::cmp;
use std::rc::Rc;
use std::time::Duration;
use std::thread;
use std::sync::{Arc, Mutex, Condvar};
use reqwest::blocking::Client;

fn read_args() -> String {
    let matches = App::new("nfviewer")
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
        let client = Client::builder().user_agent("nfviewer")
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
                                .expect("Failed to build request");
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
                        Err(format!("Bad status code: {:?}",
                                    response.status()))
                    }
                }
                Err(e) => {
                    delay = true;
                    Err(format!("HTTP request failed: {:?}", e))
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

struct Tile {
    width: usize,
    pid: u64,
    last_pc: Option<(Rc<str>,usize)>,
    buffer: String
}

struct TileRow {
    height: usize,
    tiles: Vec<Tile>
}

struct Tiler {
    rows: Vec<TileRow>,
    state: EngineState,
    active: HashSet<u64>,
    prog_set: HashSet<Rc<str>>,
    progs: Vec<(usize, Rc<str>)>,
    errors: String,
    needs_redraw: bool,
}

impl Tiler {
    fn new() -> Self {
        Tiler {
            rows: Vec::new(),
            state: EngineState::new(),
            active: HashSet::new(),
            prog_set: HashSet::new(),
            progs: Vec::new(),
            errors: String::new(),
            needs_redraw: true
        }
    }

    fn update_state(&mut self, state: EngineState) {
        self.state = state;
        self.needs_redraw = true;

        for (pid, msg) in &self.state.crashed {
            self.errors.push_str(&format!("{:X}: {:?}. ", pid, msg));
        }

        let state_progs = mem::take(&mut self.state.progs);
        let prog_set = mem::take(&mut self.prog_set);
        self.progs = Vec::new();
        for (width, prog) in state_progs {
            let rcprog = Rc::from(prog);
            let cloned = match prog_set.get(&rcprog) {
                Some(rc) => {
                    self.prog_set.insert(Rc::clone(rc));
                    Rc::clone(rc)
                }
                None => {
                    self.prog_set.insert(Rc::clone(&rcprog));
                    rcprog
                }
            };
            self.progs.push((width, cloned));
        }
    }

    fn try_draw_process(&self, window: &Window, x: usize, y: usize,
                        min_width: usize, min_height: usize,
                        max_width: usize, max_height: usize,
                        pid: u64, last_tile: Option<&mut Tile>)
                        -> Option<(usize, Tile)> {

        let proc = match self.state.procs.get(&pid) {
            Some(x) => x,
            None => return None
        };

        let (width, text) = &self.progs[proc.prog];
        let display_width = cmp::max(min_width, *width + 1);

        let height = cmp::max(min_height, (text.chars().count() / width) + 2);

        if display_width > max_width || height > max_height {
            return None;
        }

        let (last_pc, mut buffer) = match last_tile {
            Some(t) => {
                let last_pc = match t.last_pc.as_ref() {
                    Some((txt, p)) => {
                        if Rc::ptr_eq(&txt, text) {
                            Some(*p)
                        } else {
                            None
                        }
                    }
                    None => None
                };
                (last_pc, mem::take(&mut t.buffer))
            },
            None => (None, String::new())
        };

        let mut dy = 0;
        for (i, ch) in text.chars().enumerate() {
            let mut s = String::new();
            s.push(ch);
            if i % width == 0 {
                window.mv(dy + y as i32,x as i32);
                dy += 1;
            }
            if !proc.active && i == proc.pc {
                window.color_set(5);
                window.addstr(s);
                window.color_set(0);
            } else if Some(i) == last_pc {
                window.color_set(4);
                window.addstr(s);
                window.color_set(0);
            } else if i == proc.pc {
                window.color_set(3);
                window.addstr(s);
                window.color_set(0);
            } else {
                window.addstr(s);
            }
        }

        let pid_str = format!("{:X}", pid);
        let max_buf = display_width - pid_str.len() - 1;
        if let Some(s) = &proc.output {
            buffer.push_str(s);
            let buf_len = buffer.chars().count();
            if buf_len > max_buf {
                buffer = buffer.chars().skip(buf_len - max_buf).collect()
            }
        }

        window.color_set(6);
        window.mvaddstr((y + height - 3) as i32, x as i32, &pid_str);
        if let Some(i) = proc.name {
            window.mvaddnstr((y + height - 2) as i32, x as i32,
                             &self.state.names[i], (display_width - 1) as i32);
        }
        window.color_set(2);
        window.mvaddstr((y + height - 3) as i32, (x + pid_str.len()) as i32,
                        &buffer);
        window.color_set(0);

        Some((height, Tile { width: display_width,
                             pid: pid,
                             last_pc: Some((Rc::clone(text), proc.pc)),
                             buffer: buffer }))
    }

    fn draw(&mut self, window: &Window) {
        if !self.needs_redraw {
            return;
        }

        let (mut y, minx) = window.get_beg_yx();
        let (maxy, maxx) = window.get_max_yx();

        // Clear and print beat.
        window.clear();
        window.color_set(0);
        window.mvaddstr(maxy - 1, 0, format!("{}", self.state.beat));

        // Error bar
        let elen = self.errors.len();
        if elen < maxx as usize {
            for _ in 0..maxx as usize - elen {
                self.errors.push(' ');
            }
        } else if elen > maxx as usize {
            self.errors = String::from(&self.errors[elen-maxx as usize..elen]);
        }
        window.mv(maxy - 2, 0);
        window.color_set(1);
        window.mvaddstr(maxy - 2, 0, self.errors.clone());
        window.color_set(0);

        let active = mem::take(&mut self.active);
        let mut unused = self.state.procs.keys()
                                         .filter(|k| !active.contains(k))
                                         .take(20);
        let mut rows = mem::take(&mut self.rows);
        let mut old_rows = rows.iter_mut();

        let maxy = maxy - 2;

        'outer: while y < maxy {
            let mut x = minx;

            if let Some(row) = old_rows.next() {
                let mut new_tiles = Vec::new();
                for tile in row.tiles.iter_mut() {
                    if let Some((_height, t)) =
                        self.try_draw_process(window, x as usize, y as usize,
                                              tile.width, row.height,
                                              tile.width, row.height,
                                              tile.pid, Some(tile)) {
                        x += t.width as i32;
                        new_tiles.push(t);
                        self.active.insert(tile.pid);
                    }
                }

                'append: while x < maxx {
                    if let Some((_height, t)) = unused.next()
                        .and_then(|pid|
                            self.try_draw_process(window, x as usize,
                                                  y as usize,
                                                  0, row.height,
                                                  (maxx - x) as usize, 
                                                  row.height, *pid,
                                                  None)) {
                        x += t.width as i32;
                        self.active.insert(t.pid);
                        new_tiles.push(t);
                    } else {
                        break 'append;
                    }
                }

                if !new_tiles.is_empty() {
                    self.rows.push(TileRow { height: row.height,
                                             tiles: new_tiles });
                    
                    y += row.height as i32;
                }
                continue 'outer;
            }
            
            if let Some(pid) = unused.next() {
                if let Some((height, t)) =
                    self.try_draw_process(window, x as usize, y as usize,
                                          0, 0,
                                          (maxx - x) as usize, 
                                          (maxy - y) as usize, *pid,
                                          None) {
                    self.rows.push(TileRow { height: height,
                                             tiles: vec![t] });
                    self.active.insert(*pid);
                    y += height as i32;
                }

                continue 'outer;
            } else {
                break 'outer;
            }

        }

        window.refresh();
        self.needs_redraw = false;
    }

    fn retile(&mut self) {
        self.rows = Vec::new();
        self.active = HashSet::new();
        self.errors = String::new();
        self.needs_redraw = true;
    }

    fn push_error(&mut self, err: &str) {
        self.errors.push_str("   ");
        self.errors.push_str(&err);
        self.needs_redraw = true;
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
        init_pair(3, pancurses::COLOR_BLACK, pancurses::COLOR_CYAN);
        init_pair(4, pancurses::COLOR_BLACK, pancurses::COLOR_BLUE);
        init_pair(5, pancurses::COLOR_BLACK, pancurses::COLOR_YELLOW);
        init_pair(6, pancurses::COLOR_BLACK, pancurses::COLOR_MAGENTA);
    }
    window.nodelay(true);

    let handle = start_request_thread(&baseuri);
    let sleep_dur = Duration::from_millis(10);
    let mut tiler = Tiler::new();

    'outer: loop {
        tiler.draw(&window);
        loop {
            match window.getch() {
                None => break,
                Some(Input::KeyResize) => {
                    tiler.retile();
                    continue 'outer;
                },
                Some(Input::Character('r')) => {
                    tiler.retile();
                    continue 'outer;
                },
                Some(Input::Character('R')) => {
                    tiler.retile();
                    continue 'outer;
                },
                Some(Input::Character('q')) => {
                    break 'outer;
                },
                Some(Input::Character('Q')) => {
                    break 'outer;
                },
                _ => { },
            };
        }

        {
            let lock = &handle.0;
            let cond = &handle.1;
            let mut val = lock.lock().unwrap();
            if val.is_none() {
                let tup = cond.wait_timeout(val, sleep_dur).unwrap();
                val = tup.0;
            }
            if val.is_some() {
                match val.take().unwrap() {
                    Ok(st) => tiler.update_state(st),
                    Err(s) => tiler.push_error(&s),
                }
                cond.notify_one();
            }
        }

    }

    endwin();
}
