//! Graded writing pad used by the "Write" review mode.
//!
//! The prompt (pinyin + meaning) lives in the review card. This widget owns the
//! canvas, stroke validation, hints, and the per-character progression of a
//! word. It deliberately does not show the character's outline: the learner has
//! to produce it from memory.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::cairo;
use gtk::glib;
use gtk::prelude::*;

use crate::db::VocabEntry;
use crate::strokes::{self, Point, StrokeData, ViewTransform};

const STROKE_SECONDS: f64 = 0.45;
const CANVAS_SIZE: i32 = 300;
const CANVAS_PADDING: f64 = 6.0;

struct Anim {
    id: u64,
    start: Instant,
    stroke: usize,
}

struct PadState {
    chars: Vec<(char, StrokeData)>,
    word: String,
    index: usize,
    completed: usize,
    input: Vec<Point>,
    mistakes: u32,
    hints: u32,
    error_stroke: Option<usize>,
    anim: Option<Anim>,
    anim_serial: u64,
    finished: bool,
    feedback: String,
}

impl PadState {
    fn empty() -> Self {
        Self {
            chars: Vec::new(),
            word: String::new(),
            index: 0,
            completed: 0,
            input: Vec::new(),
            mistakes: 0,
            hints: 0,
            error_stroke: None,
            anim: None,
            anim_serial: 0,
            finished: false,
            feedback: String::new(),
        }
    }

    fn reset_for_entry(&mut self, chars: Vec<(char, StrokeData)>, word: String) {
        self.chars = chars;
        self.word = word;
        self.index = 0;
        self.completed = 0;
        self.input.clear();
        self.mistakes = 0;
        self.hints = 0;
        self.error_stroke = None;
        self.anim = None;
        self.finished = false;
        self.feedback = if self.chars.is_empty() {
            "No stroke data for this character.".to_string()
        } else {
            "Write the character from memory.".to_string()
        };
    }
}

type FinishedCallback = Rc<dyn Fn(bool)>;

pub struct WritingPad {
    root: gtk::Box,
    area: gtk::DrawingArea,
    status: gtk::Label,
    written: gtk::Label,
    feedback: gtk::Label,
    state: RefCell<PadState>,
    on_finished: RefCell<Option<FinishedCallback>>,
}

impl WritingPad {
    pub fn new() -> Rc<Self> {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
        root.set_halign(gtk::Align::Center);
        root.set_margin_top(2);

        let status = gtk::Label::new(None);
        status.add_css_class("dim-label");
        status.set_halign(gtk::Align::Center);

        let written = gtk::Label::new(None);
        written.add_css_class("written-word");
        written.set_halign(gtk::Align::Center);

        let canvas_holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
        canvas_holder.add_css_class("stroke-card");
        canvas_holder.set_halign(gtk::Align::Center);
        canvas_holder.set_margin_top(2);

        let area = gtk::DrawingArea::new();
        area.set_content_width(CANVAS_SIZE);
        area.set_content_height(CANVAS_SIZE);
        area.set_cursor_from_name(Some("crosshair"));
        canvas_holder.append(&area);

        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        controls.set_halign(gtk::Align::Center);
        let hint_button = gtk::Button::with_label("Hint");
        hint_button.add_css_class("flat");
        let answer_button = gtk::Button::with_label("Show answer");
        answer_button.add_css_class("flat");
        let clear_button = gtk::Button::with_label("Clear");
        clear_button.add_css_class("flat");
        controls.append(&hint_button);
        controls.append(&answer_button);
        controls.append(&clear_button);

        let feedback = gtk::Label::new(None);
        feedback.add_css_class("dim-label");
        feedback.set_halign(gtk::Align::Center);
        feedback.set_justify(gtk::Justification::Center);
        feedback.set_wrap(true);
        feedback.set_max_width_chars(38);

        root.append(&status);
        root.append(&written);
        root.append(&canvas_holder);
        root.append(&controls);
        root.append(&feedback);

        let pad = Rc::new(Self {
            root,
            area: area.clone(),
            status,
            written,
            feedback,
            state: RefCell::new(PadState::empty()),
            on_finished: RefCell::new(None),
        });

        {
            let weak = Rc::downgrade(&pad);
            area.set_draw_func(move |_, cr, width, height| {
                if let Some(pad) = weak.upgrade() {
                    pad.draw(cr, width, height);
                }
            });
        }

        {
            let weak = Rc::downgrade(&pad);
            let drag = gtk::GestureDrag::new();
            drag.set_button(0);
            drag.connect_drag_begin(move |_, x, y| {
                if let Some(pad) = weak.upgrade() {
                    pad.drag_begin(x, y);
                }
            });
            let weak = Rc::downgrade(&pad);
            drag.connect_drag_update(move |gesture, offset_x, offset_y| {
                let Some((start_x, start_y)) = gesture.start_point() else {
                    return;
                };
                if let Some(pad) = weak.upgrade() {
                    pad.drag_update(start_x + offset_x, start_y + offset_y);
                }
            });
            let weak = Rc::downgrade(&pad);
            drag.connect_drag_end(move |_, _, _| {
                if let Some(pad) = weak.upgrade() {
                    pad.validate();
                }
            });
            area.add_controller(drag);
        }

        {
            let weak = Rc::downgrade(&pad);
            hint_button.connect_clicked(move |_| {
                if let Some(pad) = weak.upgrade() {
                    pad.hint();
                }
            });
        }
        {
            let weak = Rc::downgrade(&pad);
            answer_button.connect_clicked(move |_| {
                if let Some(pad) = weak.upgrade() {
                    pad.show_answer();
                }
            });
        }
        {
            let weak = Rc::downgrade(&pad);
            clear_button.connect_clicked(move |_| {
                if let Some(pad) = weak.upgrade() {
                    pad.clear();
                }
            });
        }

        pad.render();
        pad
    }

