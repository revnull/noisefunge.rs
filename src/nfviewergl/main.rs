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
use std::time::{Duration, Instant};
use glfw::{Action, Context as _, Key, WindowEvent, PixelImage};
use luminance_glfw::GlfwSurface;
use luminance_windowing::{WindowDim, WindowOpt};
use luminance::{context::GraphicsContext, pipeline::PipelineState};
use ab_glyph::{Font,ScaleFont,FontArc};
use luminance_glyph::{GlyphBrushBuilder, GlyphBrush, Section, Layout,
                      HorizontalAlign, Instance};
use luminance_derive::{Semantics, Vertex};
use luminance::render_state::RenderState;
use luminance::tess::{Mode, Interleaved, Tess};
use luminance::backend;
use luminance::blending::{Equation, Factor, Blending};
use luminance::pipeline::{TextureBinding};
use luminance::pixel::{NormR8UI, NormUnsigned};
use luminance::texture::{Dim2};
use glyph_brush::Text;
use std::collections::{HashSet, HashMap};
use std::convert::TryInto;
use std::cmp;
use std::mem::{take, replace};
use std::rc::Rc;
use rand::{Rng};

#[derive(Copy, Clone, Debug, Semantics)]
pub enum VertexSemantics {
    #[sem(name = "position", repr = "[f32; 3]", wrapper = "VertexPosition")]
    Position,
    #[sem(name = "color", repr = "[f32; 4]", wrapper = "VertexRGBA")]
    Color,
}

impl VertexRGBA {
    fn with_alpha(&self, alpha: f32) -> Self {
        VertexRGBA::new([self[0], self[1], self[2], alpha])
    }

    fn invert(&self) -> Self {
        VertexRGBA::new([1. - self[0], 1. - self[1], 1. - self[2], self[3]])
    }

    fn rgb_mul(&self, x: f32) -> Self {
        VertexRGBA::new([self[0] * x, self[1] * x, self[2] * x, self[3]])
    }

    fn rgb_shift(&self, x: u64) -> Self {
        match x % 6 {
            1 => VertexRGBA::new([self[1], self[2], self[0], self[3]]),
            2 => VertexRGBA::new([self[2], self[0], self[1], self[3]]),
            3 => VertexRGBA::new([self[0], self[2], self[1], self[3]]),
            4 => VertexRGBA::new([self[1], self[0], self[2], self[3]]),
            5 => VertexRGBA::new([self[2], self[1], self[0], self[3]]),
            _ => *self
        }
    }

}

const VS_STR: &str = include_str!("vert.glsl");
const FS_STR: &str = include_str!("frag.glsl");

#[derive(Copy, Clone, Debug, Vertex)]
#[vertex(sem = "VertexSemantics")]
pub struct Vertex {
    #[allow(dead_code)]
    position: VertexPosition,

    #[allow(dead_code)]
    #[vertex(normalized = "true")]
    color: VertexRGBA,
}

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

#[derive(Copy, Clone, Debug, Hash)]
struct Dimensions(u32, u32); // width, height

impl Dimensions {
    fn new(w: u32, h: u32) -> Self {
        Dimensions(w, h)
    }

    #[inline]
    fn project_x(&self, x: f32) -> f32 {
        let mid = self.0 as f32 / 2.;
        (x - mid) / mid
    }

    #[inline]
    fn project_y(&self, y: f32) -> f32 {
        let mid = self.1 as f32 / -2.;
        (y + mid) / mid
    }

}

#[derive(Copy, Clone, Debug)]
struct FontMetrics(f32, f32);

impl FontMetrics {
    fn new(font: &ab_glyph::FontArc, height: f32) -> Self {
        let test_glyph = font.glyph_id('W');
        let scale = font.as_scaled(height);
        FontMetrics(scale.h_advance(test_glyph), height)
    }

    #[inline]
    fn project(&self, left: f32, top: f32, across: f32, down: f32) ->
        (f32, f32)
    {
        (left + (self.0 * across), top + (self.1 * down))
    }
}

struct ScreenSettings {
    font: ab_glyph::FontArc,
    dims: Dimensions,
    large_font: FontMetrics,
    small_font: FontMetrics,
    highlight: VertexRGBA,
    scroll_speed: u32,
    proc_limit: usize,
}

