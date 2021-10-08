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

use noisefunge::api::*;
use clap::{Arg, App};
use std::mem;
use pancurses::{initscr, cbreak, noecho, endwin, Input, has_colors,
                start_color, init_pair, curs_set, Window};
use std::collections::{HashSet};
use std::rc::Rc;
use std::time::Duration;

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

struct View {
    pid: u64,
    last_pc: Option<(Rc<str>,usize)>,
    buffer: String
}

impl View {
    fn new(pid: u64) -> Self {
        View { pid: pid,
               last_pc: None,
               buffer: String::new() }
    }
}

struct Tile {
    xpos: i32,
    ypos: i32,
    width: i32,
    height: i32,
    view: Option<View>
}

impl Tile {
    fn new(x: i32, y: i32, width: i32, height: i32,
           view: Option<View>) -> Self {
        Tile { xpos: x,
               ypos: y,
               width: width,
               height: height,
               view: view }
    }

    fn is_empty(&self) -> bool {
        self.view.is_none()
    }

    fn draw(&mut self, win: &Window, unused: &mut Vec<u64>, tiler: &mut Tiler,
            allow_split: bool, allow_retry: bool) -> Option<Tile> {
        
        if self.is_empty() {
            if ! allow_retry { return None }
            let pid = match unused.pop() {
                None => return None,
                Some(p) => p
            };
            self.view = Some(View::new(pid));
            return self.draw(win, unused, tiler, true, false);
        }
        let mut view = self.view.take().unwrap();
        let pid = view.pid;

        let proc = match tiler.state.procs.get(&pid) {
            None => {
                return self.draw(win, unused, tiler, true, true);
            }
            Some(p) => p,
        };

        let (pi, pc) = match proc.call_stack.last() {
            None => return None,
            Some((pi, pc)) => (*pi, *pc),
        };

        let (width, height, text) = &tiler.progs[pi];
        let width = *width;
        let height = *height;
        let display_height = height as i32 + 3;
        let display_width = width as i32 + 1;

        if display_width > self.width || display_height > self.height {
            // self.view is currently None. Try to draw new proc.
            return self.draw(win, unused, tiler, true, allow_retry);
        }

        tiler.active.insert(pid);
        let (last_pc, is_new) = match view.last_pc.take() {
            Some((txt, p)) => {
                if Rc::ptr_eq(&txt, text) {
                    (Some(p), false)
                } else {
                    (None, false)
                }
            },
            None => (None, true)
        };

        let mut y = self.ypos;
        for (i, ch) in text.chars().enumerate() {
            let s = ch.to_string();

            if i % width == 0 {
                win.mv(y, self.xpos);
                y += 1;
            }
            if i == pc {
                if proc.active {
                    win.color_set(3);
                } else {
                    win.color_set(5);
                }
                win.addstr(s);
                win.color_set(0);
            } else if Some(i) == last_pc {
                if proc.play.is_some() {
                    win.color_set(7);
                } else {
                    win.color_set(4);
                }
                win.addstr(s);
                win.color_set(0);
            } else {
                win.addstr(s);
            }
        }

        let pid_str = format!("{:X}", pid);
        let max_buf = self.width as usize - 1;
        if let Some(s) = &proc.output {
            view.buffer.push_str(s);
            let buf_len = view.buffer.chars().count();
            if buf_len > max_buf {
                view.buffer = view.buffer.chars()
                                         .skip(buf_len - max_buf)
                                         .collect();
            }
        }

        if is_new {
            win.color_set(3);
        } else {
            win.color_set(6);
        }
        win.mvaddstr(self.ypos + self.height as i32 - 3, self.xpos, &pid_str);
        let i = proc.name;
        win.mvaddnstr(self.ypos + self.height as i32 - 2, self.xpos,
                      &tiler.state.names[i], (self.width - 1) as i32);

        win.color_set(2);
        win.mvaddstr(self.ypos + self.height as i32 - 1,
                     self.xpos, &view.buffer);
        win.color_set(0);

        view.last_pc = Some((Rc::clone(text), pc));
        self.view = Some(view);

        if allow_split && display_width < self.width {
            let new = Tile::new(self.xpos + display_width, self.ypos,
                                self.width - display_width, self.height, None);
            self.width = display_width;
            return Some(new);

        }

        return None;
    }

}

struct TileRow {
    xpos: i32,
    ypos: i32,
    width: i32,
    height: i32,
    tiles: Vec<Tile>
}

impl TileRow {
    fn new(x: i32, y: i32, width: i32, height: i32) -> TileRow {
        TileRow { xpos: x,
                  ypos: y,
                  width: width,
                  height: height,
                  tiles: Vec::new() }
    }

    fn is_empty(&self) -> bool {
        self.tiles.is_empty()
    }