    pub fn root(&self) -> &gtk::Box {
        &self.root
    }

    pub fn set_on_finished(&self, callback: FinishedCallback) {
        *self.on_finished.borrow_mut() = Some(callback);
    }

    pub fn set_entry(&self, entry: &VocabEntry) {
        let mut chars = Vec::new();
        for character in entry.simplified.chars() {
            if let Some(data) = StrokeData::load(character) {
                chars.push((character, data));
            }
        }
        let word: String = chars.iter().map(|(character, _)| *character).collect();

        self.state.borrow_mut().reset_for_entry(chars, word);
        self.render();
    }

    pub fn hint(self: &Rc<Self>) {
        let mut state = self.state.borrow_mut();
        if state.finished || state.anim.is_some() || state.chars.is_empty() {
            return;
        }
        let stroke = state.completed;
        let count = state.chars[state.index].1.stroke_count();
        if stroke >= count {
            return;
        }

        state.hints += 1;
        state.error_stroke = None;
        state.anim_serial += 1;
        let id = state.anim_serial;
        state.anim = Some(Anim {
            id,
            start: Instant::now(),
            stroke,
        });
        state.feedback = "Hint: watch this stroke.".to_string();
        drop(state);

        self.render();
        let weak = Rc::downgrade(self);
        glib::timeout_add_local(Duration::from_millis(16), move || {
            let Some(pad) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            pad.tick(id)
        });
    }

    pub fn show_answer(&self) {
        let mut state = self.state.borrow_mut();
        if state.finished {
            return;
        }
        state.finished = true;
        state.anim = None;
        state.feedback = "Answer shown — this card will come back sooner.".to_string();
        drop(state);

        self.render();
        self.emit_finished(false);
    }

    pub fn clear(&self) {
        let mut state = self.state.borrow_mut();
        if state.finished {
            return;
        }
        state.completed = 0;
        state.input.clear();
        state.error_stroke = None;
        state.anim = None;
        state.feedback = "Write the character from memory.".to_string();
        drop(state);
        self.render();
    }

    fn tick(self: &Rc<Self>, id: u64) -> glib::ControlFlow {
        let mut state = self.state.borrow_mut();
        let Some(anim) = &state.anim else {
            return glib::ControlFlow::Break;
        };
        if anim.id != id {
            return glib::ControlFlow::Break;
        }
        if anim.start.elapsed().as_secs_f64() < STROKE_SECONDS {
            drop(state);
            self.area.queue_draw();
            return glib::ControlFlow::Continue;
        }
        state.anim = None;
        state.feedback = "Now write it yourself.".to_string();
        drop(state);
        self.render();
        glib::ControlFlow::Break
    }