impl ScreenSettings {
    fn new(font: &ab_glyph::FontArc, dims: Dimensions, large_font_px: f32,
           small_font_px: f32, highlight: VertexRGBA, scroll_speed: u32,
           proc_limit: usize) -> Self {

        ScreenSettings {
            font: font.clone(),
            dims: dims,
            large_font: FontMetrics::new(font, large_font_px),
            small_font: FontMetrics::new(font, small_font_px),
            highlight: highlight,
            scroll_speed: scroll_speed,
            proc_limit: proc_limit,
        }

    }

    fn rescale(&mut self, dims: Dimensions) {
        self.dims = dims;
    }

    fn update_highlight(&mut self, highlight: VertexRGBA) {
        self.highlight = highlight;
    }

    fn update_large_font(&mut self, px: f32) {
        self.large_font = FontMetrics::new(&self.font, px);
    }

    fn update_small_font(&mut self, px: f32) {
        self.small_font = FontMetrics::new(&self.font, px);
    }

}

struct ErrBar {
    errs : String,
    max_chars : usize,
    size_check : bool,
    dims: Dimensions
}

impl ErrBar {
    fn new(dims: Dimensions) -> Self {
        ErrBar {
            errs: String::new(),
            max_chars: 160,
            size_check: false,
            dims: dims }
    }

    fn resize(&mut self, dims: Dimensions) {
        self.dims = dims;
    }

    fn push_err(&mut self, msg : &str) {
        self.errs.push_str(" ");
        self.errs.push_str(msg);
        print!("err: {}\n", &self.errs);
        self.size_check = true;
    }

    fn make_bar<B,C>(&self, ctxt: &mut C) -> Tess<B, Vertex>
    where
        C: GraphicsContext<Backend = B>,
        B: ?Sized + backend::tess::Tess<Vertex, (), (), Interleaved>
                  + backend::tess::Tess<(), (), (), Interleaved>
    {
        let top = self.dims.project_y((self.dims.1 - 40) as f32);
        let bot = self.dims.project_y((self.dims.1 - 20) as f32);
        let vtxs = [
            Vertex::new(
                VertexPosition::new([-1., top, 0.02]),
                VertexRGBA::new([1., 0., 0., 1.])),
            Vertex::new(
                VertexPosition::new([1., top, 0.02]),
                VertexRGBA::new([1., 0., 0., 1.])),
            Vertex::new(
                VertexPosition::new([1., bot, 0.02]),
                VertexRGBA::new([1., 0., 0., 1.])),
            Vertex::new(
                VertexPosition::new([-1., bot, 0.02]),
                VertexRGBA::new([1., 0., 0., 1.]))];

        ctxt.new_tess()
            .set_vertices(vtxs)
            .set_mode(Mode::TriangleFan)
            .build()
            .expect("Failed to make error bar")
    }

    fn make_text(&mut self) -> Section {
        if self.size_check {
            self.size_check = false;
            let count = self.errs.chars().count();
            if count > self.max_chars {
                let mut chars = self.errs.chars();
                for _ in 0..count - self.max_chars {
                    chars.next();
                }
                self.errs = chars.collect();
            }
        }
        self.make_text_immutable()
    }

    fn make_text_immutable(&self) -> Section {
        Section::default()
            .with_screen_position((self.dims.0 as f32, (self.dims.1 - 40) as f32))
            .with_layout(Layout::default_single_line()
                .h_align(HorizontalAlign::Right))
            .add_text(
                Text::default()
                    .with_text(&self.errs)
                    .with_z(0.01)
                    .with_color([1., 1., 1., 1.0])
                    .with_scale(20.))
    }
}

struct ProgText {
    body: String,
    width: usize,
    height: usize
}

impl ProgText {
    fn new(body: &str, width: usize) -> Self {
        let mut dest = String::new();
        let mut i = 0;
        let mut height = 0;
        let mut newline = true;
        for c in body.chars() {
            if newline {
                newline = false;
                height += 1
            }
            if i > 0 && i % width == 0 {
                dest.push('\n');
                newline = true;
            }
            dest.push(c);
            i += 1;
        }
        ProgText {
            body: dest,
            width: width,
            height: height
        }
    }
}

impl<'a> ProgText {
    fn text(&'a self, screen: &ScreenSettings, alpha: f32, depth: f32)
        -> Text<'a> {

        Text::new(&self.body)
            .with_z(depth)
            .with_color([1., 1., 1., alpha])
            .with_scale(screen.large_font.1)
    }
}

struct Scroll {
    anchor: (f32, f32),
    delta: (f32, f32),
    start: Instant,
    end: Instant,
}

