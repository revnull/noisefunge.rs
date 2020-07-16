
use clap::{Arg, App};
use noisefunge::api::*;
use pancurses::{initscr, cbreak, noecho, endwin, Input, has_colors,
                start_color, init_pair, curs_set, Window};
use reqwest::blocking::Client;
use std::cmp;
use std::mem;
use std::thread;
use std::sync::{Arc, Mutex, Condvar};
use std::time::Duration;
use std::collections::BTreeMap;

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

fn main() {

    let baseuri = read_args();

    let window = initscr();
    cbreak();
    noecho();
    curs_set(0);
    if has_colors() {
        start_color();
        init_pair(1, pancurses::COLOR_WHITE, pancurses::COLOR_RED);
        init_pair(2, pancurses::COLOR_CYAN, pancurses::COLOR_BLACK);
        init_pair(3, pancurses::COLOR_BLACK, pancurses::COLOR_CYAN);
        init_pair(4, pancurses::COLOR_GREEN, pancurses::COLOR_BLACK);
        init_pair(5, pancurses::COLOR_BLACK, pancurses::COLOR_GREEN);
    }

    window.nodelay(true);
    let handle = start_request_thread(&baseuri);
    let mut done = false;
    let sleep_dur = Duration::from_millis(10);
    let mut state : Option<EngineState> = None;
    let mut err = None;
    let mut needs_redraw = true;

    'outer: while !done {

        if needs_redraw {
            needs_redraw = false;
            let (mut y, minx) = window.get_beg_yx();
            let (maxy, maxx) = window.get_max_yx();
            let rd_str = (0..maxx).map(|_| "~").collect::<String>();
            let wr_str = (0..maxx).map(|_| ".").collect::<String>();
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
                let mut buffers = st.buffers.iter().map(|(k,v)| (*k, *v))
                                            .collect::<Vec<(u8,i64)>>();
                buffers.sort_by(|(_,a),(_,b)| b.abs().cmp(&a.abs()));
                for (id, size) in buffers {
                    if y == maxy {
                        break;
                    }
                    window.mvaddstr(y,0,format!("{:2X}", id));
                    if size < 0 {
                        window.color_set(2);
                    } else {
                        window.color_set(4);
                    }
                    window.mvaddstr(y,3,format!("{:4} ", size.abs()));
                    let x = window.get_cur_x();
                    let bar = cmp::min((maxx - x) as i64, size.abs()) as usize;
                    if size < 0 {
                        window.color_set(3);
                        window.addnstr(&rd_str, bar);
                    } else {
                        window.color_set(5);
                        window.addnstr(&wr_str, bar);
                    }
                    window.color_set(0);
                    y += 1;
                }
            }
            window.refresh();
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

}
