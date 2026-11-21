use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::{SystemTime, UNIX_EPOCH};

use adw::prelude::*;
use gtk::glib;
use gtk::glib::object::IsA;

use crate::audio::Audio;
use crate::db::{Db, GroupSummary, VocabEntry};
use crate::writing::WritingPad;
use crate::APP_ID;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Mode {
    Quiz,
    Write,
}

#[derive(Default)]
struct Session {
    single: bool,
    queue: Vec<VocabEntry>,
    index: usize,
    revealed: bool,
}

impl Session {
    fn current(&self) -> Option<&VocabEntry> {
        self.queue.get(self.index)
    }
}

struct GroupsWidgets {
    root: gtk::Box,
    groups_box: gtk::Box,
}

struct GroupPageWidgets {
    root: gtk::Box,
    title: gtk::Label,
    subtitle: gtk::Label,
    flow: gtk::FlowBox,
    train_button: gtk::Button,
    delete_button: gtk::Button,
}

struct ReviewWidgets {
    root: gtk::ScrolledWindow,
    progress_label: gtk::Label,
    progress: gtk::ProgressBar,
    glyph: gtk::Label,
    traditional: gtk::Label,
    pinyin: gtk::Label,
    meaning: gtk::Label,
    audio_button: gtk::Button,
    answer_grid: gtk::Grid,
    answer_buttons: Vec<gtk::Button>,
    answer_labels: Vec<gtk::Label>,
    answer_feedback: gtk::Label,
    stage: gtk::Stack,
    continue_button: gtk::Button,
    audio_prompt_panel: gtk::Box,
    audio_prompt_play_button: gtk::Button,
    custom_group_popover: gtk::Popover,
    custom_group_box: gtk::Box,
    writing_pad: Rc<WritingPad>,
    hint: gtk::Label,
}

struct Ui {
    window: adw::ApplicationWindow,
    db: Rc<Db>,
    audio: Rc<Audio>,
    session: RefCell<Session>,
    mode: Cell<Mode>,
    quiz_button: gtk::ToggleButton,
    write_button: gtk::ToggleButton,
    rng: RefCell<u64>,
    correct_index: Cell<usize>,
    made_mistake: Cell<bool>,
    current_group: RefCell<Option<GroupSummary>>,
    stack: gtk::Stack,
    back_button: gtk::Button,
    groups_box: gtk::Box,
    group_title: gtk::Label,
    group_subtitle: gtk::Label,
    group_flow: gtk::FlowBox,
    group_train_button: gtk::Button,
    group_delete_button: gtk::Button,
    progress_label: gtk::Label,
    progress: gtk::ProgressBar,
    glyph: gtk::Label,
    traditional: gtk::Label,
    pinyin: gtk::Label,
    meaning: gtk::Label,
    audio_button: gtk::Button,
    answer_grid: gtk::Grid,
    answer_buttons: Vec<gtk::Button>,
    answer_labels: Vec<gtk::Label>,
    answer_feedback: gtk::Label,
    stage: gtk::Stack,
    continue_button: gtk::Button,
    audio_prompt: Cell<bool>,
    audio_prompt_panel: gtk::Box,
    custom_group_popover: gtk::Popover,
    custom_group_box: gtk::Box,
    search_entry: gtk::SearchEntry,
    search_results: gtk::Box,
    search_return: RefCell<Option<String>>,
    writing_pad: Rc<WritingPad>,
    hint: gtk::Label,
    summary: adw::StatusPage,
}