impl Scroll {
    fn new(start_pos: (f32, f32), end_pos: (f32, f32), start_time: Instant,
           dur: f32) -> Self {

        let dx = end_pos.0 - start_pos.0;
        let dy = end_pos.1 - start_pos.1;

        Scroll {
            anchor: start_pos,
            delta: (dx / dur, dy / dur),
            start: start_time,
            end: start_time + Duration::from_secs_f32(dur)
        }
    }

    fn pos(&self, now: Instant) -> Option<(f32, f32)> {
        if now > self.end {
            return None;
        }
        let s = (now - self.start).as_secs_f32();
        let x = self.anchor.0 + self.delta.0 * s;
        let y = self.anchor.1 + self.delta.1 * s;
        Some((x, y))
    }
}

#[derive(Debug)]
struct Fade<A>(A, VertexRGBA, Instant);

struct Animated {
    pid: u64,
    label: String,
    scroll: Scroll,
    max_width: usize,
    max_height: usize,
    call_stack: Vec<(Rc<ProgText>, usize)>,
    output: String,
    fades: Vec<Fade<usize>>,
    ended: Option<Instant>,
    played: Option<Fade<()>>
}

impl Animated {
    fn new(pid: u64, name: Rc<str>, scroll: Scroll,
           call_stack: Vec<(Rc<ProgText>, usize)>) -> Self {
        let label = format!("{:X} {}", pid, &name);
        let mut max_width = 0;
        let mut max_height = 0;
        for (pt, _) in &call_stack {
            max_width = cmp::max(max_width, pt.width);
            max_height = cmp::max(max_height, pt.height);
        }
        Animated {
            pid: pid,
            label: label,
            scroll: scroll,
            max_width: max_width,
            max_height: max_height,
            call_stack: call_stack,
            output: String::new(),
            fades: Vec::new(),
            ended: None,
            played: None
        }
    }

    fn update(&mut self, call_stack: Vec<(Rc<ProgText>, usize)>,
              output: Option<String>, played: bool, now: Instant,
              screen: &ScreenSettings) {

        match &self.ended {
            Some(_) => return,
            _ => {}
        };

        if call_stack.is_empty() {
            self.ended = Some(now);
            return;
        }

        let highlight = screen.highlight.rgb_shift(self.pid);
        let old_played = take(&mut self.played);
        self.played = match (played, old_played) {
            (true, _) => {
                Some(Fade((), highlight.invert(), now))
            },
            (_, Some(f)) => { 
                if now.duration_since(f.2) < Duration::from_secs(2) {
                    Some(f)
                } else {
                    None
                }
            },
            _ => { None },
        };

        let old_stack = replace(&mut self.call_stack, call_stack);
        let new_top = self.call_stack.last().expect("Empty stack");
        let old_top = old_stack.last().expect("Empty stack");

        if self.call_stack.len() != old_stack.len() {
            self.fades.clear();

            if self.call_stack.len() > old_stack.len() {
                let new_top = self.call_stack.last().expect("Empty stack");
                let ptext = &new_top.0;
                self.max_width = cmp::max(self.max_width, ptext.width);
                self.max_height = cmp::max(self.max_height, ptext.height);
            }

        } else {

            // Filter old fades
            let mut oldfades = take(&mut self.fades);
            for fade in oldfades.drain(..) {
                if now.duration_since(fade.2) < Duration::from_secs(2) {
                    self.fades.push(fade);
                }
            }

            // Push new fade if pc changed.
            if new_top.1 != old_top.1 {
                self.fades.push(Fade(old_top.1, highlight, now));
            }
        }

        if let Some(c) = output {
            let max_width = self.max_width as f32 * screen.large_font.0;
            let max_chars = (max_width / screen.small_font.0).floor() as usize;
            self.output.push_str(&c);
            let count = self.output.chars().count();
            if count > max_chars {
                let mut chars = self.output.chars();
                for _ in 0..count - max_chars {
                    chars.next();
                }
                self.output = chars.collect();
            }
        }

    }

    fn dead(&mut self, now: Instant, kill: bool) -> bool {
        if now > self.scroll.end {
            return true;
        }
        if !kill {
            return false;
        }
        match self.ended {
            None => {
                self.ended = Some(now);
                return false;
            },
            Some(past) => {
                return now.duration_since(past) > Duration::from_secs(2);
            }
        }
    }

