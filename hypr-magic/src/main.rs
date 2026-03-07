use std::cell::RefCell;
use std::env;
use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use glib::ControlFlow;
use gtk::gdk;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, HeaderBar, Label, Orientation,
    ScrolledWindow, TextView, WindowHandle,
};
use gtk4 as gtk;
use reqwest::blocking::Client;
use serde_json::{Value, json};

const APP_ID: &str = "com.seydemann.hyprmagic";
const DEFAULT_GEMINI_MODEL: &str = "gemini-3.1-flash-lite-preview";
const DEFAULT_GEMINI_THINKING_LEVEL: &str = "minimal";
const DEFAULT_PW_RECORD_CMD: &str = "pw-record";
const DEFAULT_WHISPER_CMD: &str = "whisper-cpp";
const DEFAULT_WHISPER_LANG: &str = "en";
const GEMINI_LOGO_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/gemini-logo.svg");
const SYSTEM_PROMPT: &str = "Linguistically polish the text in <raw>. Preserve the author's voice, personality, tone, and style — elevate, never replace. Find more idiomatic phrasing where natural, eliminate bloat, and fix grammatical, lexical, and punctuation errors. Do not restructure, reconstruct, or re-engineer the content. Every edit should unseal, not substitute. Where the author reached for a phrase and fell short, complete the arch they were already building — reveal what they were on the verge of writing, never impose what they weren't. The goal is not correction but emancipation: widen the bottleneck between thought and expression until what arrives on the page is proportionate to what was luminous in the mind. Surgical, enriching changes only.";
const SIGNAL_INTERRUPT: i32 = 2;

#[derive(Clone)]
struct AppConfig {
    gemini_api_key: Option<String>,
    gemini_model: String,
    gemini_thinking_level: String,
    http_client: Client,
    pw_record_cmd: String,
    whisper_cmd: String,
    whisper_model_path: Option<String>,
    whisper_lang: String,
}

#[derive(Debug)]
struct RecordingState {
    child: Child,
    wav_path: PathBuf,
}

struct PolishHistory {
    original: String,
    polished: String,
    showing_polished: bool,
}

impl PolishHistory {
    fn new(original: String, polished: String) -> Self {
        Self {
            original,
            polished,
            showing_polished: true,
        }
    }

    fn can_undo(&self) -> bool {
        self.showing_polished
    }

    fn can_redo(&self) -> bool {
        !self.showing_polished
    }

    fn undo(&mut self) -> Option<&str> {
        if !self.can_undo() {
            return None;
        }

        self.showing_polished = false;
        Some(&self.original)
    }

    fn redo(&mut self) -> Option<&str> {
        if !self.can_redo() {
            return None;
        }

        self.showing_polished = true;
        Some(&self.polished)
    }
}

#[derive(Clone)]
struct ActiveMagic {
    cancel_flag: Arc<AtomicBool>,
    original_input: String,
}

enum MagicUpdate {
    Chunk(String),
    Finished,
    Error(String),
}

fn main() {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        let config = match load_config() {
            Ok(config) => config,
            Err(msg) => {
                eprintln!("{msg}");
                let win = ApplicationWindow::builder()
                    .application(app)
                    .title("Hypr Magic")
                    .default_width(480)
                    .default_height(120)
                    .build();
                let label = Label::new(Some(&msg));
                label.set_wrap(true);
                label.set_margin_top(16);
                label.set_margin_bottom(16);
                label.set_margin_start(16);
                label.set_margin_end(16);
                win.set_child(Some(&label));
                win.present();
                return;
            }
        };
        install_css();
        build_ui(app, config);
    });

    app.run();
}

fn load_config() -> Result<AppConfig, String> {
    let gemini_api_key = env::var("GEMINI_API_KEY").ok();
    let gemini_model =
        env::var("GEMINI_MODEL").unwrap_or_else(|_| DEFAULT_GEMINI_MODEL.to_string());
    let gemini_thinking_level = env::var("GEMINI_THINKING_LEVEL")
        .unwrap_or_else(|_| default_gemini_thinking_level(&gemini_model).to_string());
    let http_client = Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|err| format!("HTTP client init failed: {err}"))?;
    let pw_record_cmd =
        env::var("PW_RECORD_CMD").unwrap_or_else(|_| DEFAULT_PW_RECORD_CMD.to_string());
    let whisper_cmd = env::var("WHISPER_CMD").unwrap_or_else(|_| DEFAULT_WHISPER_CMD.to_string());
    let whisper_model_path = env::var("WHISPER_MODEL_PATH").ok();
    let whisper_lang = env::var("WHISPER_LANG").unwrap_or_else(|_| DEFAULT_WHISPER_LANG.to_string());

    Ok(AppConfig {
        gemini_api_key,
        gemini_model,
        gemini_thinking_level,
        http_client,
        pw_record_cmd,
        whisper_cmd,
        whisper_model_path,
        whisper_lang,
    })
}

