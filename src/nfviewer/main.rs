
use noisefunge::api::*;
use noisefunge::config::*;
use clap::{Arg, App};
use pancurses::{initscr, cbreak, noecho, endwin, Input, has_colors,
                start_color, init_pair, curs_set, Window};
use std::collections::HashSet;
use std::time::Duration;
use std::thread;
use std::sync::{Arc, Mutex, Condvar};
use reqwest::blocking::Client;

fn read_args() -> String {
    let matches = App::new("nfviewer")
                          .arg(Arg::with_name("HOST")
                               .help("Noisefunge server host")
                               .required(false)
                               .default_value("localhost"))
                          .arg(Arg::with_name("PORT")
                               .help("Noisefunge server port")
                               .required(false)
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
    pid: u64
}

struct TileRow {
    height: usize,
    tiles: Vec<Tile>
}

struct Tiler {
    rows: Vec<TileRow>,
    state: EngineState,
    active: HashSet<u64>,
    errors: String,
    needs_redraw: bool,
}

impl Tiler {
    fn new() -> Self {
        Tiler {
            rows: Vec::new(),
            state: EngineState::new(),
            active: HashSet::new(),
            errors: String::new(),
            needs_redraw: true
        }
    }

    fn update_state(&mut self, state: EngineState) {
        self.state = state;
        self.needs_redraw = true;
    }

    fn try_draw_process(&self, window: &Window, x: usize, mut y: usize,
                        max_width: usize, max_height: usize,
                        pid: u64) -> bool {

        let proc = match self.state.procs.get(&pid) {
            Some(x) => x,
            None => return false
        };

        let (width, text) = &self.state.progs[proc.prog];

        let height = (text.len() / width) + 1;

        if *width > max_width || height > max_height {
            return false;
        }

        for (i, ch) in text.chars().enumerate() {
            let mut s = String::new();
            s.push(ch);
            if i % width == 0 {
                window.mv(y as i32,x as i32);
                y += 1;
            }
            if i == proc.pc {
                if proc.active {
                    window.color_set(3);
                } else {
                    window.color_set(4);
                }
                window.addstr(s);
                window.color_set(0);
            } else {
                window.addstr(s);
            }
        }

        true
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

        let mut unused = self.state.procs.keys()
                                         .filter(|k| !self.active.contains(k));
        
        let mut new_rows = Vec::new();
        let mut new_active = HashSet::new();

        let mut old_rows = self.rows.iter();

        let maxy = maxy - 2;

        'outer: while y < maxy {
            let mut x = minx;

            if let Some(row) = old_rows.next() {
                let mut new_tiles = Vec::new();
                for tile in &row.tiles {
                    if self.try_draw_process(window, x as usize, y as usize,
                                             tile.width, row.height,
                                             tile.pid) {
                        x += tile.width as i32;
                        new_tiles.push(Tile { width: tile.width,
                                              pid: tile.pid });
                    }
                }

                if !new_tiles.is_empty() {
                    new_rows.push(TileRow { height: row.height,
                                            tiles: new_tiles });
                    
                    y += row.height as i32;
                }
                continue 'outer;
            }
            
            if let Some(pid) = unused.next() {
                if self.try_draw_process(window, x as usize, y as usize,
                                         (maxx - x) as usize, 
                                         (maxy - y) as usize, *pid) {
                    
                }

                continue 'outer;
            } else {
                break 'outer;
            }

        }

        self.rows = new_rows;
        self.active = new_active;

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
        init_pair(2, pancurses::COLOR_WHITE, pancurses::COLOR_BLUE);
        init_pair(3, pancurses::COLOR_WHITE, pancurses::COLOR_GREEN);
        init_pair(4, pancurses::COLOR_WHITE, pancurses::COLOR_YELLOW);
    }
    window.nodelay(true);
    let mut done = false;

    let handle = start_request_thread(&baseuri);
    let sleep_dur = Duration::from_millis(1000);
    let mut tiler = Tiler::new();

    'outer: while !done {
        tiler.draw(&window);
        loop {
            match window.getch() {
                None => break,
                Some(Input::KeyResize) => {
                    tiler.retile();
                    continue 'outer;
                }
                Some(i) => window.mvaddstr(3,3,format!("{:?}", i)),
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