pub fn build(app: &adw::Application) {
    let db = match Db::open() {
        Ok(db) => Rc::new(db),
        Err(error) => {
            eprintln!("hanzi: could not open the database: {error:#}");
            return;
        },
    };
    let audio = Audio::new();
    let initial_scheme = db
        .setting("color_scheme")
        .ok()
        .flatten()
        .unwrap_or_else(|| "system".to_string());
    apply_color_scheme(&initial_scheme);

    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("Hanzi")
        .default_width(900)
        .default_height(820)
        .build();

    let header = adw::HeaderBar::new();
    let window_title = adw::WindowTitle::new("Hanzi", "Mandarin vocabulary");

    let back_button = gtk::Button::from_icon_name("go-previous-symbolic");
    back_button.add_css_class("flat");
    back_button.set_tooltip_text(Some("Back"));
    back_button.set_visible(false);
    header.pack_start(&back_button);

    let mode_switch = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    mode_switch.add_css_class("linked");
    let quiz_button = gtk::ToggleButton::with_label("Quiz");
    let write_button = gtk::ToggleButton::with_label("Write");
    write_button.set_group(Some(&quiz_button));
    quiz_button.set_active(true);
    quiz_button.set_tooltip_text(Some("Multiple choice: pick the translation"));
    write_button.set_tooltip_text(Some("Write the character from the pinyin and translation"));
    mode_switch.append(&quiz_button);
    mode_switch.append(&write_button);

    let search_entry = gtk::SearchEntry::new();
    search_entry.set_placeholder_text(Some("Search pinyin, hanzi or meaning"));
    search_entry.set_tooltip_text(Some("Search pinyin, characters, words or meanings"));
    search_entry.set_width_chars(18);
    search_entry.set_max_width_chars(24);

    let search_results = gtk::Box::new(gtk::Orientation::Vertical, 2);
    search_results.set_halign(gtk::Align::Center);
    search_results.set_margin_top(16);
    search_results.set_margin_bottom(16);
    search_results.set_margin_start(12);
    search_results.set_margin_end(12);
    search_results.set_size_request(560, -1);

    let search_root = gtk::ScrolledWindow::new();
    search_root.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    search_root.set_vexpand(true);
    search_root.set_child(Some(&search_results));

    let audio_check = gtk::CheckButton::with_label("Audio prompt");
    audio_check.set_margin_start(6);
    audio_check.set_tooltip_text(Some(
        "Play the character and hide its glyph, pinyin and meaning; uncheck to show the text prompt",
    ));
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    controls.set_margin_top(10); // To not overlap with bottom of window_title
    controls.set_halign(gtk::Align::Center);
    controls.append(&mode_switch);
    controls.append(&search_entry);
    // Hidden while audio is disabled; the playback code stays in place.
    if crate::audio::ENABLED {
        controls.append(&audio_check);
    }

    let title_area = gtk::Box::new(gtk::Orientation::Vertical, 2);
    title_area.set_halign(gtk::Align::Center);
    window_title.set_halign(gtk::Align::Center);
    title_area.append(&window_title);
    title_area.append(&controls);
    header.set_title_widget(Some(&title_area));

    let theme_button = gtk::MenuButton::builder()
        .icon_name(if adw::StyleManager::default().is_dark() {
            "weather-clear-night-symbolic"
        } else {
            "weather-clear-symbolic"
        })
        .tooltip_text("Appearance")
        .build();
    theme_button.add_css_class("flat");

    let theme_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    theme_box.set_margin_top(10);
    theme_box.set_margin_bottom(10);
    theme_box.set_margin_start(12);
    theme_box.set_margin_end(12);
    let theme_title = gtk::Label::new(Some("Appearance"));
    theme_title.add_css_class("heading");
    theme_title.set_halign(gtk::Align::Start);
    theme_box.append(&theme_title);

    let system_button = gtk::CheckButton::with_label("Follow system");
    let light_button = gtk::CheckButton::with_label("Light");
    let dark_button = gtk::CheckButton::with_label("Dark");
    light_button.set_group(Some(&system_button));
    dark_button.set_group(Some(&system_button));
    match initial_scheme.as_str() {
        "light" => light_button.set_active(true),
        "dark" => dark_button.set_active(true),
        _ => system_button.set_active(true),
    }
    for button in [&system_button, &light_button, &dark_button] {
        button.set_halign(gtk::Align::Start);
        theme_box.append(button);
    }
    let theme_popover = gtk::Popover::new();
    theme_popover.set_child(Some(&theme_box));
    theme_button.set_popover(Some(&theme_popover));

    {
        let db = Rc::clone(&db);
        system_button.connect_toggled(move |button| {
            if button.is_active() {
                apply_color_scheme("system");
                let _ = db.set_setting("color_scheme", "system");
            }
        });
    }
    {
        let db = Rc::clone(&db);
        light_button.connect_toggled(move |button| {
            if button.is_active() {
                apply_color_scheme("light");
                let _ = db.set_setting("color_scheme", "light");
            }
        });
    }
    {
        let db = Rc::clone(&db);
        dark_button.connect_toggled(move |button| {
            if button.is_active() {
                apply_color_scheme("dark");
                let _ = db.set_setting("color_scheme", "dark");
            }
        });
    }
    {
        let theme_button = theme_button.clone();
        adw::StyleManager::default().connect_dark_notify(move |manager| {
            theme_button.set_icon_name(if manager.is_dark() {
                "weather-clear-night-symbolic"
            } else {
                "weather-clear-symbolic"
            });
        });
    }

    let about_button = gtk::Button::from_icon_name("help-about-symbolic");
    about_button.set_tooltip_text(Some("About Hanzi"));
    let header_end = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    header_end.append(&theme_button);
    header_end.append(&about_button);
    header.pack_end(&header_end);

    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .hhomogeneous(false)
        .vhomogeneous(false)
        .vexpand(true)
        .build();

    let GroupsWidgets {
        root: groups_root,
        groups_box,
    } = build_groups_page();

    let GroupPageWidgets {
        root: group_root,
        title: group_title,
        subtitle: group_subtitle,
        flow: group_flow,
        train_button: group_train_button,
        delete_button: group_delete_button,
    } = build_group_page();

    let ReviewWidgets {
        root: review_root,
        progress_label,
        progress,
        glyph,
        traditional,
        pinyin,
        meaning,
        audio_button,
        answer_grid,
        answer_buttons,
        answer_labels,
        answer_feedback,
        stage,
        continue_button,
        audio_prompt_panel,
        audio_prompt_play_button,
        custom_group_popover,
        custom_group_box,
        writing_pad,
        hint,
    } = build_review();

    let (summary, summary_back_button) = build_summary();

    stack.add_named(&groups_root, Some("groups"));
    stack.add_named(&group_root, Some("group"));
    stack.add_named(&review_root, Some("review"));
    stack.add_named(&summary, Some("summary"));
    stack.add_named(&search_root, Some("search"));

    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&stack));
    window.set_content(Some(&toolbar));

    let answer_buttons_for_signals = answer_buttons.clone();

    let ui = Rc::new(Ui {
        window: window.clone(),
        db,
        audio,
        session: RefCell::new(Session::default()),
        mode: Cell::new(Mode::Quiz),
        quiz_button: quiz_button.clone(),
        write_button: write_button.clone(),
        rng: RefCell::new(seed_rng()),
        correct_index: Cell::new(0),
        made_mistake: Cell::new(false),
        current_group: RefCell::new(None),
        stack: stack.clone(),
        back_button: back_button.clone(),
        groups_box,
        group_title,
        group_subtitle,
        group_flow,
        group_train_button: group_train_button.clone(),
        group_delete_button: group_delete_button.clone(),
        progress_label,
        progress,
        glyph,
        traditional,
        pinyin,
        meaning,
        audio_button: audio_button.clone(),
        answer_grid,
        answer_buttons,
        answer_labels,
        answer_feedback,
        stage,
        continue_button: continue_button.clone(),
        audio_prompt: Cell::new(false),
        audio_prompt_panel,
        custom_group_popover,
        custom_group_box,
        search_entry: search_entry.clone(),
        search_results,
        search_return: RefCell::new(None),
        writing_pad,
        hint,
        summary,
    });

    {
        let ui = Rc::clone(&ui);
        back_button.connect_clicked(move |_| ui.go_back());
    }
    {
        let weak = Rc::downgrade(&ui);
        ui.custom_group_popover.connect_show(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.rebuild_custom_group_popover();
            }
        });
    }
    {
        let ui = Rc::clone(&ui);
        group_train_button.connect_clicked(move |_| ui.train_current_group());
    }
    {
        let ui = Rc::clone(&ui);
        group_delete_button.connect_clicked(move |_| ui.delete_current_group());
    }
    {
        let ui = Rc::clone(&ui);
        audio_button.connect_clicked(move |_| ui.play_current());
    }
    {
        let ui = Rc::clone(&ui);
        audio_prompt_play_button.connect_clicked(move |_| ui.play_current());
    }
    {
        let ui = Rc::clone(&ui);
        audio_check.connect_toggled(move |button| {
            ui.audio_prompt.set(button.is_active());
            ui.audio_prompt_changed();
        });
    }
    for (index, button) in answer_buttons_for_signals.into_iter().enumerate() {
        let ui = Rc::clone(&ui);
        button.connect_clicked(move |_| ui.answer(index));
    }
    {
        let ui = Rc::clone(&ui);
        continue_button.connect_clicked(move |_| ui.continue_after_answer());
    }
    {
        let ui = Rc::clone(&ui);
        quiz_button.connect_toggled(move |button| {
            if button.is_active() {
                ui.mode.set(Mode::Quiz);
                ui.mode_changed();
            }
        });
    }
    {
        let ui = Rc::clone(&ui);
        write_button.connect_toggled(move |button| {
            if button.is_active() {
                ui.mode.set(Mode::Write);
                ui.mode_changed();
            }
        });
    }
    {
        let weak = Rc::downgrade(&ui);
        ui.writing_pad.set_on_finished(Rc::new(move |correct| {
            if let Some(ui) = weak.upgrade() {
                ui.writing_finished(correct);
            }
        }));
    }
    {
        let ui = Rc::clone(&ui);
        summary_back_button.connect_clicked(move |_| ui.back_to_group());
    }
    {
        let weak = Rc::downgrade(&ui);
        ui.search_entry.connect_changed(move |_| {
            if let Some(ui) = weak.upgrade() {
                ui.search_changed();
            }
        });
    }
    {
        let window = window.clone();
        about_button.connect_clicked(move |_| show_about(&window));
    }

    let key_controller = gtk::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let ui = Rc::clone(&ui);
        key_controller.connect_key_pressed(move |_, keyval, _, _| ui.key_pressed(keyval));
    }
    window.add_controller(key_controller);

    ui.populate_groups();
    window.present();
}