fn install_css() {
    let provider = gtk::CssProvider::new();
    let css = r#"
    .icon-window {
      background: transparent;
    }

    .icon-handle,
    .icon-handle:focus,
    .icon-handle:focus-visible {
      background: transparent;
      border: none;
      box-shadow: none;
      outline: none;
    }

    .icon-shell {
      background: alpha(currentColor, 0.06);
      border: 1px solid alpha(currentColor, 0.12);
      border-radius: 14px;
      box-shadow: none;
      padding: 4px;
      transition: background-color 180ms ease, border-color 180ms ease;
    }

    .icon-shell:hover {
      background: alpha(currentColor, 0.10);
      border-color: alpha(currentColor, 0.18);
    }

    .icon-picture {
      margin: 0;
    }

    .title-stack {
      margin: 2px 0;
    }

    .window-title {
      font-size: 14px;
      font-weight: 700;
    }

    .window-subtitle {
      color: alpha(currentColor, 0.50);
      font-size: 11px;
      font-weight: 500;
    }

    .header-status {
      margin-start: 12px;
      min-width: 150px;
    }

    .editor-surface {
      background: alpha(currentColor, 0.03);
      border: 1px solid alpha(currentColor, 0.12);
      border-radius: 10px;
      transition: border-color 180ms ease, background-color 180ms ease;
    }

    .editor-surface textview,
    .editor-surface textview text {
      background: transparent;
      font-size: 15px;
    }

    .editor-surface textview text {
      caret-color: #7aa2f7;
    }

    .editor-surface.streaming {
      border-color: #4a6cf7;
      background: alpha(#4a6cf7, 0.05);
    }

    .actions-row {
      margin-top: 2px;
    }

    .primary-group,
    .history-group,
    .utility-group {
      border-spacing: 0;
    }

    .magic-btn {
      font-weight: 700;
      background: #2b4daa;
      color: #edf2ff;
      min-width: 84px;
      min-height: 34px;
      padding: 6px 16px;
      border-radius: 8px;
      transition: background-color 160ms ease, box-shadow 160ms ease, transform 160ms ease;
    }

    .magic-btn:hover {
      background: #3559bb;
    }

    .magic-btn.polishing {
      animation: polish-pulse 1.8s ease-in-out infinite;
    }

    .mic-btn {
      min-width: 68px;
      min-height: 34px;
      padding: 6px 12px;
      border-radius: 8px;
      background: alpha(currentColor, 0.08);
      border: 1px solid alpha(currentColor, 0.12);
      transition: background-color 160ms ease, border-color 160ms ease, opacity 160ms ease;
    }

    .mic-btn:hover {
      background: alpha(currentColor, 0.12);
    }

    .mic-btn.recording {
      background: #8a1d14;
      color: #fff6f3;
      border-color: #8a1d14;
      font-weight: 700;
      animation: record-pulse 1.25s ease-in-out infinite;
    }

    .quiet-btn {
      min-height: 32px;
      padding: 5px 12px;
      border-radius: 8px;
      background: alpha(currentColor, 0.04);
      border: 1px solid alpha(currentColor, 0.10);
      transition: background-color 160ms ease, border-color 160ms ease, opacity 160ms ease;
    }

    .quiet-btn:hover {
      background: alpha(currentColor, 0.08);
    }

    button:disabled {
      opacity: 0.38;
    }

    .status-label {
      font-size: 12px;
      font-weight: 500;
      color: alpha(currentColor, 0.52);
    }

    .status-active {
      color: #7aa2f7;
    }

    .status-success {
      color: #73c991;
    }

    .status-error {
      color: #d4453b;
    }

    .status-recording {
      color: #d4453b;
      font-weight: 700;
    }

    @keyframes polish-pulse {
      0%, 100% {
        background: #2b4daa;
      }
      50% {
        background: #4368cb;
      }
    }

    @keyframes record-pulse {
      0%, 100% {
        background: #8a1d14;
      }
      50% {
        background: #b13024;
      }
    }
    "#;

    provider.load_from_data(css);

    if let Some(display) = gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

fn build_ui(app: &Application, config: AppConfig) {
    let icon_window = ApplicationWindow::builder()
        .application(app)
        .title("Hypr Magic Icon")
        .default_width(56)
        .default_height(56)
        .resizable(false)
        .decorated(false)
        .build();
    icon_window.set_hide_on_close(false);
    icon_window.set_can_focus(false);
    icon_window.add_css_class("icon-window");
    let app_for_quit = app.clone();
    icon_window.connect_close_request(move |_| {
        app_for_quit.quit();
        glib::Propagation::Proceed
    });

    let handle = WindowHandle::new();
    handle.add_css_class("icon-handle");
    let icon_shell = GtkBox::new(Orientation::Vertical, 0);
    icon_shell.set_halign(gtk::Align::Center);
    icon_shell.set_valign(gtk::Align::Center);
    icon_shell.set_width_request(56);
    icon_shell.set_height_request(56);
    icon_shell.set_overflow(gtk::Overflow::Hidden);
    icon_shell.add_css_class("icon-shell");

    let icon_picture = gtk::Picture::for_filename(GEMINI_LOGO_PATH);
    icon_picture.set_keep_aspect_ratio(true);
    icon_picture.set_can_shrink(true);
    icon_picture.set_width_request(44);
    icon_picture.set_height_request(44);
    icon_picture.add_css_class("icon-picture");
    icon_shell.append(&icon_picture);

    handle.set_child(Some(&icon_shell));
    icon_window.set_child(Some(&handle));

    let editor_window = ApplicationWindow::builder()
        .application(app)
        .title("Hypr Magic")
        .default_width(640)
        .default_height(420)
        .build();
    editor_window.set_hide_on_close(true);
    editor_window.set_transient_for(Some(&icon_window));

    let title_box = GtkBox::new(Orientation::Vertical, 0);
    title_box.add_css_class("title-stack");
    let title_label = Label::new(Some("Hypr Magic"));
    title_label.set_xalign(0.0);
    title_label.add_css_class("window-title");
    let subtitle_label = Label::new(Some("Scratchpad"));
    subtitle_label.set_xalign(0.0);
    subtitle_label.add_css_class("window-subtitle");
    title_box.append(&title_label);
    title_box.append(&subtitle_label);

    let status = Label::new(Some("Idle"));
    status.set_xalign(1.0);
    status.add_css_class("status-label");
    status.add_css_class("header-status");

    let header_bar = HeaderBar::new();
    header_bar.set_show_title_buttons(true);
    header_bar.set_title_widget(Some(&title_box));
    header_bar.pack_end(&status);
    editor_window.set_titlebar(Some(&header_bar));

    let root = GtkBox::new(Orientation::Vertical, 10);
    root.set_margin_top(16);
    root.set_margin_bottom(16);
    root.set_margin_start(16);
    root.set_margin_end(16);

    let scroller = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();
    scroller.add_css_class("editor-surface");

    let text_view = TextView::new();
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);
    text_view.set_monospace(false);
    text_view.set_left_margin(12);
    text_view.set_right_margin(12);
    text_view.set_top_margin(8);
    text_view.set_bottom_margin(8);
    scroller.set_child(Some(&text_view));

    let actions = GtkBox::new(Orientation::Horizontal, 0);
    actions.add_css_class("actions-row");
    let primary_group = GtkBox::new(Orientation::Horizontal, 6);
    primary_group.add_css_class("primary-group");
    let history_group = GtkBox::new(Orientation::Horizontal, 6);
    history_group.add_css_class("history-group");
    let utility_group = GtkBox::new(Orientation::Horizontal, 6);
    utility_group.add_css_class("utility-group");
    utility_group.set_margin_start(14);
    let spacer = GtkBox::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    let magic_button = Button::with_label("Magic");
    magic_button.add_css_class("magic-btn");
    magic_button.set_tooltip_text(Some("Polish text (Ctrl+Enter)"));
    let mic_button = Button::with_label("Mic");
    mic_button.add_css_class("mic-btn");
    mic_button.set_tooltip_text(Some("Voice input (Ctrl+Shift+M)"));
    let undo_button = Button::with_label("Undo");
    undo_button.add_css_class("quiet-btn");
    undo_button.set_tooltip_text(Some("Undo polish (Ctrl+Z)"));
    undo_button.set_sensitive(false);
    let redo_button = Button::with_label("Redo");
    redo_button.add_css_class("quiet-btn");
    redo_button.set_tooltip_text(Some("Redo polish (Ctrl+Y / Ctrl+Shift+Z)"));
    redo_button.set_sensitive(false);
    let copy_button = Button::with_label("Copy");
    copy_button.add_css_class("quiet-btn");
    copy_button.set_tooltip_text(Some("Copy to clipboard"));
    let clear_button = Button::with_label("Clear");
    clear_button.add_css_class("quiet-btn");
    clear_button.set_tooltip_text(Some("Clear scratchpad"));

    primary_group.append(&magic_button);
    primary_group.append(&mic_button);
    history_group.append(&undo_button);
    history_group.append(&redo_button);
    utility_group.append(&copy_button);
    utility_group.append(&clear_button);

    actions.append(&primary_group);
    actions.append(&spacer);
    actions.append(&history_group);
    actions.append(&utility_group);

    root.append(&scroller);
    root.append(&actions);

    editor_window.set_child(Some(&root));

    let editor_for_icon = editor_window.clone();
    let click = gtk::GestureClick::new();
    click.connect_released(move |_, _, _, _| {
        if editor_for_icon.is_visible() {
            editor_for_icon.hide();
        } else {
            editor_for_icon.present();
        }
    });
    handle.add_controller(click);

    let recording_state: Rc<RefCell<Option<RecordingState>>> = Rc::new(RefCell::new(None));
    let recording_started_at: Rc<RefCell<Option<Instant>>> = Rc::new(RefCell::new(None));
    let transcription_running = Rc::new(RefCell::new(false));
    let active_magic: Rc<RefCell<Option<ActiveMagic>>> = Rc::new(RefCell::new(None));
    let polish_history: Rc<RefCell<Option<PolishHistory>>> = Rc::new(RefCell::new(None));
    let suppress_history_reset = Rc::new(RefCell::new(false));

    {
        let recording_state = recording_state.clone();
        let recording_started_at = recording_started_at.clone();
        let active_magic = active_magic.clone();
        app.connect_shutdown(move |_| {
            *recording_started_at.borrow_mut() = None;
            if let Some(active) = active_magic.borrow_mut().take() {
                active.cancel_flag.store(true, Ordering::Relaxed);
            }
            if let Some(mut state) = recording_state.borrow_mut().take() {
                let _ = stop_recording(&mut state.child);
                let _ = fs::remove_file(&state.wav_path);
            }
        });
    }

    {
        let buffer = text_view.buffer();
        let undo_button = undo_button.clone();
        let redo_button = redo_button.clone();
        let polish_history = polish_history.clone();
        let suppress_history_reset = suppress_history_reset.clone();
        buffer.connect_changed(move |_| {
            if *suppress_history_reset.borrow() {
                return;
            }
            if clear_polish_history(&mut polish_history.borrow_mut()) {
                sync_history_buttons(&undo_button, &redo_button, &polish_history.borrow());
            }
        });
    }

    {
        let text_view = text_view.clone();
        copy_button.connect_clicked(move |_| {
            let buffer = text_view.buffer();
            let start = buffer.start_iter();
            let end = buffer.end_iter();
            let text = buffer.text(&start, &end, false).to_string();
            if let Some(display) = gdk::Display::default() {
                let clipboard = display.clipboard();
                clipboard.set_text(&text);
            }
        });
    }

    {
        let text_view = text_view.clone();
        clear_button.connect_clicked(move |_| {
            text_view.buffer().set_text("");
        });
    }

    {
        let text_view = text_view.clone();
        let undo_button = undo_button.clone();
        let undo_button_for_click = undo_button.clone();
        let redo_button = redo_button.clone();
        let status = status.clone();
        let polish_history = polish_history.clone();
        let suppress_history_reset = suppress_history_reset.clone();
        undo_button_for_click.connect_clicked(move |_| {
            let mut history = polish_history.borrow_mut();
            let Some(text) = undo_polish_history(&mut history) else {
                return;
            };
            set_buffer_text(&text_view, &suppress_history_reset, &text);
            set_status(&status, "Restored original", "status-success");
            sync_history_buttons(&undo_button, &redo_button, &history);
        });
    }

    {
        let text_view = text_view.clone();
        let undo_button = undo_button.clone();
        let redo_button = redo_button.clone();
        let redo_button_for_click = redo_button.clone();
        let status = status.clone();
        let polish_history = polish_history.clone();
        let suppress_history_reset = suppress_history_reset.clone();
        redo_button_for_click.connect_clicked(move |_| {
            let mut history = polish_history.borrow_mut();
            let Some(text) = redo_polish_history(&mut history) else {
                return;
            };
            set_buffer_text(&text_view, &suppress_history_reset, &text);
            set_status(&status, "Restored polished", "status-success");
            sync_history_buttons(&undo_button, &redo_button, &history);
        });
    }

    {
        let text_view = text_view.clone();
        let scroller = scroller.clone();
        let status = status.clone();
        let magic_button = magic_button.clone();
        let magic_button_for_handler = magic_button.clone();
        let mic_button = mic_button.clone();
        let undo_button = undo_button.clone();
        let redo_button = redo_button.clone();
        let clear_button = clear_button.clone();
        let recording_state = recording_state.clone();
        let transcription_running = transcription_running.clone();
        let active_magic = active_magic.clone();
        let polish_history = polish_history.clone();
        let config = config.clone();

        magic_button.connect_clicked(move |_| {
            if let Some(active) = active_magic.borrow_mut().take() {
                active.cancel_flag.store(true, Ordering::Relaxed);
                text_view.buffer().set_text(&active.original_input);
                scroll_to_end(&text_view);
                scroller.remove_css_class("streaming");
                set_status(&status, "Canceled", "");
                clear_polish_history(&mut polish_history.borrow_mut());
                set_idle_ui(
                    &magic_button_for_handler,
                    &mic_button,
                    &undo_button,
                    &redo_button,
                    &clear_button,
                    false,
                    false,
                );
                return;
            }

            if recording_state.borrow().is_some() || *transcription_running.borrow() {
                set_status(&status, "Finish recording before polishing.", "status-error");
                return;
            }

            let buffer = text_view.buffer();
            let start = buffer.start_iter();
            let end = buffer.end_iter();
            let input = buffer.text(&start, &end, false).to_string();

            if input.trim().is_empty() {
                set_status(&status, "Scratchpad is empty.", "status-error");
                return;
            }

            set_status(&status, "Polishing...", "status-active");
            clear_polish_history(&mut polish_history.borrow_mut());
            sync_history_buttons(&undo_button, &redo_button, &polish_history.borrow());
            set_polishing_ui(
                &magic_button_for_handler,
                &mic_button,
                &undo_button,
                &redo_button,
                &clear_button,
            );
            scroller.add_css_class("streaming");
            let (tx, rx) = mpsc::channel::<MagicUpdate>();
            let config = config.clone();
            let text_view = text_view.clone();
            let scroller = scroller.clone();
            let status = status.clone();
            let magic_button_for_handler = magic_button_for_handler.clone();
            let mic_button = mic_button.clone();
            let undo_button = undo_button.clone();
            let redo_button = redo_button.clone();
            let clear_button = clear_button.clone();
            let active_magic = active_magic.clone();
            let polish_history = polish_history.clone();
            let original_input = input.clone();
            let cancel_flag = Arc::new(AtomicBool::new(false));
            let idle_cancel_flag = cancel_flag.clone();
            *active_magic.borrow_mut() = Some(ActiveMagic {
                cancel_flag: cancel_flag.clone(),
                original_input: original_input.clone(),
            });
            let mut started_stream = false;

            glib::idle_add_local(move || {
                if idle_cancel_flag.load(Ordering::Relaxed) {
                    return ControlFlow::Break;
                }

                match rx.try_recv() {
                    Ok(update) => {
                        match update {
                            MagicUpdate::Chunk(chunk) => {
                                if !started_stream {
                                    text_view.buffer().set_text("");
                                    started_stream = true;
                                }
                                append_to_buffer(&text_view, &chunk);
                                scroll_to_end(&text_view);
                                set_status(&status, "Polishing...", "status-active");
                                ControlFlow::Continue
                            }
                            MagicUpdate::Finished => {
                                *active_magic.borrow_mut() = None;
                                scroller.remove_css_class("streaming");
                                let polished = buffer_text(&text_view);
                                set_polish_result(
                                    &mut polish_history.borrow_mut(),
                                    original_input.clone(),
                                    polished,
                                );
                                set_status(&status, "Done", "status-success");
                                set_idle_ui(
                                    &magic_button_for_handler,
                                    &mic_button,
                                    &undo_button,
                                    &redo_button,
                                    &clear_button,
                                    true,
                                    false,
                                );
                                ControlFlow::Break
                            }
                            MagicUpdate::Error(err) => {
                                *active_magic.borrow_mut() = None;
                                scroller.remove_css_class("streaming");
                                clear_polish_history(&mut polish_history.borrow_mut());
                                if started_stream {
                                    text_view.buffer().set_text(&original_input);
                                    scroll_to_end(&text_view);
                                }
                                set_status(&status, &format!("Error: {err}"), "status-error");
                                set_idle_ui(
                                    &magic_button_for_handler,
                                    &mic_button,
                                    &undo_button,
                                    &redo_button,
                                    &clear_button,
                                    false,
                                    false,
                                );
                                ControlFlow::Break
                            }
                        }
                    }
                    Err(mpsc::TryRecvError::Empty) => ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        *active_magic.borrow_mut() = None;
                        scroller.remove_css_class("streaming");
                        clear_polish_history(&mut polish_history.borrow_mut());
                        if started_stream {
                            text_view.buffer().set_text(&original_input);
                            scroll_to_end(&text_view);
                        }
                        set_status(&status, "Error: worker thread disconnected", "status-error");
                        set_idle_ui(
                            &magic_button_for_handler,
                            &mic_button,
                            &undo_button,
                            &redo_button,
                            &clear_button,
                            false,
                            false,
                        );
                        ControlFlow::Break
                    }
                }
            });

            thread::spawn(move || {
                polish_text_streaming(&config, &input, cancel_flag, tx);
            });
        });
    }

    {
        let text_view = text_view.clone();
        let status = status.clone();
        let magic_button = magic_button.clone();
        let mic_button = mic_button.clone();
        let mic_button_for_click = mic_button.clone();
        let undo_button = undo_button.clone();
        let redo_button = redo_button.clone();
        let clear_button = clear_button.clone();
        let recording_state = recording_state.clone();
        let recording_started_at = recording_started_at.clone();
        let transcription_running = transcription_running.clone();
        let polish_history = polish_history.clone();
        let config = config.clone();

        mic_button_for_click.connect_clicked(move |_| {
            if *transcription_running.borrow() {
                return;
            }

            if recording_state.borrow().is_none() {
                if let Err(err) = ensure_voice_config(&config) {
                    set_status(&status, &format!("Error: {err}"), "status-error");
                    return;
                }

                match start_recording(&config) {
                    Ok(state) => {
                        *recording_state.borrow_mut() = Some(state);
                        *recording_started_at.borrow_mut() = Some(Instant::now());
                        set_status(&status, "Recording... 0:00", "status-recording");
                        set_recording_ui(
                            &magic_button,
                            &mic_button,
                            &undo_button,
                            &redo_button,
                            &clear_button,
                        );
                        start_recording_timer(&status, &recording_started_at);
                    }
                    Err(err) => set_status(&status, &format!("Error: {err}"), "status-error"),
                }
                return;
            }

            let Some(state) = recording_state.borrow_mut().take() else {
                return;
            };

            *transcription_running.borrow_mut() = true;
            *recording_started_at.borrow_mut() = None;
            set_status(&status, "Transcribing...", "status-active");
            set_transcribing_ui(
                &magic_button,
                &mic_button,
                &undo_button,
                &redo_button,
                &clear_button,
            );

            let (tx, rx) = mpsc::channel::<Result<String, String>>();
            let config = config.clone();
            let text_view = text_view.clone();
            let status = status.clone();
            let magic_button = magic_button.clone();
            let mic_button = mic_button.clone();
            let undo_button = undo_button.clone();
            let redo_button = redo_button.clone();
            let clear_button = clear_button.clone();
            let transcription_running = transcription_running.clone();
            let polish_history = polish_history.clone();

            glib::idle_add_local(move || match rx.try_recv() {
                Ok(result) => {
                    *transcription_running.borrow_mut() = false;
                    match result {
                        Ok(transcript) => {
                            append_transcript(&text_view, &transcript);
                            set_status(&status, "Done", "status-success");
                        }
                        Err(err) => {
                            set_status(&status, &format!("Error: {err}"), "status-error");
                        }
                    }
                    set_idle_ui(
                        &magic_button,
                        &mic_button,
                        &undo_button,
                        &redo_button,
                        &clear_button,
                        history_can_undo(&polish_history.borrow()),
                        history_can_redo(&polish_history.borrow()),
                    );
                    ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    *transcription_running.borrow_mut() = false;
                    set_status(&status, "Error: worker thread disconnected", "status-error");
                    set_idle_ui(
                        &magic_button,
                        &mic_button,
                        &undo_button,
                        &redo_button,
                        &clear_button,
                        history_can_undo(&polish_history.borrow()),
                        history_can_redo(&polish_history.borrow()),
                    );
                    ControlFlow::Break
                }
            });

            thread::spawn(move || {
                let _ = tx.send(finalize_recording_and_transcribe(&config, state));
            });
        });
    }

    {
        let editor_window = editor_window.clone();
        let editor_window_for_key = editor_window.clone();
        let magic_button = magic_button.clone();
        let mic_button = mic_button.clone();
        let undo_button = undo_button.clone();
        let redo_button = redo_button.clone();
        let key = gtk::EventControllerKey::new();
        key.connect_key_pressed(move |_, keyval, _, state| {
            let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
            let shift = state.contains(gdk::ModifierType::SHIFT_MASK);

            if keyval == gdk::Key::Escape {
                editor_window_for_key.hide();
                return glib::Propagation::Stop;
            }

            if ctrl && (keyval == gdk::Key::Return || keyval == gdk::Key::KP_Enter) {
                if magic_button.is_sensitive() {
                    magic_button.emit_clicked();
                }
                return glib::Propagation::Stop;
            }

            if ctrl && shift && keyval == gdk::Key::M {
                if mic_button.is_sensitive() {
                    mic_button.emit_clicked();
                }
                return glib::Propagation::Stop;
            }

            if ctrl && keyval == gdk::Key::z {
                if undo_button.is_sensitive() {
                    undo_button.emit_clicked();
                }
                return glib::Propagation::Stop;
            }

            if (ctrl && keyval == gdk::Key::y) || (ctrl && shift && keyval == gdk::Key::Z) {
                if redo_button.is_sensitive() {
                    redo_button.emit_clicked();
                }
                return glib::Propagation::Stop;
            }

            glib::Propagation::Proceed
        });
        editor_window.add_controller(key);
    }

    editor_window.hide();
    icon_window.present();
}