    fn bounds(&self, screen: &ScreenSettings, now: Instant) -> Option<Bounds> {
        let pos : (f32, f32) = match self.scroll.pos(now) {
            None => return None,
            Some(p) => p,
        };

        let Dimensions(w, h) = screen.dims;
        let w = w as f32;
        let h = h as f32;

        let lft = pos.0 - 4.;
        let lft = if lft < 0. { 0. } else { lft };
        let top = pos.1 - 2.;
        let top = if top < 0. { 0. } else { top };
        let rgt = pos.0 + self.max_width as f32 * screen.large_font.0 + 4.;
        let rgt = if rgt > w { w } else { rgt };
        let bot = pos.1 + self.max_height as f32 * screen.large_font.1 +
                    2. * screen.small_font.1 + 2.;
        let bot = if bot > h { h } else { bot };

        Some(Bounds::new(top, lft, bot, rgt))
    }

}

impl<'a> Animated {
    fn animate<B,C>(&'a self, ctxt: &mut C,
                    layer: &mut Layer<B>,
                    rear_brush: &mut GlyphBrush<B>,
                    screen: &ScreenSettings, depth: f32,
                    now: Instant) -> f32
    where
        C: GraphicsContext<Backend = B>,
        [[f32; 4]; 4]: backend::shader::Uniformable<B>,
        TextureBinding<Dim2, NormUnsigned>: backend::shader::Uniformable<B>,
        B: backend::tess::Tess<Vertex, (), (), Interleaved>
           + backend::tess::Tess<(), (), (), Interleaved>
           + backend::tess::Tess<(), u32, Instance, Interleaved>
           + backend::pipeline::PipelineTexture<Dim2, NormR8UI>
    {

        let pos : (f32, f32) = match self.scroll.pos(now) {
            None => return depth,
            Some(p) => p,
        };

        let dims = screen.dims;
        let alpha = match self.ended {
            None => 1.,
            Some(e) => {
                let age = now.duration_since(e).as_secs_f32();
                1. - (age / 2.)
            }
        };

        let mut depth = depth;
        let mut stack_iter = self.call_stack.iter().rev().take(4);
        let last = stack_iter.next().expect("Failed to get top of stack");
        let ptext = &last.0;
        let body_top = pos.1 + screen.small_font.1;
        let stack_items : Vec<&(Rc<ProgText>, usize)> =
            stack_iter.rev().collect();

        let mut stack_alpha = alpha * alpha *
            1. / 2_i32.pow(stack_items.len() as u32) as f32;
        let mut stack_depth = 0.999;
        for (pt, _pc) in stack_items {
            rear_brush.queue(Section::default()
                .with_screen_position((pos.0, body_top))
                .add_text(pt.text(screen, stack_alpha, stack_depth)));
            
            stack_depth -= 0.001;
            stack_alpha *= 2.;
        }

        depth -= 0.001;
        let bnds = self.bounds(screen, now).expect("Can't get bounds.");

        let l = dims.project_x(bnds.lft);
        let r = dims.project_x(bnds.rgt);
        let t = dims.project_y(bnds.top);
        let b = dims.project_y(bnds.bot);
        let bg = match &self.played {
            Some(Fade(_, rgba, start)) => {
                let age = now.duration_since(*start).as_secs_f32();
                let mul = 0.5 * (2. - age);
                rgba.rgb_mul(mul * mul)
            },
            _ => VertexRGBA::new([0.0, 0.0, 0.0, 1.0])
        }.with_alpha(0.5 * alpha);
        let vtxs = [Vertex::new(VertexPosition::new([l, t, depth]), bg),
                    Vertex::new(VertexPosition::new([r, t, depth]), bg),
                    Vertex::new(VertexPosition::new([r, b, depth]), bg),
                    Vertex::new(VertexPosition::new([l, b, depth]), bg)];
        layer.boxes.push(ctxt.new_tess()
                .set_vertices(vtxs)
                .set_mode(Mode::TriangleFan)
                .build()
                .expect("Failed to make background."));
        let mut draw_highlight = |p: usize, hl: VertexRGBA, d: f32| {
            let pc_y = p / ptext.width;
            let pc_x = p % ptext.width;

            let (left_px, top_px) =
                    screen.large_font.project(pos.0, body_top,
                                              pc_x as f32, pc_y as f32);
            let (right_px, bot_px) =
                    screen.large_font.project(pos.0, body_top,
                                              (1 + pc_x) as f32,
                                              (1 + pc_y) as f32);

            let l = dims.project_x(left_px);
            let r = dims.project_x(right_px);
            let t = dims.project_y(top_px);
            let b = dims.project_y(bot_px);
            let vtxs = [Vertex::new(VertexPosition::new([l, t, d]), hl),
                        Vertex::new(VertexPosition::new([r, t, d]), hl),
                        Vertex::new(VertexPosition::new([r, b, d]), hl),
                        Vertex::new(VertexPosition::new([l, b, d]), hl)];
            layer.boxes.push(ctxt.new_tess()
                .set_vertices(vtxs)
                .set_mode(Mode::TriangleFan)
                .build()
                .expect("Failed to make highlight."));
        };
        
        for Fade(fade_pc, hl, start) in &self.fades {
            depth -= 0.001;
            let age = now.duration_since(*start).as_secs_f32();
            if age > 2. { continue }
            let alpha = 0.45 * (2. - age);
            let alpha = alpha * alpha;
            draw_highlight(*fade_pc, hl.with_alpha(alpha), depth);
        }

        depth -= 0.001;
        let hl = screen.highlight.rgb_shift(self.pid).with_alpha(alpha);
        draw_highlight(last.1, hl, depth);
        depth -= 0.001;

        layer.brush.queue(Section::default()
            .with_screen_position(pos)
            .add_text(Text::new(&self.label)
                .with_z(depth)
                .with_color([0., 1., 0., alpha])
                .with_scale(screen.small_font.1)));

        let body_top = pos.1 + screen.small_font.1;
        layer.brush.queue(Section::default()
            .with_screen_position((pos.0, body_top))
            .add_text(ptext.text(screen, alpha, depth)));

        let output_top = body_top + screen.large_font.1 *
            self.max_height as f32;
        layer.brush.queue(Section::default()
            .with_screen_position((pos.0, output_top))
            .add_text(Text::new(&self.output)
                .with_z(depth)
                .with_color([0., 0., 1., alpha])
                .with_scale(screen.small_font.1)));

        depth
    }
}

