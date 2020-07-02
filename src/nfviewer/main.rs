
use noisefunge::api::*;
use noisefunge::config::*;
use clap::{Arg, App};
use pancurses::{initscr, cbreak, noecho, endwin, Input};
use std::time::Duration;
use std::thread::sleep;

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

fn main() {

    let baseuri = read_args();

    println!("baseuri: {}", baseuri);

    let window = initscr();
    cbreak();
    noecho();
    window.nodelay(true);
    let (mut miny, mut minx) = window.get_beg_yx();
    let (mut maxy, mut maxx) = window.get_max_yx();
    let mut done = false;
    let mut retile = true;

    'outer: while !done {
        window.clear();
        window.mvaddstr(1,1,format!("{} - {}", miny, minx));
        window.mvaddstr(2,2,format!("{} - {}", maxy, maxx));
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
        if !done { sleep(Duration::from_millis(100)) };
    }

    endwin();
}
