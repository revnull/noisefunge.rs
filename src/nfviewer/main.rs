
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
    width: i32,
    pid: Option<u64>
}

struct TileRow {
    height: i32,
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

    fn draw(&mut self, window: &Window) {
        if !self.needs_redraw {
            return;
        }

        let (mut y, minx) = window.get_beg_yx();
        let (maxy, maxx) = window.get_max_yx();

        let unused = self.state.procs.keys()
                                     .filter(|k| !self.active.contains(k));
        
        let mut new_rows = Vec::new();

        for row in &self.rows {
            let h = row.height;
            let mut new_tiles = Vec::new();
            let mut x = minx;

            if !new_tiles.is_empty() {
                new_rows.push(TileRow { height: row.height,
                                        tiles: new_tiles });
                y += h;
            }

        }

        self.rows = new_rows;

        // Clear and print beat.
        window.color_set(0);
        window.clear();
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