struct Animator {
    names: HashSet<Rc<str>>,
    ptexts: HashMap<(usize, String), Rc<ProgText>>,
    anims: Vec<Animated>,
}

impl Animator {
    fn new() -> Self {
        Animator {
            names : HashSet::new(),
            ptexts : HashMap::new(),
            anims : Vec::new(),
        }
    }

    fn update(&mut self, mut state: EngineState, screen: &ScreenSettings) {
        let now = Instant::now();

        let mut oldpt = take(&mut self.ptexts);
        let mut ptvec = Vec::new();
        for tup in state.progs.drain(..) {
            if let Some(pt) = oldpt.remove(&tup) {
                ptvec.push(Rc::clone(&pt));
                self.ptexts.insert(tup, pt);
            } else {
                let (w, b) = &tup;
                let pt = Rc::new(ProgText::new(b, *w));
                ptvec.push(Rc::clone(&pt));
                self.ptexts.insert(tup, pt);
            }
        }

        let mut oldnames = take(&mut self.names);
        let mut namevec = Vec::new();
        for name in state.names.drain(..) {
            let rcname = Rc::from(name);
            if let Some(n) = oldnames.take(&rcname) {
                namevec.push(Rc::clone(&n));
                self.names.insert(n);
            } else {
                namevec.push(Rc::clone(&rcname));
                self.names.insert(rcname);
            }
        }

        let mut oldanim = take(&mut self.anims);
        for mut anim in oldanim.drain(..) {
            match state.procs.remove(&anim.pid) {
                None => {
                    if ! anim.dead(now, true) {
                        self.anims.push(anim);
                    }
                },
                Some(mut proc) => {
                    if anim.dead(now, false) {
                        continue;
                    }
                    let mut call_stack = Vec::new();
                    for (pt, pc) in proc.call_stack.drain(..) {
                        call_stack.push((Rc::clone(&ptvec[pt]), pc));
                    }
                    anim.update(call_stack, proc.output, proc.play.is_some(),
                                now, screen);
                    self.anims.push(anim);
                }
            }
        }

        let mut rng = rand::thread_rng();
        for (pid, mut proc) in state.procs.drain() {
            if self.anims.len() >= screen.proc_limit {
                break;
            }
            let mut call_stack = Vec::new();
            let mut max_width = 0;
            for (pt, pc) in proc.call_stack.drain(..) {
                let ptext = &ptvec[pt];
                call_stack.push((Rc::clone(ptext), pc));
                max_width = cmp::max(max_width, ptext.width);
            }

            let sx = screen.dims.0 as f32;
            let sy = screen.dims.1 as f32;
            let minx = -(screen.large_font.0 * max_width as f32);
            let (start_x, end_x) = if rng.gen() {
                (minx, sx)
            } else {
                (sx, minx)
            };
            let start_y : f32 = sy * rng.gen_range(-0.1, 0.9);
            let end_y : f32 = sy * rng.gen_range(-0.1, 0.9);

            let scroll = Scroll::new((start_x, start_y), (end_x, end_y), now,
                                     screen.scroll_speed as f32 +
                                     rng.gen_range(0., 5.));

            self.anims.push(Animated::new(pid, Rc::clone(&namevec[proc.name]),
                                          scroll, call_stack));
            break;
        }

    }
}