    fn drag_begin(&self, x: f64, y: f64) {
        let mut state = self.state.borrow_mut();
        if state.finished || state.anim.is_some() || state.chars.is_empty() {
            return;
        }
        if state.completed >= state.chars[state.index].1.stroke_count() {
            return;
        }

        let transform = view(self.area.width(), self.area.height());
        state.input.clear();
        state.input.push(transform.to_data(x, y));
        state.error_stroke = None;
        state.feedback = "Write the stroke…".to_string();
        drop(state);
        self.area.queue_draw();
        self.feedback.set_text("Write the stroke…");
    }

    fn drag_update(&self, x: f64, y: f64) {
        let mut state = self.state.borrow_mut();
        if state.finished || state.anim.is_some() {
            return;
        }

        let transform = view(self.area.width(), self.area.height());
        let point = transform.to_data(x, y);
        if let Some(last) = state.input.last() {
            if strokes::distance(*last, point) < 4.0 {
                return;
            }
        }
        state.input.push(point);
        drop(state);
        self.area.queue_draw();
    }

    fn validate(&self) {
        let mut state = self.state.borrow_mut();
        if state.finished || state.anim.is_some() || state.chars.is_empty() {
            state.input.clear();
            return;
        }

        if state.input.len() < 2 || state.completed >= state.chars[state.index].1.stroke_count() {
            state.input.clear();
            drop(state);
            self.area.queue_draw();
            return;
        }

        let accepted = strokes::stroke_matches(
            &state.input,
            &state.chars[state.index].1.medians[state.completed],
        );
        state.input.clear();

        if accepted {
            state.completed += 1;
            state.error_stroke = None;
            state.feedback = format!(
                "Good — stroke {} of {}.",
                state.completed,
                state.chars[state.index].1.stroke_count()
            );
        } else {
            state.mistakes += 1;
            state.error_stroke = Some(state.completed);
            state.feedback = "Not quite — try that stroke again.".to_string();
        }
        drop(state);

        self.area.queue_draw();
        self.render();
        self.advance_if_complete();
    }

    fn advance_if_complete(&self) {
        let mut state = self.state.borrow_mut();
        if state.finished || state.chars.is_empty() {
            return;
        }
        let count = state.chars[state.index].1.stroke_count();
        if state.completed < count {
            return;
        }

        if state.index + 1 < state.chars.len() {
            state.index += 1;
            state.completed = 0;
            state.input.clear();
            state.feedback = "Good — next character.".to_string();
            drop(state);
            self.render();
            return;
        }

        state.finished = true;
        let correct = state.mistakes == 0 && state.hints == 0;
        state.feedback = if correct {
            "Correct! No mistakes.".to_string()
        } else {
            "Finished — this card will come back sooner.".to_string()
        };
        drop(state);

        self.render();
        self.emit_finished(correct);
    }

    fn emit_finished(&self, correct: bool) {
        let callback = self.on_finished.borrow().clone();
        if let Some(callback) = callback {
            callback(correct);
        }
    }

    fn render(&self) {
        let state = self.state.borrow();
        if state.chars.is_empty() {
            self.status.set_text("No stroke data");
            self.written.set_text("");
            self.feedback.set_text(&state.feedback);
            drop(state);
            self.area.queue_draw();
            return;
        }

        let (_, data) = &state.chars[state.index];
        if state.finished {
            self.status
                .set_text(&format!("{} characters", state.chars.len()));
        } else {
            self.status.set_text(&format!(
                "Character {} of {} · stroke {} of {}",
                state.index + 1,
                state.chars.len(),
                state.completed,
                data.stroke_count()
            ));
        }

        let skeleton = state
            .word
            .chars()
            .enumerate()
            .map(|(index, character)| {
                if state.finished || index < state.index {
                    character.to_string()
                } else {
                    "＿".to_string()
                }
            })
            .collect::<Vec<_>>()
            .join(" ");
        self.written.set_text(&skeleton);
        self.feedback.set_text(&state.feedback);

        drop(state);
        self.area.queue_draw();
    }