    fn draw(&mut self, win: &Window, unused: &mut Vec<u64>,
            tiler: &mut Tiler) {

        let mut x = self.xpos;
        let mut tiles = mem::take(&mut self.tiles);
        let mut prev_empty = false;

        for mut t in tiles.drain(..) {
            if t.xpos != x || t.ypos != self.ypos {
                panic!("x: {} vs {} - y: {} vs {}", t.xpos, self.xpos,
                       t.ypos, self.ypos);
            }
            match t.draw(win, unused, tiler, false, true) {
                None => {
                    x += t.width;
                    if t.is_empty() && prev_empty {
                        match self.tiles.last_mut() {
                            None => panic!("No last tile."),
                            Some(prev) => prev.width += t.width,
                        }
                    } else {
                        self.tiles.push(t);
                    }
                },
                Some(empty) => {
                    x += t.width + empty.width;
                    self.tiles.push(t);
                    self.tiles.push(empty);
                },
            }

            prev_empty = self.tiles.last().map(|t| t.is_empty())
                                          .unwrap_or(false);
        }

        // Pop any empty ones off of the end.
        while self.tiles.last().map(|t| t.is_empty()).unwrap_or(false) {
            let t = self.tiles.pop().unwrap();
            x = t.xpos;
        }

        let mut attempts = 5;
        while x < (self.xpos + self.width) && attempts > 0 {
            let pid = match unused.last() {
                None => break,
                Some(pid) => pid,
            };
            let proc = match tiler.state.procs.get(&pid) {
                None => {
                    attempts -= 1;
                    unused.pop();
                    continue;
                },
                Some(proc) => proc,
            };

            let pi = match proc.call_stack.last() {
                None => {
                    unused.pop();
                    continue;
                },
                Some((pi, _)) => *pi,
            };
            let (width, height, _text) = &tiler.progs[pi];
            let width = *width;
            let height = *height;
            let display_width = width as i32 + 1;
            let display_height = height as i32 + 3;
            if display_height > self.height || x + display_width > self.width {
                attempts -= 1;
                unused.pop();
                continue;
            }
            let mut tile = Tile::new(x, self.ypos, display_width,
                                     self.height, None);
            tile.draw(win, unused, tiler, false, false);
            x += tile.width;
            self.tiles.push(tile);
        }

    }

}

struct Tiler {
    rows: Vec<TileRow>,
    state: EngineState,
    active: HashSet<u64>,
    prog_set: HashSet<Rc<str>>,
    progs: Vec<(usize, usize, Rc<str>)>, // width, height, body
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
            let height = cloned.chars().count() / width;
            self.progs.push((width, height, cloned));
        }
    }

    fn draw(&mut self, win: &Window) {
        if !self.needs_redraw {
            return;
        }
        self.needs_redraw = false;
        let (miny, minx) = win.get_beg_yx();
        let (maxy, maxx) = win.get_max_yx();

        // Clear and print beat.
        win.clear();
        win.color_set(0);
        win.mvaddstr(maxy - 1, 0, format!("{}", self.state.beat));

        // Error bar
        let elen = self.errors.len();
        if elen < maxx as usize {
            for _ in 0..maxx as usize - elen {
                self.errors.push(' ');
            }
        } else if elen > maxx as usize {
            self.errors = String::from(&self.errors[elen-maxx as usize..elen]);
        }
        win.mv(maxy - 2, 0);
        win.color_set(1);
        win.mvaddstr(maxy - 2, 0, self.errors.clone());
        win.color_set(0);

        let maxy = maxy - 3;
        let mut y = miny;

        let active = mem::take(&mut self.active);
        let mut unused = self.state.procs.keys()
                                         .filter(|p| !active.contains(p))
                                         .take(50)
                                         .cloned()
                                         .collect();

        let mut old_rows = mem::take(&mut self.rows);
        for mut row in old_rows.drain(..) {
            row.draw(win, &mut unused, self);
            y += row.height;
            self.rows.push(row);
        };

        while self.rows.last().map(|t| t.is_empty()).unwrap_or(false) {
            let row = self.rows.pop().unwrap();
            y -= row.height;
        }

        while y < maxy {
            let pid = match unused.last() {
                None => break,
                Some(pid) => pid,
            };
            let proc = match self.state.procs.get(&pid) {
                None => {
                    unused.pop();
                    continue;
                },
                Some(proc) => proc,
            };

            let (_width, height, _text) = match proc.call_stack.last() {
                Some((pi, _)) => &self.progs[*pi],
                None => {
                    unused.pop();
                    continue;
                },
            };

            let display_height = *height as i32 + 3;
            if y + display_height > maxy {
                unused.pop();
                continue;
            }
            let mut row = TileRow::new(minx, y, maxx - minx, display_height);
            row.draw(win, &mut unused, self);
            y += row.height;
            self.rows.push(row);
        }

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
        init_pair(7, pancurses::COLOR_BLACK, pancurses::COLOR_GREEN);
    }
    window.nodelay(true);

    let client = FungeClient::new(&baseuri);
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

        match client.get_state(sleep_dur) {
            None => {},
            Some(Ok(st)) => {
                tiler.update_state(st);
            },
            Some(Err(s)) => {
                tiler.push_error(&s);
            }
        }
    }

    endwin();
}