fn get_layer<'a, C, B>(ctxt: &mut C, layers: &'a mut Vec<Layer<B>>,
                       brush_cache: &mut Vec<GlyphBrush<B>>, font: &FontArc,
                       bounds: &Bounds) -> &'a mut Layer<B>
    where
        C: GraphicsContext<Backend = B>,
        [[f32; 4]; 4]: backend::shader::Uniformable<B>,
        TextureBinding<Dim2, NormUnsigned>: backend::shader::Uniformable<B>,
        B: backend::tess::Tess<Vertex, (), (), Interleaved>
         + backend::tess::Tess<(), (), (), Interleaved>
         + backend::texture::Texture<Dim2, NormR8UI>
         + backend::shader::Shader
         + backend::tess::Tess<(), u32, Instance, Interleaved>
         + backend::pipeline::PipelineBase
         + backend::pipeline::PipelineTexture<Dim2, NormR8UI>
         + backend::render_gate::RenderGate
         + backend::tess_gate::TessGate<(), u32, Instance,
                                                 Interleaved>
{
    let mut best = None;
    for i in (0..layers.len()).rev() {
        if bounds.overlaps(&layers[i].bounds) {
            break;
        }
        best = Some(i);
    }

    let i = match best {
        None => {
            let br = brush_cache.pop().unwrap_or_else(||
                        GlyphBrushBuilder::using_font(font.clone())
                                          .build(ctxt));
            layers.push(Layer::new(br));
            layers.len() - 1
        }
        Some(i) => i
    };

    layers[i].bounds.push(*bounds);
    &mut layers[i]
}

impl<'a> Animator {
    fn animate<B,C>(&'a self, ctxt: &mut C,
                    layers: &mut Vec<Layer<B>>,
                    brush_cache: &mut Vec<GlyphBrush<B>>,
                    rear_brush: &mut GlyphBrush<B>,
                    font: &FontArc,
                    screen: &ScreenSettings,
                    now: Instant)
    where
        C: GraphicsContext<Backend = B>,
        [[f32; 4]; 4]: backend::shader::Uniformable<B>,
        TextureBinding<Dim2, NormUnsigned>: backend::shader::Uniformable<B>,
        B: backend::tess::Tess<Vertex, (), (), Interleaved>
         + backend::tess::Tess<(), (), (), Interleaved>
         + backend::texture::Texture<Dim2, NormR8UI>
         + backend::shader::Shader
         + backend::tess::Tess<(), u32, Instance, Interleaved>
         + backend::pipeline::PipelineBase
         + backend::pipeline::PipelineTexture<Dim2, NormR8UI>
         + backend::render_gate::RenderGate
         + backend::tess_gate::TessGate<(), u32, Instance,
                                                 Interleaved>
    {

        let mut depth = 0.900;
        for anim in &self.anims {
            let bounds = match anim.bounds(screen, now) {
                Some(b) => b,
                _ => continue
            };
            depth -= 0.001;
            let layer = get_layer(ctxt, layers, brush_cache,
                                  font, &bounds);
            depth = anim.animate(ctxt, layer, rear_brush, screen, depth, now);
        }

    }
}

#[derive(Copy, Clone, Debug)]
struct Bounds {
    top : f32,
    lft : f32,
    bot : f32,
    rgt : f32
}

impl Bounds {
    fn new(top: f32, lft: f32, bot: f32, rgt: f32) -> Self {
        Bounds {
            top: top
           ,lft: lft
           ,bot: bot
           ,rgt: rgt
        }
    }

    fn overlap(&self, other: &Bounds) -> bool {
        !(self.top >= other.bot || self.bot <= other.top ||
          self.lft >= other.rgt || self.rgt <= other.lft)
    }

    fn overlaps<'a, I>(&self, others: I) -> bool
        where I: IntoIterator<Item = &'a Bounds>
    {
        for b in others {
            if self.overlap(b) { 
                return true
            }
        }

        false
    }
}

