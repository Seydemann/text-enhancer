use std::env;
use std::sync::mpsc;
use std::time::Duration;

use glib::ControlFlow;
use gtk::gdk;
use gtk::prelude::*;
use gtk::{
    Application, ApplicationWindow, Box as GtkBox, Button, Label, Orientation, ScrolledWindow,
    TextView, WindowHandle,
};
use gtk4 as gtk;
use reqwest::blocking::Client;
use serde_json::{Value, json};

const APP_ID: &str = "com.seydemann.hyprmagic";
const DEFAULT_MODEL: &str = "gemini-3-flash-preview";
const GEMINI_LOGO_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/assets/gemini-logo.svg");
const SYSTEM_PROMPT: &str = "Linguistically polish the text in <raw>. Preserve the author's voice, personality, tone, and style -- elevate, never replace. Find more idiomatic phrasing where natural, eliminate bloat, and fix grammatical, lexical, and punctuation errors. Do not restructure, reconstruct, or re-engineer the content. Every edit should unseal, not substitute. Where the author reached for a phrase and fell short, complete the arch they were already building -- reveal what they were on the verge of writing, never impose what they weren't. The goal is not correction but emancipation: widen the bottleneck between thought and expression until what arrives on the page is proportionate to what was luminous in the mind. Surgical, enriching changes only.";

#[derive(Clone, Debug)]
struct AppConfig {
    api_key: String,
    model: String,
}

fn main() {
    let app = Application::builder().application_id(APP_ID).build();

    app.connect_activate(|app| {
        let config = match load_config() {
            Ok(c) => c,
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
    let api_key = env::var("GEMINI_API_KEY").map_err(|_| {
        "Missing GEMINI_API_KEY. Set it in your shell before starting the app.".to_string()
    })?;

    let model = env::var("GEMINI_MODEL").unwrap_or_else(|_| DEFAULT_MODEL.to_string());

    Ok(AppConfig { api_key, model })
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
      background: transparent;
      border: none;
      box-shadow: none;
      padding: 0;
    }

    .icon-label {
      color: #eaf0ff;
      font-size: 24px;
      font-weight: 700;
    }

    .icon-picture {
      margin: 0;
    }

    .magic-btn {
      font-weight: 700;
      background: #14378a;
      color: #f6f8ff;
    }

    .status-label {
      color: #9aa3bd;
      font-size: 12px;
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
    // Floating icon window
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
    // Make the editor behave like a floating utility tied to the icon.
    editor_window.set_transient_for(Some(&icon_window));

    let root = GtkBox::new(Orientation::Vertical, 10);
    root.set_margin_top(12);
    root.set_margin_bottom(12);
    root.set_margin_start(12);
    root.set_margin_end(12);

    let header = Label::new(Some("Write or paste text, then click Magic to polish."));
    header.set_xalign(0.0);

    let scroller = ScrolledWindow::builder()
        .hexpand(true)
        .vexpand(true)
        .build();

    let text_view = TextView::new();
    text_view.set_wrap_mode(gtk::WrapMode::WordChar);
    text_view.set_monospace(false);
    scroller.set_child(Some(&text_view));

    let actions = GtkBox::new(Orientation::Horizontal, 8);
    let magic_button = Button::with_label("Magic");
    magic_button.add_css_class("magic-btn");
    let copy_button = Button::with_label("Copy");
    let clear_button = Button::with_label("Clear");

    let status = Label::new(Some("Idle"));
    status.set_xalign(0.0);
    status.add_css_class("status-label");

    actions.append(&magic_button);
    actions.append(&copy_button);
    actions.append(&clear_button);

    root.append(&header);
    root.append(&scroller);
    root.append(&actions);
    root.append(&status);

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
        let status = status.clone();
        let magic_button = magic_button.clone();
        let magic_button_for_handler = magic_button.clone();
        let config = config.clone();

        magic_button.connect_clicked(move |_| {
            let buffer = text_view.buffer();
            let start = buffer.start_iter();
            let end = buffer.end_iter();
            let input = buffer.text(&start, &end, false).to_string();

            if input.trim().is_empty() {
                status.set_text("Scratchpad is empty.");
                return;
            }

            status.set_text("Polishing...");
            magic_button_for_handler.set_sensitive(false);
            let (tx, rx) = mpsc::channel::<Result<String, String>>();
            let config = config.clone();
            std::thread::spawn(move || {
                let _ = tx.send(polish_text(&config, &input));
            });

            let text_view = text_view.clone();
            let status = status.clone();
            let magic_button_for_handler = magic_button_for_handler.clone();
            glib::timeout_add_local(Duration::from_millis(50), move || match rx.try_recv() {
                Ok(result) => {
                    match result {
                        Ok(polished) => {
                            text_view.buffer().set_text(&polished);
                            status.set_text("Done");
                        }
                        Err(err) => {
                            status.set_text(&format!("Error: {err}"));
                        }
                    }
                    magic_button_for_handler.set_sensitive(true);
                    ControlFlow::Break
                }
                Err(mpsc::TryRecvError::Empty) => ControlFlow::Continue,
                Err(mpsc::TryRecvError::Disconnected) => {
                    status.set_text("Error: worker thread disconnected");
                    magic_button_for_handler.set_sensitive(true);
                    ControlFlow::Break
                }
            });
        });
    }

    editor_window.hide();
    icon_window.present();
}

fn polish_text(config: &AppConfig, raw: &str) -> Result<String, String> {
    let endpoint = format!(
        "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent",
        config.model
    );

    let payload = json!({
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

    let client = Client::builder()
        .timeout(Duration::from_secs(90))
        .build()
        .map_err(|e| format!("HTTP client init failed: {e}"))?;

    let response = client
        .post(endpoint)
        .header("Content-Type", "application/json")
        .header("x-goog-api-key", &config.api_key)
        .json(&payload)
        .send()
        .map_err(|e| format!("Request failed: {e}"))?;

    let status = response.status();
    let body: Value = response
        .json()
        .map_err(|e| format!("Invalid API response JSON: {e}"))?;

    if !status.is_success() {
        let message = body
            .get("error")
            .and_then(|v| v.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("Unknown API error");
        return Err(format!("Gemini API error ({}): {message}", status.as_u16()));
    }

    extract_text(&body)
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

fn escape_xml(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