fn set_idle_ui(
    magic_button: &Button,
    mic_button: &Button,
    undo_button: &Button,
    redo_button: &Button,
    clear_button: &Button,
    can_undo: bool,
    can_redo: bool,
) {
    magic_button.set_sensitive(true);
    magic_button.set_label("Magic");
    magic_button.remove_css_class("polishing");
    mic_button.set_sensitive(true);
    mic_button.remove_css_class("recording");
    mic_button.set_label("Mic");
    undo_button.set_sensitive(can_undo);
    redo_button.set_sensitive(can_redo);
    clear_button.set_sensitive(true);
}

fn set_recording_ui(
    magic_button: &Button,
    mic_button: &Button,
    undo_button: &Button,
    redo_button: &Button,
    clear_button: &Button,
) {
    magic_button.set_sensitive(false);
    magic_button.remove_css_class("polishing");
    mic_button.set_sensitive(true);
    mic_button.add_css_class("recording");
    mic_button.set_label("Stop");
    undo_button.set_sensitive(false);
    redo_button.set_sensitive(false);
    clear_button.set_sensitive(false);
}

fn set_transcribing_ui(
    magic_button: &Button,
    mic_button: &Button,
    undo_button: &Button,
    redo_button: &Button,
    clear_button: &Button,
) {
    magic_button.set_sensitive(false);
    magic_button.remove_css_class("polishing");
    mic_button.set_sensitive(false);
    mic_button.remove_css_class("recording");
    mic_button.set_label("Mic");
    undo_button.set_sensitive(false);
    redo_button.set_sensitive(false);
    clear_button.set_sensitive(false);
}