    fn draw(&self, cr: &cairo::Context, width: i32, height: i32) {
        let state = self.state.borrow();
        if state.chars.is_empty() {
            return;
        }

        let (_, data) = &state.chars[state.index];
        let transform = view(width, height);
        let ink = self.area.style_context().color();
        let accent = theme_color(&self.area, "accent_bg_color", &ink);
        let error = theme_color(&self.area, "error_bg_color", &ink);

        let mut active = None;
        if let Some(anim) = &state.anim {
            let elapsed = anim.start.elapsed().as_secs_f64();
            if elapsed < STROKE_SECONDS {
                active = Some((anim.stroke, (elapsed / STROKE_SECONDS).clamp(0.0, 1.0)));
            }
        }

        let _ = cr.save();
        cr.set_matrix(transform.matrix());

        for index in 0..data.stroke_count() {
            if let Some((active_index, _)) = active {
                if index == active_index {
                    continue;
                }
            }
            if index >= state.completed && state.error_stroke != Some(index) {
                continue;
            }

            let color = if state.error_stroke == Some(index) {
                error
            } else {
                ink
            };
            set_source(cr, color);
            data.append_stroke(cr, index);
            let _ = cr.fill();
        }

        if let Some((index, progress)) = active {
            if progress >= 0.995 {
                set_source(cr, accent);
                data.append_stroke(cr, index);
                let _ = cr.fill();
            } else {
                draw_partial(cr, data, index, progress, accent, transform, width, height);
            }
        }

        if state.input.len() >= 2 {
            set_source(cr, accent);
            cr.set_line_width(16.0);
            cr.set_line_cap(cairo::LineCap::Round);
            cr.set_line_join(cairo::LineJoin::Round);
            cr.move_to(state.input[0].x, state.input[0].y);
            for point in &state.input[1..] {
                cr.line_to(point.x, point.y);
            }
            let _ = cr.stroke();
        } else if let Some(point) = state.input.first() {
            set_source(cr, accent);
            cr.arc(point.x, point.y, 9.0, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }

        let _ = cr.restore();
    }
}

fn view(width: i32, height: i32) -> ViewTransform {
    ViewTransform::new(width.max(1) as f64, height.max(1) as f64, CANVAS_PADDING)
}

fn theme_color(widget: &gtk::DrawingArea, name: &str, fallback: &gtk::gdk::RGBA) -> gtk::gdk::RGBA {
    widget
        .style_context()
        .lookup_color(name)
        .unwrap_or(*fallback)
}

fn set_source(cr: &cairo::Context, color: gtk::gdk::RGBA) {
    cr.set_source_rgba(
        color.red() as f64,
        color.green() as f64,
        color.blue() as f64,
        color.alpha() as f64,
    );
}

#[allow(clippy::too_many_arguments)]
fn draw_partial(
    cr: &cairo::Context,
    data: &StrokeData,
    index: usize,
    progress: f64,
    color: gtk::gdk::RGBA,
    transform: ViewTransform,
    width: i32,
    height: i32,
) {
    let width = width.max(1);
    let height = height.max(1);

    let Ok(outline_mask) = cairo::ImageSurface::create(cairo::Format::A8, width, height) else {
        return;
    };
    let Ok(outline_cr) = cairo::Context::new(&outline_mask) else {
        return;
    };
    outline_cr.set_matrix(transform.matrix());
    outline_cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
    data.append_stroke(&outline_cr, index);
    let _ = outline_cr.fill();

    let Ok(stroke_mask) = cairo::ImageSurface::create(cairo::Format::A8, width, height) else {
        return;
    };
    let Ok(stroke_cr) = cairo::Context::new(&stroke_mask) else {
        return;
    };
    stroke_cr.set_matrix(transform.matrix());
    stroke_cr.set_line_width(190.0);
    stroke_cr.set_line_cap(cairo::LineCap::Round);
    stroke_cr.set_line_join(cairo::LineJoin::Round);
    stroke_cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);

    let prefix = strokes::prefix(&data.medians[index], progress);
    if let Some(first) = prefix.first() {
        stroke_cr.move_to(first.x, first.y);
        for point in &prefix[1..] {
            stroke_cr.line_to(point.x, point.y);
        }
        let _ = stroke_cr.stroke();
    }

    stroke_cr.identity_matrix();
    stroke_cr.set_operator(cairo::Operator::In);
    let _ = stroke_cr.set_source_surface(&outline_mask, 0.0, 0.0);
    let _ = stroke_cr.paint();
    outline_mask.flush();
    stroke_mask.flush();

    let _ = cr.save();
    cr.identity_matrix();
    set_source(cr, color);
    let _ = cr.mask_surface(&stroke_mask, 0.0, 0.0);
    let _ = cr.restore();
}