struct Layer<B>
    where
        [[f32; 4]; 4]: backend::shader::Uniformable<B>,
        TextureBinding<Dim2, NormUnsigned>: backend::shader::Uniformable<B>,
        B: ?Sized + backend::tess::Tess<Vertex, (), (), Interleaved>
                  + backend::tess::Tess<(), (), (), Interleaved>
                  + backend::texture::Texture<Dim2, NormR8UI>
                  + backend::shader::Shader
                  + backend::tess::Tess<(), u32, Instance, Interleaved>
                  
{
    brush : GlyphBrush<B>
   ,boxes : Vec<Tess<B, Vertex>>
   ,bounds : Vec<Bounds>
}

impl<B> Layer<B>
    where
        [[f32; 4]; 4]: backend::shader::Uniformable<B>,
        TextureBinding<Dim2, NormUnsigned>: backend::shader::Uniformable<B>,
        B: ?Sized + backend::tess::Tess<Vertex, (), (), Interleaved>
                  + backend::tess::Tess<(), (), (), Interleaved>
                  + backend::texture::Texture<Dim2, NormR8UI>
                  + backend::shader::Shader
                  + backend::tess::Tess<(), u32, Instance, Interleaved>
{
    fn new(brush : GlyphBrush<B>) -> Self {
        Layer {
            brush: brush
           ,boxes: Vec::new()
           ,bounds: Vec::new()
        }
    }

}

const ICO_16: &[u8; 1024] = include_bytes!("icons/icon16.dat");
const ICO_32: &[u8; 4096] = include_bytes!("icons/icon32.dat");
const ICO_128: &[u8; 65536] = include_bytes!("icons/icon128.dat");

fn ico_to_vec(bytes: &[u8], len: usize) -> Vec<u32> {
    let mut vec = Vec::new();
    for i in (0..len).step_by(4) {
        vec.push(u32::from_le_bytes(
            bytes[i..i+4].try_into().expect("count not read icon.")));
    }
    return vec
}

fn icons() -> Vec<PixelImage> {
    let mut vec = Vec::new();
    vec.push(PixelImage { width : 16, height : 16,
                          pixels : ico_to_vec(ICO_16, ICO_16.len())});
    vec.push(PixelImage { width : 32, height : 32,
                          pixels : ico_to_vec(ICO_32, ICO_32.len())});
    vec.push(PixelImage { width : 128, height : 128,
                          pixels : ico_to_vec(ICO_128, ICO_128.len())});

    vec
}