fn set_polishing_ui(
    magic_button: &Button,
    mic_button: &Button,
    undo_button: &Button,
    redo_button: &Button,
    clear_button: &Button,
) {
    magic_button.set_sensitive(true);
    magic_button.set_label("Cancel");
    magic_button.add_css_class("polishing");
    mic_button.set_sensitive(false);
    undo_button.set_sensitive(false);
    redo_button.set_sensitive(false);
    clear_button.set_sensitive(false);
}

fn set_status(status: &Label, text: &str, class: &str) {
    for css_class in [
        "status-active",
        "status-success",
        "status-error",
        "status-recording",
    ] {
        status.remove_css_class(css_class);
    }

    if !class.is_empty() {
        status.add_css_class(class);
    }

    status.set_text(text);
}

fn ensure_voice_config(config: &AppConfig) -> Result<(), String> {
    let Some(model_path) = config.whisper_model_path.as_deref() else {
        return Err(
            "Voice input unavailable: set WHISPER_MODEL_PATH to a local whisper.cpp model file."
                .to_string(),
        );
    };

    if !Path::new(model_path).exists() {
        return Err(format!("WHISPER_MODEL_PATH does not exist: {model_path}"));
    }

    Ok(())
}

fn start_recording(config: &AppConfig) -> Result<RecordingState, String> {
    let wav_path = next_recording_path();
    let child = Command::new(&config.pw_record_cmd)
        .args(["--format", "s16", "--rate", "16000", "--channels", "1"])
        .arg(&wav_path)
        .spawn()
        .map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => {
                format!(
                    "{} not found. Install PipeWire tools or set PW_RECORD_CMD.",
                    config.pw_record_cmd
                )
            }
            _ => format!("Failed to start {}: {err}", config.pw_record_cmd),
        })?;

    Ok(RecordingState { child, wav_path })
}

