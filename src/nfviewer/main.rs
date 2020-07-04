
use noisefunge::api::*;
use noisefunge::config::*;
use clap::{Arg, App};
use pancurses::{initscr, cbreak, noecho, endwin, Input, has_colors,
                start_color, init_pair, curs_set};
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
        let prev = 0;
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

fn main() {

    let baseuri = read_args();

    println!("baseuri: {}", baseuri);

    let window = initscr();
    cbreak();
    noecho();
    curs_set(0);
    if has_colors() {
        start_color();
        init_pair(1, pancurses::COLOR_WHITE, pancurses::COLOR_RED);
    }
    window.nodelay(true);
    let (mut miny, mut minx) = window.get_beg_yx();
    let (mut maxy, mut maxx) = window.get_max_yx();
    let mut done = false;
    let mut retile = true;

    let handle = start_request_thread(&baseuri);
    let sleep_dur = Duration::from_millis(10);
    let mut server_state = EngineState::new();
    let mut errs = String::new();

    'outer: while !done {
        window.color_set(0);
        window.clear();
        window.mvaddstr(maxy - 1, 0, format!("{}", server_state.beat));
        window.mvaddstr(1,1,format!("{} - {}", miny, minx));
        window.mvaddstr(2,2,format!("{} - {}", maxy, maxx));
        let elen = errs.len();
        if elen < maxx as usize {
            for _ in 0..maxx as usize - elen {
                errs.push(' ');
            }
        } else if elen > maxx as usize {
            errs = String::from(&errs[elen-maxx as usize..elen]);
        }
        window.mv(maxy - 2, 0);
        window.color_set(1);
        window.mvaddstr(maxy - 2, 0, errs.clone());
        window.refresh();
        loop {
            match window.getch() {
                None => break,
                Some(Input::KeyResize) => {
                    miny = window.get_beg_y();
                    minx = window.get_beg_x();
                    maxy = window.get_max_y();
                    maxx = window.get_max_x();
                    retile = true;
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
                    Ok(st) => server_state = st,
                    Err(s) => {
                        errs.push_str("   ");
                        errs.push_str(&s);
                    }
                }
                cond.notify_one();
            }
        }

        if !done { thread::sleep(Duration::from_millis(100)) };
    }

    endwin();
}