fn main() {

    let baseuri = read_args();

    let client = FungeClient::new(&baseuri);
    let sleep_dur = Duration::from_millis(5);

    let mut width = 640;
    let mut height = 480;
    let mut surface = GlfwSurface::new_gl33(
        "nfviewergl",
        WindowOpt::default()
            .set_num_samples(2)
            .set_dim(WindowDim::Windowed {
                width: width,
                height: height,
            })).expect("GLFW surface creation failed.");

    let mut dims = Dimensions::new(width, height);
    let mut errbar = ErrBar::new(dims);

    let font = ab_glyph::FontArc::try_from_slice(
                    include_bytes!("DejaVuSansMono.ttf")
                ).expect("Failed to load font.");
    let mut glyph_brush = GlyphBrushBuilder::using_font(font.clone())
                                            .build(&mut surface);

    let mut rear_brush = GlyphBrushBuilder::using_font(font.clone())
                                           .build(&mut surface);
    let mut program = surface
            .new_shader_program::<VertexSemantics, (), ()>()
            .from_strings(VS_STR, None, None, FS_STR)
            .unwrap()
            .ignore_warnings();

    surface.window.set_icon_from_pixels(icons());

    let mut beat = 0;

    let mut animator = Animator::new();

    let mut fps = false;

    let rs = RenderState::default()
        .set_blending(Blending {
            equation: Equation::Additive,
            src: Factor::SrcAlpha,
            dst: Factor::SrcAlphaComplement });

    let mut bigfont = 28.;
    let mut screen = ScreenSettings::new(&font, dims, bigfont,
                                         bigfont * 2. / 3.,
                                         VertexRGBA::new([0., 0., 1., 1.]),
                                         10, 10);

    let mut brush_cache = Vec::new();
    let start = Instant::now();
    let mut frames :u64 = 0;
    'outer: loop {

        surface.window.glfw.poll_events();

        for (_, event) in surface.events_rx.try_iter() {
            match event {
                WindowEvent::Close | WindowEvent::Key(Key::Escape, _,
                                                      Action::Release, _) => {
                    break 'outer
                }

                WindowEvent::Key(Key::F, _, Action::Press, _) => {
                    fps = !fps;
                }

                WindowEvent::Key(Key::Equal, _, Action::Press, _) => {
                    bigfont += 4.;
                    screen.update_large_font(bigfont);
                    screen.update_small_font(bigfont * 2. / 3.);
                }

                WindowEvent::Key(Key::Minus, _, Action::Press, _) => {
                    bigfont -= 4.;
                    screen.update_large_font(bigfont);
                    screen.update_small_font(bigfont * 2. / 3.);
                }

                WindowEvent::Key(Key::Up, _, Action::Press, _) => {
                    screen.scroll_speed = cmp::max(4, screen.scroll_speed - 2);
                }

                WindowEvent::Key(Key::Down, _, Action::Press, _) => {
                    screen.scroll_speed += 2;
                }

                // Handle window resizing.
                WindowEvent::FramebufferSize(new_width, new_height) => {
                    print!("New size {}x{}\n", new_width, new_height);
                    width = new_width as u32;
                    height = new_height as u32;
                    dims = Dimensions(width, height);
                    screen.rescale(dims);
                    errbar.resize(dims);
                }

                _ => {}
            }
        }

        let now = Instant::now();
        let dur = now.duration_since(start).as_secs_f32();
        screen.update_highlight(VertexRGBA::new(
            [(((dur / 4.1).cos() + 1.) / 2.),
             ((dur.cos() + 1.) / 2.),
             (((dur + 3.14).cos() + 1.) / 2.),
             1.]));

        let mut tess_queue = Vec::new();

        tess_queue.push(errbar.make_bar(&mut surface));
        let back_buffer = surface.back_buffer().unwrap();

        let beat_str = format!("{}", beat);
        glyph_brush.queue(Section::default()
            .with_screen_position((0.,height as f32 - 20.))
            .add_text(
                Text::default()
                    .with_text(&beat_str)
                    .with_color([1., 1., 1., 1.])
                    .with_scale(20.)));

        glyph_brush.queue(errbar.make_text());

        let mut layers = Vec::new();

        animator.animate(&mut surface, &mut layers, &mut brush_cache,
                         &mut rear_brush, &font, &screen, now);

        rear_brush.process_queued(&mut surface);
        for l in layers.iter_mut() {
            l.brush.process_queued(&mut surface);
        }
        glyph_brush.process_queued(&mut surface);

        let render = surface.new_pipeline_gate().pipeline(
            &back_buffer,
            &PipelineState::default().set_clear_color([0.0, 0.0, 0.0, 1.]),
            |mut pipeline, mut shd_gate| {
                rear_brush.draw_queued(&mut pipeline, &mut shd_gate,
                                       width as u32, height as u32)?;
                for mut l in layers.drain(..) {
                    shd_gate.shade(&mut program, |_, _, mut rdr_gate| {
                        rdr_gate.render(&rs, |mut tess_gate| {
                            for t in &l.boxes {
                                tess_gate.render(t)?;
                            }
                            Ok(())
                        })
                    })?;
                    l.brush.draw_queued(&mut pipeline, &mut shd_gate,
                                        width as u32, height as u32)?;
                }
                shd_gate.shade(&mut program, |_, _, mut rdr_gate| {
                    rdr_gate.render(&rs, |mut tess_gate| {
                        for t in tess_queue.iter().rev() {
                            tess_gate.render(t)?;
                        }
                        Ok(())
                    })
                })?;
                glyph_brush.draw_queued(&mut pipeline, &mut shd_gate,
                                        width as u32, height as u32)
            },
        );

        for l in layers.drain(..).rev() {
            brush_cache.push(l.brush);
        }
        render.assume().into_result().expect("Render failed.");
        surface.window.swap_buffers();
        frames += 1;

        match client.get_state(sleep_dur) {
            None => {},
            Some(Ok(st)) => {
                //print!("state: {:?}\n", &st);
                for (pid, msg) in &st.crashed {
                    errbar.push_err(&format!("{:X}: {:?}", pid, msg));
                }
                beat = st.beat;
                animator.update(st, &screen);
                if fps {
                    print!("{} FPS\n",frames as f32 / dur);
                }
            },
            Some(Err(s)) => {
                errbar.push_err(&format!("client_err: {}\n", &s));
            }
        }
    }

}