fn finalize_recording_and_transcribe(
    config: &AppConfig,
    mut state: RecordingState,
) -> Result<String, String> {
    stop_recording(&mut state.child)?;

    if !state.wav_path.exists() {
        return Err("Recording failed: no audio file was created.".to_string());
    }

    let result = transcribe_audio(config, &state.wav_path);
    let _ = fs::remove_file(&state.wav_path);
    result
}

fn stop_recording(child: &mut Child) -> Result<(), String> {
    interrupt_process(child.id()).map_err(|err| format!("Failed to stop recording: {err}"))?;

    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return Ok(()),
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(50)),
            Ok(None) => {
                child
                    .kill()
                    .map_err(|err| format!("Failed to terminate recording: {err}"))?;
                child
                    .wait()
                    .map_err(|err| format!("Failed to wait for recorder exit: {err}"))?;
                return Ok(());
            }
            Err(err) => return Err(format!("Failed while stopping recording: {err}")),
        }
    }
}

fn transcribe_audio(config: &AppConfig, wav_path: &Path) -> Result<String, String> {
    let Some(model_path) = config.whisper_model_path.as_deref() else {
        return Err(
            "Voice input unavailable: set WHISPER_MODEL_PATH to a local whisper.cpp model file."
                .to_string(),
        );
    };

    let output = Command::new(&config.whisper_cmd)
        .args([
            "-m",
            model_path,
            "-f",
            wav_path.to_string_lossy().as_ref(),
            "-l",
            &config.whisper_lang,
            "-nt",
        ])
        .output()
        .map_err(|err| match err.kind() {
            io::ErrorKind::NotFound => format!(
                "{} not found. Install whisper.cpp and set WHISPER_CMD if needed.",
                config.whisper_cmd
            ),
            _ => format!("Failed to run {}: {err}", config.whisper_cmd),
        })?;

    if !output.status.success() {
        return Err(format!(
            "{} exited with {}: {}",
            config.whisper_cmd,
            output.status,
            summarize_output(&output)
        ));
    }

    let transcript = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if transcript.is_empty() {
        return Err(format!(
            "{} returned no transcript output.",
            config.whisper_cmd
        ));
    }

    Ok(transcript)
}

fn polish_text_streaming(
    config: &AppConfig,
    raw: &str,
    cancel_flag: Arc<AtomicBool>,
    tx: mpsc::Sender<MagicUpdate>,
) {
    match polish_text_streaming_inner(config, raw, &cancel_flag, &tx) {
        Ok(()) => {
            if !cancel_flag.load(Ordering::Relaxed) {
                let _ = tx.send(MagicUpdate::Finished);
            }
        }
        Err(err) => {
            if !cancel_flag.load(Ordering::Relaxed) {
                let _ = tx.send(MagicUpdate::Error(err));
            }
        }
    }
}

fn polish_text_streaming_inner(
    config: &AppConfig,
    raw: &str,
    cancel_flag: &Arc<AtomicBool>,
    tx: &mpsc::Sender<MagicUpdate>,
) -> Result<(), String> {
    let Some(api_key) = config.gemini_api_key.as_deref() else {
        return Err("Magic unavailable: set GEMINI_API_KEY.".to_string());
    };

    let (endpoint, payload) = build_gemini_request(config, raw);

    let response = config
        .http_client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .header("x-goog-api-key", api_key)
        .json(&payload)
        .send()
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = response.status();
    if !status.is_success() {
        let body: Value = serde_json::from_reader(response)
            .map_err(|e| format!("Invalid API response JSON: {e}"))?;
        let message = body
            .get("error")
            .and_then(|v| v.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("Unknown API error");
        return Err(format!("Gemini API error ({}): {message}", status.as_u16()));
    }

    let reader = BufReader::new(response);
    consume_sse_stream(reader, cancel_flag.as_ref(), |text| {
        tx.send(MagicUpdate::Chunk(text)).is_ok()
    })
}

fn extract_text(body: &Value) -> Result<String, String> {
    let candidates = body
        .get("candidates")
        .and_then(Value::as_array)
        .ok_or_else(|| "Missing candidates in API response".to_string())?;

    let first = candidates
        .first()
        .ok_or_else(|| "Gemini returned no candidates".to_string())?;

    let parts = first
        .get("content")
        .and_then(|v| v.get("parts"))
        .and_then(Value::as_array)
        .ok_or_else(|| "Gemini response contained no text parts".to_string())?;

    let mut out = String::new();
    for part in parts {
        if let Some(text) = part.get("text").and_then(Value::as_str) {
            out.push_str(text);
        }
    }

    if out.trim().is_empty() {
        Err("Gemini returned empty text".to_string())
    } else {
        Ok(out)
    }
}

fn build_gemini_request(config: &AppConfig, raw: &str) -> (String, Value) {
    let endpoint = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:streamGenerateContent?alt=sse",
        config.gemini_model
    );

    let mut payload = json!({
        "systemInstruction": {
            "parts": [
                {"text": SYSTEM_PROMPT}
            ]
        },
        "contents": [
            {
                "parts": [
                    {
                        "text": format!("<raw>{}</raw>", escape_xml(raw))
                    }
                ]
            }
        ]
    });

    if let Some(generation_config) = gemini_generation_config(config) {
        payload["generationConfig"] = generation_config;
    }

    (endpoint, payload)
}