impl Ui {
    /// Route a key press by screen first, then by review phase.  Keeping the
    /// two levels separate makes it easy to add shortcuts for other screens
    /// without leaving the review-specific state machine.
    fn key_pressed(self: &Rc<Self>, keyval: gtk::gdk::Key) -> glib::Propagation {
        let Some(character) = keyval.to_unicode() else {
            return glib::Propagation::Proceed;
        };

        match self.stack.visible_child_name().as_deref() {
            Some("review") => self.review_key_pressed(character),
            // Groups, group cards, search and summary currently keep their own
            // focused widgets. Add page-specific shortcuts here when needed.
            _ => glib::Propagation::Proceed,
        }
    }

    fn review_key_pressed(self: &Rc<Self>, character: char) -> glib::Propagation {
        if character.eq_ignore_ascii_case(&'p') {
            if crate::audio::ENABLED {
                self.play_current();
            }
            return glib::Propagation::Stop;
        }

        if self.session.borrow().revealed {
            return self.revealed_key_pressed(character);
        }

        match self.mode.get() {
            Mode::Quiz => self.quiz_key_pressed(character),
            Mode::Write => self.write_key_pressed(character),
        }
    }

    fn revealed_key_pressed(self: &Rc<Self>, character: char) -> glib::Propagation {
        if matches!(character, ' ' | '\r' | '\n') {
            self.continue_after_answer();
            return glib::Propagation::Stop;
        }
        if character.eq_ignore_ascii_case(&'r') {
            self.show_current();
            return glib::Propagation::Stop;
        }
        if character.eq_ignore_ascii_case(&'q') {
            self.switch_mode(Mode::Quiz);
            return glib::Propagation::Stop;
        }
        if character.eq_ignore_ascii_case(&'w') {
            self.switch_mode(Mode::Write);
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    }

    fn quiz_key_pressed(&self, character: char) -> glib::Propagation {
        if character == ' ' {
            return glib::Propagation::Stop;
        }

        let index = match character {
            '1' => 0,
            '2' => 1,
            '3' => 2,
            '4' => 3,
            _ => return glib::Propagation::Proceed,
        };
        self.answer(index);
        glib::Propagation::Stop
    }

    fn write_key_pressed(&self, character: char) -> glib::Propagation {
        if character.eq_ignore_ascii_case(&'h') {
            self.writing_pad.hint();
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    }

    /// Switch modes through the header toggle so its active state stays in
    /// sync.  If the requested mode is already active, repeat the current
    /// entry instead.
    fn switch_mode(&self, mode: Mode) {
        let button = match mode {
            Mode::Quiz => &self.quiz_button,
            Mode::Write => &self.write_button,
        };
        if button.is_active() {
            self.show_current();
        } else {
            button.set_active(true);
        }
    }

    fn populate_groups(self: &Rc<Self>) {
        while let Some(child) = self.groups_box.first_child() {
            self.groups_box.remove(&child);
        }

        match self.db.groups() {
            Ok(groups) if !groups.is_empty() => {
                for group in groups {
                    let card = make_group_card(&group);
                    let ui = Rc::clone(self);
                    card.connect_clicked(move |_| ui.open_group(group.clone()));
                    self.groups_box.append(&card);
                }
            },
            Ok(_) => {
                let label = gtk::Label::new(Some("No groups in the database."));
                label.add_css_class("dim-label");
                self.groups_box.append(&label);
            },
            Err(error) => eprintln!("hanzi: could not read groups: {error:#}"),
        }
    }

    fn open_group(self: &Rc<Self>, group: GroupSummary) {
        self.current_group.replace(Some(group.clone()));
        self.group_title.set_text(&group.name);
        let group_mastery = self.db.group_mastery(&group.id).unwrap_or(0);
        let mut subtitle = format!("{} symbols · shuffled training", group.count);
        if group_mastery > 0 {
            subtitle.push_str(&format!(" · mastery {group_mastery}/5"));
        }
        self.group_subtitle.set_text(&subtitle);
        self.group_delete_button.set_visible(group.custom);

        while let Some(child) = self.group_flow.first_child() {
            self.group_flow.remove(&child);
        }

        let entries = match self.db.group_entries(&group.id) {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("hanzi: could not read group {}: {error:#}", group.id);
                return;
            },
        };
        let levels = self.db.mastery_levels(&group.id).unwrap_or_default();

        self.group_train_button.set_sensitive(!entries.is_empty());
        for entry in entries {
            let button = gtk::Button::with_label(&entry.simplified);
            button.add_css_class("symbol-button");
            let level = levels.get(&entry.id).copied().unwrap_or(0);
            apply_mastery(&button, level);
            let mut tooltip = if entry.is_word() {
                format!("{} — {} (word)", entry.display_pinyin(), entry.meaning)
            } else {
                format!("{} — {} (character)", entry.display_pinyin(), entry.meaning)
            };
            if level > 0 {
                tooltip.push_str(&format!(" · mastery {level}/5"));
            }
            button.set_tooltip_text(Some(&tooltip));
            let ui = Rc::clone(self);
            let picked = entry.clone();
            button.connect_clicked(move |_| ui.start_single(picked.clone()));
            self.group_flow.insert(&button, -1);
        }

        self.stack.set_visible_child_name("group");
        self.back_button.set_visible(true);
    }

    fn train_current_group(&self) {
        let Some(group) = self.current_group.borrow().clone() else {
            return;
        };
        let mut entries = match self.db.group_entries(&group.id) {
            Ok(entries) => entries,
            Err(error) => {
                eprintln!("hanzi: could not read group {}: {error:#}", group.id);
                return;
            },
        };
        if entries.is_empty() {
            return;
        }
        {
            let mut rng = self.rng.borrow_mut();
            shuffle(&mut entries, &mut rng);
        }
        self.start_session(entries, false);
    }

    fn start_single(&self, entry: VocabEntry) {
        self.start_session(vec![entry], true);
    }

    fn start_session(&self, entries: Vec<VocabEntry>, single: bool) {
        if entries.is_empty() {
            return;
        }

        *self.session.borrow_mut() = Session {
            single,
            queue: entries,
            index: 0,
            revealed: false,
        };
        self.stack.set_visible_child_name("review");
        self.back_button.set_visible(true);
        self.show_current();
        gtk::prelude::GtkWindowExt::set_focus(&self.window, None::<&gtk::Widget>);
    }

    fn show_current(&self) {
        let payload = {
            let session = self.session.borrow();
            session
                .current()
                .map(|entry| (entry.clone(), session.index + 1, session.queue.len()))
        };

        let Some((entry, position, total)) = payload else {
            self.finish();
            return;
        };

        let has_audio = Audio::has_character_clip(&entry);
        let audio_prompt = self.audio_prompt.get() && has_audio;

        self.glyph.set_text(&entry.simplified);
        self.refresh_mastery(&entry);
        self.pinyin.set_text(&entry.display_pinyin());
        self.meaning.set_text(&entry.meaning);
        self.audio_button.set_visible(has_audio);
        self.audio_prompt_panel.set_visible(false);
        self.reset_answer_state();
        self.session.borrow_mut().revealed = false;
        self.writing_pad.root().set_visible(false);

        match self.mode.get() {
            Mode::Quiz => {
                self.stage.set_size_request(-1, 188);
                self.stage.set_visible_child_name("choices");
                self.answer_grid.set_visible(true);
                self.setup_choices(&entry);

                if audio_prompt {
                    self.glyph.set_visible(false);
                    self.traditional.set_visible(false);
                    self.pinyin.set_visible(false);
                    self.meaning.set_visible(false);
                    self.audio_button.set_visible(false);
                    self.audio_prompt_panel.set_visible(true);
                    self.hint.set_text("Audio prompt · 1–4 choose · P replay");
                    self.play_current();
                } else {
                    self.glyph.set_visible(true);
                    self.traditional.set_visible(false);
                    if let Some(traditional) = entry.display_traditional() {
                        self.traditional
                            .set_text(&format!("traditional · {traditional}"));
                        self.traditional.set_visible(true);
                    }
                    self.pinyin.set_visible(false);
                    self.meaning.set_visible(false);
                    self.hint.set_text(if self.audio_prompt.get() {
                        "Audio prompt unavailable for this entry · 1–4 choose"
                    } else if has_audio {
                        "1–4 choose · P plays · Space continues"
                    } else {
                        "1–4 choose · Space continues"
                    });
                }
            },
            Mode::Write => {
                self.stage.set_size_request(-1, -1);
                self.stage.set_visible_child_name("reveal");
                self.answer_grid.set_visible(false);
                self.writing_pad.set_entry(&entry);
                self.writing_pad.root().set_visible(true);

                if audio_prompt {
                    self.glyph.set_visible(false);
                    self.traditional.set_visible(false);
                    self.pinyin.set_visible(false);
                    self.meaning.set_visible(false);
                    self.audio_button.set_visible(false);
                    self.audio_prompt_panel.set_visible(true);
                    self.hint
                        .set_text("Audio prompt · write what you hear · H hint · P replay");
                    self.play_current();
                } else {
                    self.glyph.set_visible(false);
                    self.traditional.set_visible(false);
                    self.pinyin.set_visible(true);
                    self.meaning.set_visible(true);
                    self.hint.set_text(if self.audio_prompt.get() {
                        "Audio prompt unavailable for this entry · write from memory"
                    } else if has_audio {
                        "Write from memory · H hint · P plays"
                    } else {
                        "Write from memory · H hint"
                    });
                }
            },
        }

        self.progress_label
            .set_text(&format!("{position} of {total}"));
        self.progress.set_fraction(position as f64 / total as f64);
        let single = self.session.borrow().single;
        self.progress_label.set_visible(!single);
        self.progress.set_visible(!single);
        self.back_button.set_visible(true);
    }

    fn audio_prompt_changed(&self) {
        let on_review = self
            .stack
            .visible_child_name()
            .map(|name| name.as_str() == "review")
            .unwrap_or(false);
        if !on_review || self.session.borrow().current().is_none() {
            return;
        }
        self.show_current();
    }

    fn mode_changed(&self) {
        let on_review = self
            .stack
            .visible_child_name()
            .map(|name| name.as_str() == "review")
            .unwrap_or(false);
        if !on_review || self.session.borrow().current().is_none() {
            return;
        }
        self.show_current();
    }

    fn writing_finished(&self, correct: bool) {
        if self.mode.get() != Mode::Write || self.session.borrow().revealed {
            return;
        }

        self.session.borrow_mut().revealed = true;
        self.writing_pad.root().set_visible(false);
        self.audio_prompt_panel.set_visible(false);
        self.glyph.set_visible(true);
        self.pinyin.set_visible(true);
        self.meaning.set_visible(true);

        let entry = {
            let session = self.session.borrow();
            session.current().cloned()
        };
        if let Some(entry) = entry {
            self.audio_button
                .set_visible(Audio::has_character_clip(&entry));
            self.traditional.set_visible(false);
            if let Some(traditional) = entry.display_traditional() {
                self.traditional
                    .set_text(&format!("traditional · {traditional}"));
                self.traditional.set_visible(true);
            }
            if correct {
                let _ = self.db.record_write_correct(&entry.id);
            } else {
                let _ = self.db.record_write_wrong(&entry.id);
            }
            self.refresh_mastery(&entry);
        }

        self.hint.set_text(if correct {
            "Correct — Space continues · R repeats · Q/W switch mode"
        } else {
            "Mistakes — Space continues · R repeats · Q/W switch mode"
        });
        self.continue_button.set_visible(true);
        self.continue_button.grab_focus();
    }

    fn reset_answer_state(&self) {
        self.made_mistake.set(false);
        self.answer_feedback.set_text(" ");
        self.continue_button.set_visible(false);

        for button in &self.answer_buttons {
            button.remove_css_class("answer-correct");
            button.remove_css_class("answer-wrong");
            button.set_sensitive(true);
        }
    }

    fn setup_choices(&self, entry: &VocabEntry) {
        let mut pool = self.db.meanings().unwrap_or_default();
        pool.retain(|meaning| meaning != &entry.meaning);
        pool.sort();
        pool.dedup();

        {
            let mut rng = self.rng.borrow_mut();
            shuffle(&mut pool, &mut rng);
        }

        let mut texts: Vec<String> = pool.into_iter().take(3).collect();
        while texts.len() < 3 {
            texts.push("—".to_string());
        }
        texts.push(entry.meaning.clone());

        {
            let mut rng = self.rng.borrow_mut();
            shuffle(&mut texts, &mut rng);
        }

        let correct = texts
            .iter()
            .position(|text| text == &entry.meaning)
            .unwrap_or(0);

        for (index, label) in self.answer_labels.iter().enumerate() {
            label.set_text(texts.get(index).map(String::as_str).unwrap_or("—"));
        }

        self.correct_index.set(correct);
    }

    fn answer(&self, index: usize) {
        if self.mode.get() != Mode::Quiz
            || self.session.borrow().revealed
            || index >= self.answer_buttons.len()
        {
            return;
        }

        if index == self.correct_index.get() {
            self.answer_buttons[index].add_css_class("answer-correct");
            for button in &self.answer_buttons {
                button.set_sensitive(false);
            }

            self.audio_prompt_panel.set_visible(false);
            self.glyph.set_visible(true);
            self.pinyin.set_visible(true);
            self.meaning.set_visible(true);
            self.answer_feedback.set_text(" ");
            self.stage.set_visible_child_name("reveal");
            self.session.borrow_mut().revealed = true;

            let entry = {
                let session = self.session.borrow();
                session.current().cloned()
            };
            if let Some(entry) = entry {
                self.audio_button
                    .set_visible(Audio::has_character_clip(&entry));
                self.traditional.set_visible(false);
                if let Some(traditional) = entry.display_traditional() {
                    self.traditional
                        .set_text(&format!("traditional · {traditional}"));
                    self.traditional.set_visible(true);
                }
                if !self.made_mistake.get() {
                    let _ = self.db.record_quiz_correct(&entry.id);
                    self.refresh_mastery(&entry);
                }
            }

            let missed = self.made_mistake.get();
            self.hint.set_text(if missed {
                "Missed one — Space continues · R repeats · Q/W switch mode"
            } else {
                "Correct — Space continues · R repeats · Q/W switch mode"
            });
            self.continue_button.set_visible(true);
            self.play_current();
        } else {
            if !self.made_mistake.get() {
                self.made_mistake.set(true);
                let entry = {
                    let session = self.session.borrow();
                    session.current().cloned()
                };
                if let Some(entry) = entry {
                    let _ = self.db.record_quiz_wrong(&entry.id);
                    self.refresh_mastery(&entry);
                }
            }
            self.answer_buttons[index].add_css_class("answer-wrong");
            self.answer_buttons[index].set_sensitive(false);
            self.answer_feedback
                .set_text("Not quite — try another one.");
        }
    }

    fn continue_after_answer(self: &Rc<Self>) {
        if !self.session.borrow().revealed {
            return;
        }
        self.advance();
    }

    fn advance(self: &Rc<Self>) {
        let (done, single) = {
            let mut session = self.session.borrow_mut();
            session.index += 1;
            (session.current().is_none(), session.single)
        };

        if done {
            if single {
                // A single-symbol review has nothing to summarise: go back to
                // the group the symbol was picked from.
                self.back_to_group();
            } else {
                self.finish();
            }
        } else {
            self.show_current();
        }
    }

    fn finish(&self) {
        let (title, description) = {
            let session = self.session.borrow();
            let count = session.index;
            if count == 1 {
                ("Group complete", "1 symbol trained.".to_string())
            } else {
                ("Group complete", format!("{count} symbols trained."))
            }
        };

        self.summary.set_title(title);
        self.summary.set_description(Some(&description));
        self.stack.set_visible_child_name("summary");
        self.back_button.set_visible(true);
    }

    fn go_back(self: &Rc<Self>) {
        let page = self
            .stack
            .visible_child_name()
            .map(|name| name.to_string())
            .unwrap_or_default();

        match page.as_str() {
            "search" => {
                self.search_entry.set_text("");
            },
            "group" => self.show_groups(),
            "review" | "summary" => {
                let has_group = self.current_group.borrow().is_some();
                if has_group {
                    self.back_to_group();
                } else {
                    self.show_groups();
                }
            },
            _ => {},
        }
    }

    fn show_groups(self: &Rc<Self>) {
        self.stack.set_visible_child_name("groups");
        self.back_button.set_visible(false);
        self.current_group.replace(None);
        self.populate_groups();
    }

    fn back_to_group(self: &Rc<Self>) {
        let group = self.current_group.borrow().clone();
        if let Some(group) = group {
            self.open_group(group);
        } else {
            self.show_groups();
        }
    }

    fn rebuild_custom_group_popover(self: &Rc<Self>) {
        while let Some(child) = self.custom_group_box.first_child() {
            self.custom_group_box.remove(&child);
        }

        let title = gtk::Label::new(Some("Add to group"));
        title.add_css_class("heading");
        title.set_halign(gtk::Align::Start);
        self.custom_group_box.append(&title);

        let custom: Vec<GroupSummary> = self
            .db
            .groups()
            .unwrap_or_default()
            .into_iter()
            .filter(|group| group.custom)
            .collect();

        if custom.is_empty() {
            let empty = gtk::Label::new(Some("No custom groups yet."));
            empty.add_css_class("dim-label");
            empty.set_halign(gtk::Align::Start);
            self.custom_group_box.append(&empty);
        } else {
            for group in custom {
                let button = gtk::Button::with_label(&group.name);
                button.add_css_class("flat");
                button.set_halign(gtk::Align::Fill);
                let weak = Rc::downgrade(self);
                let popover = self.custom_group_popover.clone();
                button.connect_clicked(move |_| {
                    if let Some(ui) = weak.upgrade() {
                        ui.add_current_to_custom_group(&group.id, &group.name, &popover);
                    }
                });
                self.custom_group_box.append(&button);
            }
        }

        self.custom_group_box
            .append(&gtk::Separator::new(gtk::Orientation::Horizontal));

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let entry = gtk::Entry::new();
        entry.set_placeholder_text(Some("New group name"));
        entry.set_width_chars(14);
        let add = gtk::Button::with_label("+");
        add.add_css_class("suggested-action");
        row.append(&entry);
        row.append(&add);
        self.custom_group_box.append(&row);

        {
            let weak = Rc::downgrade(self);
            let popover = self.custom_group_popover.clone();
            let entry = entry.clone();
            add.connect_clicked(move |_| {
                if let Some(ui) = weak.upgrade() {
                    ui.create_custom_group_and_add(entry.text().as_str(), &popover);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            let popover = self.custom_group_popover.clone();
            entry.connect_activate(move |entry| {
                if let Some(ui) = weak.upgrade() {
                    ui.create_custom_group_and_add(entry.text().as_str(), &popover);
                }
            });
        }
    }

    fn add_current_to_custom_group(
        self: &Rc<Self>,
        group_id: &str,
        group_name: &str,
        popover: &gtk::Popover,
    ) {
        let entry = self.session.borrow().current().cloned();
        let Some(entry) = entry else {
            popover.popdown();
            return;
        };
        match self.db.append_to_custom_group(group_id, &entry.id) {
            Ok(true) => {
                self.hint
                    .set_text(&format!("Added {} to “{}”", entry.simplified, group_name));
                self.populate_groups();
            },
            Ok(false) => self.hint.set_text(&format!(
                "{} is already in “{}”",
                entry.simplified, group_name
            )),
            Err(error) => {
                eprintln!("hanzi: could not add to group: {error:#}");
                self.hint.set_text("Could not add to a group.");
            },
        }
        popover.popdown();
    }

    fn create_custom_group_and_add(self: &Rc<Self>, raw_name: &str, popover: &gtk::Popover) {
        let name = raw_name.trim();
        if name.is_empty() {
            return;
        }
        match self.db.create_custom_group(name) {
            Ok(group_id) => {
                if let Some(entry) = self.session.borrow().current().cloned() {
                    if let Err(error) = self.db.append_to_custom_group(&group_id, &entry.id) {
                        eprintln!("hanzi: could not add to new group: {error:#}");
                    }
                }
                self.populate_groups();
                self.hint.set_text(&format!("Created “{name}”."));
            },
            Err(error) => {
                eprintln!("hanzi: could not create group: {error:#}");
                self.hint.set_text("Could not create a group.");
            },
        }
        popover.popdown();
    }

    fn delete_current_group(self: &Rc<Self>) {
        let Some(group) = self.current_group.borrow().clone() else {
            return;
        };
        if !group.custom {
            return;
        }

        let dialog = adw::AlertDialog::builder()
            .heading("Delete group?")
            .body(format!(
                "“{}” will be removed. The deck entries themselves stay.",
                group.name
            ))
            .build();
        dialog.add_response("cancel", "Cancel");
        dialog.add_response("delete", "Delete");
        dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
        dialog.set_default_response(Some("cancel"));
        dialog.set_close_response("cancel");

        let ui = Rc::clone(self);
        dialog.choose(
            Some(&self.window),
            None::<&gio::Cancellable>,
            move |response| {
                if response.as_str() == "delete" {
                    match ui.db.delete_custom_group(&group.id) {
                        Ok(_) => {
                            ui.current_group.replace(None);
                            ui.show_groups();
                        },
                        Err(error) => eprintln!("hanzi: could not delete group: {error:#}"),
                    }
                }
            },
        );
    }

    fn refresh_mastery(&self, entry: &VocabEntry) {
        let level = self.db.mastery_level(&entry.id).unwrap_or(0);
        let (quiz, write) = self.db.mastery_counts(&entry.id).unwrap_or((0, 0));
        apply_mastery(&self.glyph, level);
        if quiz + write > 0 {
            self.glyph.set_tooltip_text(Some(&format!(
                "Mastery {level}/5 · quiz {quiz} · writing {write}"
            )));
        } else {
            self.glyph.set_tooltip_text(None);
        }
    }

    fn leave_search(&self) {
        if self.stack.visible_child_name().as_deref() == Some("search") {
            let previous = self
                .search_return
                .take()
                .unwrap_or_else(|| "groups".to_string());
            self.stack.set_visible_child_name(&previous);
            self.back_button.set_visible(previous != "groups");
        }
    }

    fn search_changed(self: &Rc<Self>) {
        while let Some(child) = self.search_results.first_child() {
            self.search_results.remove(&child);
        }

        let query = self.search_entry.text();
        if query.trim().is_empty() {
            self.leave_search();
            return;
        }

        if self.stack.visible_child_name().as_deref() != Some("search") {
            let previous = self
                .stack
                .visible_child_name()
                .map(|name| name.to_string())
                .unwrap_or_else(|| "groups".to_string());
            self.search_return.replace(Some(previous));
            self.stack.set_visible_child_name("search");
            self.back_button.set_visible(true);
        }

        let entries = self
            .db
            .search_entries(query.as_str(), 20)
            .unwrap_or_default();
        if entries.is_empty() {
            let empty = gtk::Label::new(Some("No matches."));
            empty.add_css_class("dim-label");
            empty.set_halign(gtk::Align::Center);
            empty.set_margin_top(24);
            self.search_results.append(&empty);
        } else {
            for entry in entries {
                let button = gtk::Button::new();
                button.add_css_class("flat");
                button.add_css_class("search-result");
                button.set_halign(gtk::Align::Fill);

                let glyph = gtk::Label::new(Some(&entry.simplified));
                glyph.add_css_class("search-glyph");
                glyph.set_valign(gtk::Align::Center);

                let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
                text.set_hexpand(true);
                let pinyin = gtk::Label::new(Some(&entry.display_pinyin()));
                pinyin.add_css_class("search-pinyin");
                pinyin.set_halign(gtk::Align::Start);
                let meaning = gtk::Label::new(Some(&entry.meaning));
                meaning.add_css_class("dim-label");
                meaning.set_halign(gtk::Align::Start);
                meaning.set_ellipsize(gtk::pango::EllipsizeMode::End);
                meaning.set_max_width_chars(42);
                text.append(&pinyin);
                text.append(&meaning);

                let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                row.append(&glyph);
                row.append(&text);
                button.set_child(Some(&row));

                let weak = Rc::downgrade(self);
                let picked = entry.clone();
                button.connect_clicked(move |_| {
                    if let Some(ui) = weak.upgrade() {
                        ui.start_single(picked.clone());
                        ui.search_entry.set_text("");
                    }
                });
                self.search_results.append(&button);
            }
        }
    }

    fn play_current(&self) {
        let entry = {
            let session = self.session.borrow();
            session.current().cloned()
        };
        if let Some(entry) = entry {
            Audio::play_entry(&self.audio, &entry);
        }
    }
}

fn build_groups_page() -> GroupsWidgets {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
    root.set_halign(gtk::Align::Center);
    root.set_valign(gtk::Align::Center);
    root.set_margin_top(24);
    root.set_margin_bottom(24);
    root.set_margin_start(24);
    root.set_margin_end(24);

    let hero = gtk::Label::new(Some("汉字"));
    hero.add_css_class("hero-glyph");
    hero.set_halign(gtk::Align::Center);

    let subtitle = gtk::Label::new(Some(
        "Pick a group, train it shuffled, or choose one symbol.",
    ));
    subtitle.add_css_class("dim-label");
    subtitle.set_halign(gtk::Align::Center);
    subtitle.set_wrap(true);
    subtitle.set_justify(gtk::Justification::Center);

    let groups_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
    groups_box.set_halign(gtk::Align::Center);
    groups_box.set_margin_top(20);

    root.append(&hero);
    root.append(&subtitle);
    root.append(&groups_box);

    GroupsWidgets { root, groups_box }
}

fn build_group_page() -> GroupPageWidgets {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.set_halign(gtk::Align::Fill);
    root.set_valign(gtk::Align::Fill);
    root.set_margin_top(20);
    root.set_margin_bottom(16);
    root.set_margin_start(24);
    root.set_margin_end(24);

    let title = gtk::Label::new(None);
    title.add_css_class("title-1");
    title.set_halign(gtk::Align::Center);

    let subtitle = gtk::Label::new(None);
    subtitle.add_css_class("dim-label");
    subtitle.set_halign(gtk::Align::Center);

    let train_button = gtk::Button::with_label("Train shuffled");
    train_button.add_css_class("suggested-action");
    train_button.add_css_class("pill");
    train_button.set_tooltip_text(Some("Train every entry in this group"));

    let delete_button = gtk::Button::from_icon_name("user-trash-symbolic");
    delete_button.add_css_class("flat");
    delete_button.add_css_class("circular");
    delete_button.set_tooltip_text(Some("Delete this custom group"));
    delete_button.set_visible(false);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    actions.set_halign(gtk::Align::Center);
    actions.set_margin_top(4);
    actions.append(&train_button);
    actions.append(&delete_button);

    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_column_spacing(8);
    flow.set_row_spacing(8);
    flow.set_max_children_per_line(10);
    flow.set_min_children_per_line(2);
    flow.set_homogeneous(true);
    flow.set_halign(gtk::Align::Center);
    flow.set_valign(gtk::Align::Start);
    flow.set_margin_top(8);

    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_min_content_height(420);
    scroll.set_child(Some(&flow));

    root.append(&title);
    root.append(&subtitle);
    root.append(&actions);
    root.append(&scroll);

    GroupPageWidgets {
        root,
        title,
        subtitle,
        flow,
        train_button,
        delete_button,
    }
}

fn build_review() -> ReviewWidgets {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_halign(gtk::Align::Fill);
    content.set_valign(gtk::Align::Center);
    content.set_margin_top(16);
    content.set_margin_bottom(16);
    content.set_margin_start(24);
    content.set_margin_end(24);

    let progress_label = gtk::Label::new(Some("1 of 40"));
    progress_label.add_css_class("dim-label");
    progress_label.set_halign(gtk::Align::Center);

    let progress = gtk::ProgressBar::new();
    progress.set_size_request(500, -1);
    progress.set_halign(gtk::Align::Center);
    progress.set_hexpand(false);
    progress.set_show_text(false);
    progress.add_css_class("review-progress");

    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("hanzi-card");
    // Minimum height for two-line translations; smaller values make GTK
    // under-allocate the answer grid by a few pixels in debug builds.
    card.set_size_request(500, 460);
    card.set_halign(gtk::Align::Center);

    let glyph = gtk::Label::new(Some("你"));
    glyph.add_css_class("hanzi-glyph");
    glyph.set_halign(gtk::Align::Center);
    glyph.set_valign(gtk::Align::Center);
    glyph.set_vexpand(true);

    let traditional = gtk::Label::new(None);
    traditional.add_css_class("dim-label");
    traditional.set_halign(gtk::Align::Center);
    traditional.set_visible(false);

    let audio_button = gtk::Button::from_icon_name("audio-volume-high-symbolic");
    audio_button.add_css_class("flat");
    audio_button.add_css_class("circular");
    audio_button.set_halign(gtk::Align::Center);
    audio_button.set_tooltip_text(Some("Play pronunciation (P)"));

    let custom_group_button = gtk::MenuButton::builder()
        .icon_name("list-add-symbolic")
        .tooltip_text("Add to a custom group")
        .build();
    custom_group_button.add_css_class("flat");
    custom_group_button.add_css_class("circular");

    let custom_group_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    custom_group_box.set_margin_top(8);
    custom_group_box.set_margin_bottom(8);
    custom_group_box.set_margin_start(10);
    custom_group_box.set_margin_end(10);
    custom_group_box.set_size_request(220, -1);

    let custom_group_popover = gtk::Popover::new();
    let custom_group_scroll = gtk::ScrolledWindow::new();
    custom_group_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    custom_group_scroll.set_max_content_height(320);
    custom_group_scroll.set_propagate_natural_height(true);
    custom_group_scroll.set_child(Some(&custom_group_box));
    custom_group_popover.set_child(Some(&custom_group_scroll));
    custom_group_button.set_popover(Some(&custom_group_popover));

    let pinyin = gtk::Label::new(None);
    pinyin.add_css_class("pinyin");
    pinyin.set_halign(gtk::Align::Center);
    pinyin.set_visible(false);
    pinyin.set_margin_top(2);

    let meaning = gtk::Label::new(None);
    meaning.add_css_class("meaning");
    meaning.set_halign(gtk::Align::Center);
    meaning.set_justify(gtk::Justification::Center);
    meaning.set_wrap(true);
    meaning.set_lines(3);
    meaning.set_ellipsize(gtk::pango::EllipsizeMode::End);
    meaning.set_max_width_chars(34);
    meaning.set_visible(false);

    let audio_prompt_panel = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    audio_prompt_panel.set_halign(gtk::Align::Center);
    audio_prompt_panel.set_valign(gtk::Align::Center);
    audio_prompt_panel.set_margin_top(4);
    audio_prompt_panel.set_visible(false);

    let audio_prompt_play_button = gtk::Button::new();
    audio_prompt_play_button.add_css_class("audio-prompt-play");
    audio_prompt_play_button.add_css_class("suggested-action");
    audio_prompt_play_button.set_tooltip_text(Some("Replay the character (P)"));
    let audio_prompt_icon = gtk::Image::from_icon_name("audio-volume-high-symbolic");
    audio_prompt_icon.set_pixel_size(28);
    audio_prompt_play_button.set_child(Some(&audio_prompt_icon));

    let audio_prompt_label = gtk::Label::new(Some("Listen to the character."));
    audio_prompt_label.add_css_class("dim-label");
    audio_prompt_label.set_max_width_chars(30);

    audio_prompt_panel.append(&audio_prompt_play_button);
    audio_prompt_panel.append(&audio_prompt_label);

    let answer_grid = gtk::Grid::new();
    answer_grid.set_row_spacing(8);
    answer_grid.set_column_spacing(8);
    answer_grid.set_column_homogeneous(true);
    answer_grid.set_halign(gtk::Align::Center);
    answer_grid.set_margin_top(10);
    answer_grid.set_visible(false);

    let mut answer_buttons = Vec::new();
    let mut answer_labels = Vec::new();
    for index in 0..4 {
        let number = gtk::Label::new(Some(&(index + 1).to_string()));
        number.add_css_class("answer-number");
        number.set_valign(gtk::Align::Center);

        let text = gtk::Label::new(None);
        text.set_wrap(true);
        text.set_justify(gtk::Justification::Center);
        text.set_max_width_chars(22);
        text.set_lines(2);
        text.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.set_hexpand(true);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        row.append(&number);
        row.append(&text);

        let button = gtk::Button::new();
        button.set_child(Some(&row));
        button.add_css_class("answer-choice");
        button.set_hexpand(true);
        button.set_size_request(200, 64);
        button.set_tooltip_text(Some(&format!("Choose answer {}", index + 1)));

        answer_grid.attach(&button, index % 2, index / 2, 1, 1);
        answer_buttons.push(button);
        answer_labels.push(text);
    }

    // Keep the feedback line allocated even when empty so that a wrong
    // answer does not resize the card.
    let answer_feedback = gtk::Label::new(Some(" "));
    answer_feedback.add_css_class("answer-feedback");
    answer_feedback.set_halign(gtk::Align::Center);

    let writing_pad = WritingPad::new();
    writing_pad.root().set_visible(false);

    let continue_button = gtk::Button::with_label("Continue");
    continue_button.add_css_class("suggested-action");
    continue_button.add_css_class("pill");
    continue_button.set_halign(gtk::Align::Center);
    continue_button.set_margin_top(6);
    continue_button.set_visible(false);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    actions.set_halign(gtk::Align::Center);
    actions.append(&audio_button);
    actions.append(&custom_group_button);

    // The quiz choices and the revealed pinyin/meaning occupy the same box.
    // In quiz mode that box has a fixed height, so revealing the answer never
    // moves the character, the buttons, or the rest of the card.
    let choices = gtk::Box::new(gtk::Orientation::Vertical, 6);
    choices.set_halign(gtk::Align::Fill);
    choices.set_valign(gtk::Align::Center);
    choices.append(&answer_grid);
    choices.append(&answer_feedback);

    let reveal = gtk::Box::new(gtk::Orientation::Vertical, 8);
    reveal.set_halign(gtk::Align::Fill);
    reveal.set_valign(gtk::Align::Center);
    reveal.append(&pinyin);
    reveal.append(&meaning);
    reveal.append(&continue_button);

    let stage = gtk::Stack::new();
    stage.set_vhomogeneous(false);
    stage.set_transition_type(gtk::StackTransitionType::Crossfade);
    stage.set_transition_duration(140);
    stage.add_named(&choices, Some("choices"));
    stage.add_named(&reveal, Some("reveal"));
    stage.set_visible_child_name("choices");

    card.append(&glyph);
    card.append(&traditional);
    card.append(&actions);
    card.append(&audio_prompt_panel);
    card.append(&stage);
    card.append(writing_pad.root());

    let hint = gtk::Label::new(Some("1–4 choose · Space continues"));
    hint.add_css_class("hint");
    hint.set_halign(gtk::Align::Center);
    hint.set_wrap(true);
    hint.set_justify(gtk::Justification::Center);
    hint.set_max_width_chars(72);

    content.append(&progress_label);
    content.append(&progress);
    content.append(&card);
    content.append(&hint);

    let root = gtk::ScrolledWindow::new();
    root.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    root.set_vexpand(true);
    root.set_child(Some(&content));

    ReviewWidgets {
        root,
        progress_label,
        progress,
        glyph,
        traditional,
        pinyin,
        meaning,
        audio_button,
        answer_grid,
        answer_buttons,
        answer_labels,
        answer_feedback,
        stage,
        continue_button,
        audio_prompt_panel,
        audio_prompt_play_button,
        custom_group_popover,
        custom_group_box,
        writing_pad,
        hint,
    }
}

fn build_summary() -> (adw::StatusPage, gtk::Button) {
    let summary = adw::StatusPage::builder()
        .icon_name("object-select-symbolic")
        .title("Group complete")
        .description("Symbols trained.")
        .build();

    let back_button = gtk::Button::with_label("Back to group");
    back_button.add_css_class("suggested-action");
    back_button.add_css_class("pill");
    back_button.set_halign(gtk::Align::Center);
    back_button.set_margin_top(12);

    summary.set_child(Some(&back_button));
    (summary, back_button)
}

fn mastery_class(level: u8) -> Option<&'static str> {
    match level {
        1 => Some("mastery-1"),
        2 => Some("mastery-2"),
        3 => Some("mastery-3"),
        4 => Some("mastery-4"),
        5 => Some("mastery-5"),
        _ => None,
    }
}

fn apply_mastery<W: IsA<gtk::Widget>>(widget: &W, level: u8) {
    for step in 0..=5 {
        widget.remove_css_class(&format!("mastery-{step}"));
    }
    if let Some(class) = mastery_class(level) {
        widget.add_css_class(class);
    }
}

fn make_group_card(group: &GroupSummary) -> gtk::Button {
    let card = gtk::Button::new();
    card.add_css_class("group-card");

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);

    let title = gtk::Label::new(Some(&group.name));
    title.add_css_class("title-4");
    title.set_halign(gtk::Align::Start);

    let count = gtk::Label::new(Some(&if group.custom {
        format!("{} symbols · custom group", group.count)
    } else {
        format!("{} symbols", group.count)
    }));
    count.add_css_class("dim-label");
    count.set_halign(gtk::Align::Start);

    text.append(&title);
    text.append(&count);

    if group.mastery > 0 {
        let badge = gtk::Label::new(Some(&format!("★ {}", group.mastery)));
        badge.add_css_class("mastery-badge");
        badge.set_valign(gtk::Align::Center);
        badge.set_tooltip_text(Some(&format!(
            "Every entry has reached mastery {}/5",
            group.mastery
        )));
        content.append(&badge);
    }

    let arrow = gtk::Image::from_icon_name("go-next-symbolic");
    arrow.set_valign(gtk::Align::Center);

    content.append(&text);
    content.append(&arrow);
    card.set_child(Some(&content));
    card
}

fn apply_color_scheme(scheme: &str) {
    let manager = adw::StyleManager::default();
    let color_scheme = match scheme {
        "light" => adw::ColorScheme::ForceLight,
        "dark" => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    };
    manager.set_color_scheme(color_scheme);
}

fn show_about(parent: &adw::ApplicationWindow) {
    let about = adw::AboutDialog::builder()
        .application_name("Hanzi")
        .application_icon(APP_ID)
        .developer_name("horooka")
        .version("0.1.0")
        .website("https://github.com/horooka/hanzi")
        .license_type(gtk::License::MitX11)
        .developers(["horooka"])
        .comments(
            "Stroke-order data: Make Me a Hanzi / hanzi-writer-data \
             (Arphic Public License). Definitions adapted from CC-CEDICT and \
             the HSK 3.0 2025 vocabulary dataset (CC BY-SA 4.0).",
        )
        .build();

    about.present(Some(parent));
}

fn seed_rng() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos() as u64)
        .unwrap_or(0x9e37_79b9_7f4a_7c15)
        | 1
}

fn next_rand(state: &mut u64) -> u64 {
    let mut value = *state;
    value ^= value << 13;
    value ^= value >> 7;
    value ^= value << 17;
    *state = value;
    value
}

fn shuffle<T>(items: &mut [T], state: &mut u64) {
    for index in (1..items.len()).rev() {
        let other = (next_rand(state) % (index as u64 + 1)) as usize;
        items.swap(index, other);
    }
}
