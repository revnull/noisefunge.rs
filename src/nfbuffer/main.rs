
use clap::{Arg, App};
use noisefunge::api::*;
use pancurses::{initscr, cbreak, noecho, endwin, Input, has_colors,
                start_color, init_pair, curs_set};
use std::cmp;
use std::mem;
use std::time::Duration;

fn read_args() -> String {
    let matches = App::new("nfbuffer")
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
    let client = FungeClient::new(&baseuri);
    let sleep_dur = Duration::from_millis(10);
    let mut state : Option<EngineState> = None;
    let mut err = None;
    let mut needs_redraw = true;

    'outer: loop {

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
                let mut rcount = 0;
                let mut wcount = 0;
                let active = st.procs.iter().filter(|p| p.1.active).count();
                let sleeping = st.sleeping;

                let mut buffers =
                    st.buffers.iter().map(|(k,v)| {
                        if *v < 0 {
                            rcount += v.abs();
                        } else {
                            wcount += v;
                        }
                        (*k, *v)
                        }).collect::<Vec<(u8,i64)>>();
                buffers.sort_by(|(_,a),(_,b)| b.abs().cmp(&a.abs()));

                window.mvaddstr(y, 0,
                                format!("{:<8} A:{:6} S:{:6} R{:6} W{:6}",
                                        st.beat, active, sleeping,
                                        rcount, wcount));
                y += 1;

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

        loop {
            match window.getch() {
                None => break,
                Some(Input::KeyResize) => {
                    needs_redraw = true;
                    continue 'outer;
                }
                Some(Input::Character('q')) => {
                    break 'outer;
                },
                Some(Input::Character('Q')) => {
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