fn consume_sse_stream<R, F>(
    reader: R,
    cancel_flag: &AtomicBool,
    mut on_text: F,
) -> Result<(), String>
where
    R: BufRead,
    F: FnMut(String) -> bool,
{
    let mut sent_any_text = false;

    for line in reader.lines() {
        if cancel_flag.load(Ordering::Relaxed) {
            return Ok(());
        }

        let line = line.map_err(|e| format!("Stream read failed: {e}"))?;
        let line = line.trim();
        if line.is_empty() || !line.starts_with("data: ") {
            continue;
        }

        let data = &line[6..];
        if data == "[DONE]" {
            break;
        }

        let chunk: Value =
            serde_json::from_str(data).map_err(|e| format!("Invalid stream JSON: {e}"))?;
        let text = extract_text(&chunk).unwrap_or_default();
        if !text.is_empty() {
            sent_any_text = true;
            if !on_text(text) {
                return Ok(());
            }
        }
    }

    if cancel_flag.load(Ordering::Relaxed) {
        return Ok(());
    }

    if !sent_any_text {
        return Err("Gemini returned empty text".to_string());
    }

    Ok(())
}

fn default_gemini_thinking_level(model: &str) -> &'static str {
    if model.contains("flash-lite") {
        DEFAULT_GEMINI_THINKING_LEVEL
    } else {
        "low"
    }
}

fn gemini_generation_config(config: &AppConfig) -> Option<Value> {
    if !config.gemini_model.starts_with("gemini-3") {
        return None;
    }

    let thinking_level = config.gemini_thinking_level.trim();
    if thinking_level.is_empty() {
        return None;
    }

    Some(json!({
        "thinkingConfig": {
            "thinkingLevel": thinking_level
        }
    }))
}

fn append_transcript(text_view: &TextView, transcript: &str) {
    let transcript = transcript.trim();
    if transcript.is_empty() {
        return;
    }

    let buffer = text_view.buffer();
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    let existing = buffer.text(&start, &end, false).to_string();

    if existing.trim().is_empty() {
        buffer.set_text(transcript);
        return;
    }

    let separator = if existing.ends_with('\n') { "" } else { "\n" };
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, separator);
    buffer.insert(&mut end, transcript);
    scroll_to_end(text_view);
}

fn append_to_buffer(text_view: &TextView, text: &str) {
    if text.is_empty() {
        return;
    }

    let buffer = text_view.buffer();
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, text);
}

fn buffer_text(text_view: &TextView) -> String {
    let buffer = text_view.buffer();
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.text(&start, &end, false).to_string()
}

fn set_buffer_text(text_view: &TextView, suppress_history_reset: &Rc<RefCell<bool>>, text: &str) {
    *suppress_history_reset.borrow_mut() = true;
    text_view.buffer().set_text(text);
    scroll_to_end(text_view);
    *suppress_history_reset.borrow_mut() = false;
}

fn scroll_to_end(text_view: &TextView) {
    let buffer = text_view.buffer();
    let mut end = buffer.end_iter();
    text_view.scroll_to_iter(&mut end, 0.0, false, 0.0, 1.0);
}

fn history_can_undo(history: &Option<PolishHistory>) -> bool {
    history.as_ref().is_some_and(PolishHistory::can_undo)
}

fn history_can_redo(history: &Option<PolishHistory>) -> bool {
    history.as_ref().is_some_and(PolishHistory::can_redo)
}

fn clear_polish_history(history: &mut Option<PolishHistory>) -> bool {
    history.take().is_some()
}

fn set_polish_result(history: &mut Option<PolishHistory>, original: String, polished: String) {
    *history = Some(PolishHistory::new(original, polished));
}

fn undo_polish_history(history: &mut Option<PolishHistory>) -> Option<String> {
    history
        .as_mut()
        .and_then(|item| item.undo().map(ToOwned::to_owned))
}

fn redo_polish_history(history: &mut Option<PolishHistory>) -> Option<String> {
    history
        .as_mut()
        .and_then(|item| item.redo().map(ToOwned::to_owned))
}

fn sync_history_buttons(
    undo_button: &Button,
    redo_button: &Button,
    history: &Option<PolishHistory>,
) {
    undo_button.set_sensitive(history_can_undo(history));
    redo_button.set_sensitive(history_can_redo(history));
}

fn start_recording_timer(status: &Label, recording_started_at: &Rc<RefCell<Option<Instant>>>) {
    let status = status.clone();
    let recording_started_at = recording_started_at.clone();
    glib::timeout_add_local(Duration::from_millis(250), move || {
        let Some(started_at) = *recording_started_at.borrow() else {
            return ControlFlow::Break;
        };

        let elapsed = started_at.elapsed().as_secs();
        let minutes = elapsed / 60;
        let seconds = elapsed % 60;
        set_status(
            &status,
            &format!("Recording... {minutes}:{seconds:02}"),
            "status-recording",
        );
        ControlFlow::Continue
    });
}

fn next_recording_path() -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();
    env::temp_dir().join(format!("hypr-magic-{stamp}.wav"))
}

fn summarize_output(output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr;
    }

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if !stdout.is_empty() {
        return stdout;
    }

    "no output".to_string()
}

fn interrupt_process(pid: u32) -> io::Result<()> {
    let raw_pid =
        i32::try_from(pid).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid pid"))?;

    let rc = unsafe { libc_kill(raw_pid, SIGNAL_INTERRUPT) };
    if rc == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[link(name = "c")]
unsafe extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}

fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Cursor;
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::process::ExitStatusExt;
    use std::sync::atomic::AtomicBool;
    use std::sync::{Mutex, OnceLock};

    fn test_config(model: &str, thinking_level: &str) -> AppConfig {
        AppConfig {
            gemini_api_key: Some("test-key".to_string()),
            gemini_model: model.to_string(),
            gemini_thinking_level: thinking_level.to_string(),
            http_client: Client::builder().build().expect("client"),
            pw_record_cmd: DEFAULT_PW_RECORD_CMD.to_string(),
            whisper_cmd: "whisper-cpp".to_string(),
            whisper_model_path: None,
            whisper_lang: "en".to_string(),
        }
    }

    fn env_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn test_output(stdout: &[u8], stderr: &[u8]) -> Output {
        Output {
            status: std::process::ExitStatus::from_raw(0),
            stdout: stdout.to_vec(),
            stderr: stderr.to_vec(),
        }
    }

    fn write_executable_script(name: &str, body: &str) -> PathBuf {
        let path = env::temp_dir().join(format!(
            "hypr-magic-{name}-{}-{}.sh",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, body).unwrap();
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[test]
    fn extract_text_returns_concatenated_text_parts() {
        let body = json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"text": "Hello"},
                        {"text": " world"}
                    ]
                }
            }]
        });

        assert_eq!(extract_text(&body).unwrap(), "Hello world");
    }

    #[test]
    fn extract_text_errors_when_candidates_missing() {
        let body = json!({});
        assert!(extract_text(&body).unwrap_err().contains("Missing candidates"));
    }

    #[test]
    fn extract_text_errors_for_thinking_only_response() {
        let body = json!({
            "candidates": [{
                "content": {
                    "parts": [
                        {"thoughtSignature": "abc123"},
                        {"text": ""}
                    ]
                }
            }]
        });

        assert!(extract_text(&body).unwrap_err().contains("empty text"));
    }

    #[test]
    fn escape_xml_escapes_all_special_characters() {
        let input = r#"<tag attr="fish & chips">it's ok</tag>"#;
        let escaped = escape_xml(input);
        assert_eq!(
            escaped,
            "&lt;tag attr=&quot;fish &amp; chips&quot;&gt;it&apos;s ok&lt;/tag&gt;"
        );
    }

    #[test]
    fn default_gemini_thinking_level_uses_minimal_for_flash_lite() {
        assert_eq!(
            default_gemini_thinking_level("gemini-3.1-flash-lite-preview"),
            "minimal"
        );
    }

    #[test]
    fn default_gemini_thinking_level_uses_low_for_other_models() {
        assert_eq!(
            default_gemini_thinking_level("gemini-3-flash-preview"),
            "low"
        );
    }

    #[test]
    fn gemini_generation_config_builds_thinking_config_for_gemini_three() {
        let config = test_config("gemini-3.1-flash-lite-preview", "minimal");
        let generation = gemini_generation_config(&config).unwrap();
        assert_eq!(
            generation,
            json!({"thinkingConfig": {"thinkingLevel": "minimal"}})
        );
    }

    #[test]
    fn gemini_generation_config_is_none_for_non_gemini_three_model() {
        let config = test_config("gemini-2.5-flash", "minimal");
        assert!(gemini_generation_config(&config).is_none());
    }

    #[test]
    fn gemini_generation_config_is_none_for_empty_thinking_level() {
        let config = test_config("gemini-3.1-flash-lite-preview", "");
        assert!(gemini_generation_config(&config).is_none());
    }

    #[test]
    fn build_gemini_request_uses_streaming_endpoint_and_xml_escaped_raw_text() {
        let config = test_config("gemini-3.1-flash-lite-preview", "minimal");
        let (endpoint, payload) = build_gemini_request(&config, r#"<tag>fish & "chips"</tag>"#);

        assert_eq!(
            endpoint,
            "https://generativelanguage.googleapis.com/v1beta/models/gemini-3.1-flash-lite-preview:streamGenerateContent?alt=sse"
        );
        assert_eq!(
            payload["contents"][0]["parts"][0]["text"].as_str(),
            Some("<raw>&lt;tag&gt;fish &amp; &quot;chips&quot;&lt;/tag&gt;</raw>")
        );
        assert_eq!(
            payload["generationConfig"],
            json!({"thinkingConfig": {"thinkingLevel": "minimal"}})
        );
    }

    #[test]
    fn consume_sse_stream_emits_text_chunks_from_fixture() {
        let fixture = include_str!("../tests/fixtures/gemini_stream_ok.txt");
        let cancel_flag = AtomicBool::new(false);
        let mut chunks = Vec::new();

        consume_sse_stream(Cursor::new(fixture), &cancel_flag, |text| {
            chunks.push(text);
            true
        })
        .unwrap();

        assert_eq!(chunks, vec!["Hello".to_string(), " world".to_string()]);
    }

    #[test]
    fn consume_sse_stream_errors_when_fixture_contains_no_text() {
        let fixture = include_str!("../tests/fixtures/gemini_stream_empty.txt");
        let cancel_flag = AtomicBool::new(false);

        let err = consume_sse_stream(Cursor::new(fixture), &cancel_flag, |_| true).unwrap_err();
        assert!(err.contains("empty text"));
    }

    #[test]
    fn consume_sse_stream_errors_on_invalid_json_chunk() {
        let fixture = include_str!("../tests/fixtures/gemini_stream_invalid.txt");
        let cancel_flag = AtomicBool::new(false);

        let err = consume_sse_stream(Cursor::new(fixture), &cancel_flag, |_| true).unwrap_err();
        assert!(err.contains("Invalid stream JSON"));
    }

    #[test]
    fn consume_sse_stream_stops_cleanly_when_cancelled_mid_stream() {
        let fixture = include_str!("../tests/fixtures/gemini_stream_ok.txt");
        let cancel_flag = AtomicBool::new(false);
        let mut chunks = Vec::new();

        consume_sse_stream(Cursor::new(fixture), &cancel_flag, |text| {
            chunks.push(text);
            cancel_flag.store(true, Ordering::Relaxed);
            true
        })
        .unwrap();

        assert_eq!(chunks, vec!["Hello".to_string()]);
    }

    #[test]
    fn consume_sse_stream_stops_when_receiver_is_gone() {
        let fixture = include_str!("../tests/fixtures/gemini_stream_ok.txt");
        let cancel_flag = AtomicBool::new(false);
        let mut calls = 0;

        consume_sse_stream(Cursor::new(fixture), &cancel_flag, |_| {
            calls += 1;
            false
        })
        .unwrap();

        assert_eq!(calls, 1);
    }

    #[test]
    fn history_can_undo_and_redo_track_single_level_state() {
        assert!(!history_can_undo(&None));
        assert!(!history_can_redo(&None));

        let polished = Some(PolishHistory::new("raw".to_string(), "polished".to_string()));
        assert!(history_can_undo(&polished));
        assert!(!history_can_redo(&polished));

        let mut original_history = PolishHistory::new("raw".to_string(), "polished".to_string());
        original_history.undo();
        let original = Some(original_history);
        assert!(!history_can_undo(&original));
        assert!(history_can_redo(&original));
    }

    #[test]
    fn polish_history_transitions_between_polished_and_original() {
        let mut history = Some(PolishHistory::new("raw".to_string(), "polished".to_string()));

        assert_eq!(undo_polish_history(&mut history).as_deref(), Some("raw"));
        assert!(!history_can_undo(&history));
        assert!(history_can_redo(&history));

        assert_eq!(redo_polish_history(&mut history).as_deref(), Some("polished"));
        assert!(history_can_undo(&history));
        assert!(!history_can_redo(&history));
    }

    #[test]
    fn polish_history_rejects_repeat_undo_and_redo() {
        let mut history = Some(PolishHistory::new("raw".to_string(), "polished".to_string()));

        assert_eq!(undo_polish_history(&mut history).as_deref(), Some("raw"));
        assert_eq!(undo_polish_history(&mut history), None);

        assert_eq!(redo_polish_history(&mut history).as_deref(), Some("polished"));
        assert_eq!(redo_polish_history(&mut history), None);
    }

    #[test]
    fn polish_history_can_be_cleared_after_user_edit() {
        let mut history = Some(PolishHistory::new("raw".to_string(), "polished".to_string()));

        assert!(clear_polish_history(&mut history));
        assert!(history.is_none());
        assert!(!clear_polish_history(&mut history));
    }

    #[test]
    fn set_polish_result_overwrites_previous_history() {
        let mut history = Some(PolishHistory::new("first".to_string(), "first+".to_string()));

        set_polish_result(&mut history, "second".to_string(), "second+".to_string());

        let current = history.as_ref().unwrap();
        assert_eq!(current.original, "second");
        assert_eq!(current.polished, "second+");
        assert!(current.showing_polished);
    }

    #[test]
    fn summarize_output_prefers_stderr_then_stdout_then_default() {
        assert_eq!(summarize_output(&test_output(b"", b"stderr")), "stderr");
        assert_eq!(summarize_output(&test_output(b"stdout", b"")), "stdout");
        assert_eq!(summarize_output(&test_output(b"", b"")), "no output");
    }

    #[test]
    fn next_recording_path_uses_temp_dir_and_wav_suffix() {
        let path = next_recording_path();
        assert!(path.starts_with(env::temp_dir()));
        let name = path.file_name().unwrap().to_string_lossy();
        assert!(name.starts_with("hypr-magic-"));
        assert!(name.ends_with(".wav"));
    }

    #[test]
    fn ensure_voice_config_rejects_missing_or_nonexistent_models() {
        let missing = test_config("gemini-3.1-flash-lite-preview", "minimal");
        assert!(ensure_voice_config(&missing).unwrap_err().contains("set WHISPER_MODEL_PATH"));

        let mut nonexistent = test_config("gemini-3.1-flash-lite-preview", "minimal");
        nonexistent.whisper_model_path = Some("/tmp/definitely-missing-whisper-model.bin".to_string());
        assert!(ensure_voice_config(&nonexistent)
            .unwrap_err()
            .contains("does not exist"));
    }

    #[test]
    fn ensure_voice_config_accepts_existing_model_path() {
        let path = env::temp_dir().join(format!(
            "hypr-magic-test-model-{}.bin",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&path, b"test").unwrap();

        let mut config = test_config("gemini-3.1-flash-lite-preview", "minimal");
        config.whisper_model_path = Some(path.to_string_lossy().to_string());

        let result = ensure_voice_config(&config);
        let _ = fs::remove_file(&path);
        assert!(result.is_ok());
    }

    #[test]
    fn start_recording_reports_missing_pw_record_command() {
        let mut config = test_config("gemini-3.1-flash-lite-preview", "minimal");
        config.pw_record_cmd = "/tmp/definitely-missing-pw-record".to_string();

        let err = start_recording(&config).unwrap_err();
        assert!(err.contains("PW_RECORD_CMD"));
    }

    #[test]
    fn finalize_recording_and_transcribe_uses_stubbed_processes_and_cleans_up() {
        let recorder = write_executable_script(
            "pw-record",
            r#"#!/usr/bin/env bash
set -euo pipefail
outfile="${@: -1}"
printf 'RIFFfake' > "$outfile"
trap 'exit 0' INT
while true; do
  sleep 1
done
"#,
        );
        let whisper = write_executable_script(
            "whisper",
            r#"#!/usr/bin/env bash
set -euo pipefail
wav=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "-f" ]; then
    wav="$2"
    shift 2
    continue
  fi
  shift
done
[ -f "$wav" ] || exit 91
printf 'transcribed text\n'
"#,
        );
        let model = env::temp_dir().join(format!(
            "hypr-magic-test-model-{}.bin",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&model, b"test-model").unwrap();

        let mut config = test_config("gemini-3.1-flash-lite-preview", "minimal");
        config.pw_record_cmd = recorder.to_string_lossy().to_string();
        config.whisper_cmd = whisper.to_string_lossy().to_string();
        config.whisper_model_path = Some(model.to_string_lossy().to_string());

        let state = start_recording(&config).unwrap();
        let wav_path = state.wav_path.clone();
        let deadline = Instant::now() + Duration::from_secs(1);
        while !wav_path.exists() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        assert!(wav_path.exists());

        let result = finalize_recording_and_transcribe(&config, state);

        let _ = fs::remove_file(&recorder);
        let _ = fs::remove_file(&whisper);
        let _ = fs::remove_file(&model);

        assert_eq!(result.unwrap(), "transcribed text");
        assert!(!wav_path.exists());
    }

    #[test]
    fn transcribe_audio_reports_subprocess_failure() {
        let whisper = write_executable_script(
            "whisper-fail",
            r#"#!/usr/bin/env bash
set -euo pipefail
printf 'bad transcript path' >&2
exit 42
"#,
        );
        let model = env::temp_dir().join(format!(
            "hypr-magic-test-model-{}.bin",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let wav = env::temp_dir().join(format!(
            "hypr-magic-test-audio-{}.wav",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(&model, b"test-model").unwrap();
        fs::write(&wav, b"RIFFfake").unwrap();

        let mut config = test_config("gemini-3.1-flash-lite-preview", "minimal");
        config.whisper_cmd = whisper.to_string_lossy().to_string();
        config.whisper_model_path = Some(model.to_string_lossy().to_string());

        let err = transcribe_audio(&config, &wav).unwrap_err();

        let _ = fs::remove_file(&whisper);
        let _ = fs::remove_file(&model);
        let _ = fs::remove_file(&wav);

        assert!(err.contains("exited with"));
        assert!(err.contains("bad transcript path"));
    }

    #[test]
    fn load_config_reads_env_and_defaults() {
        let _guard = env_lock().lock().unwrap();

        unsafe {
            env::remove_var("GEMINI_API_KEY");
            env::remove_var("GEMINI_MODEL");
            env::remove_var("GEMINI_THINKING_LEVEL");
            env::remove_var("PW_RECORD_CMD");
            env::remove_var("WHISPER_CMD");
            env::remove_var("WHISPER_MODEL_PATH");
            env::remove_var("WHISPER_LANG");
        }

        let defaults = load_config().unwrap();
        assert!(defaults.gemini_api_key.is_none());
        assert_eq!(defaults.gemini_model, DEFAULT_GEMINI_MODEL);
        assert_eq!(defaults.gemini_thinking_level, DEFAULT_GEMINI_THINKING_LEVEL);
        assert_eq!(defaults.pw_record_cmd, DEFAULT_PW_RECORD_CMD);
        assert_eq!(defaults.whisper_cmd, DEFAULT_WHISPER_CMD);
        assert!(defaults.whisper_model_path.is_none());
        assert_eq!(defaults.whisper_lang, DEFAULT_WHISPER_LANG);

        unsafe {
            env::set_var("GEMINI_API_KEY", "abc");
            env::set_var("GEMINI_MODEL", "gemini-3-flash-preview");
            env::set_var("GEMINI_THINKING_LEVEL", "low");
            env::set_var("PW_RECORD_CMD", "/tmp/pw-record-test");
            env::set_var("WHISPER_CMD", "/tmp/whisper-test");
            env::set_var("WHISPER_MODEL_PATH", "/tmp/test-model.bin");
            env::set_var("WHISPER_LANG", "fr");
        }

        let configured = load_config().unwrap();
        assert_eq!(configured.gemini_api_key.as_deref(), Some("abc"));
        assert_eq!(configured.gemini_model, "gemini-3-flash-preview");
        assert_eq!(configured.gemini_thinking_level, "low");
        assert_eq!(configured.pw_record_cmd, "/tmp/pw-record-test");
        assert_eq!(configured.whisper_cmd, "/tmp/whisper-test");
        assert_eq!(
            configured.whisper_model_path.as_deref(),
            Some("/tmp/test-model.bin")
        );
        assert_eq!(configured.whisper_lang, "fr");

        unsafe {
            env::remove_var("GEMINI_API_KEY");
            env::remove_var("GEMINI_MODEL");
            env::remove_var("GEMINI_THINKING_LEVEL");
            env::remove_var("PW_RECORD_CMD");
            env::remove_var("WHISPER_CMD");
            env::remove_var("WHISPER_MODEL_PATH");
            env::remove_var("WHISPER_LANG");
        }
    }
}
