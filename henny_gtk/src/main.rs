use gtk4::gdk::Display;
use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box as GBox, Button, CssProvider, Entry, Label, ListBox,
    ListBoxRow, Orientation, ScrolledWindow, SelectionMode,
};
use serde::Deserialize;

const BACKEND: &str = "http://127.0.0.1:6969";

const CSS: &str = r#"
window { background: #111; color: #ccc; }
entry  { background: #1e1e1e; color: #eee; border: 1px solid #333; }
list   { background: #111; }
row    { border-bottom: 1px solid #222; padding: 6px 10px; }
row:hover { background: #1a1a1a; }
row label { color: #ccc; }
.err  { color: #f66; }
"#;

#[derive(Debug, Deserialize)]
struct QueryResponse {
    results: Option<Vec<String>>,
    error: Option<String>,
}

fn main() -> glib::ExitCode {
    let app = Application::builder()
        .build();

    app.connect_startup(|_| {
        let provider = CssProvider::new();
        provider.load_from_data(CSS);
        gtk4::style_context_add_provider_for_display(
            &Display::default().expect("no display"),
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });

    app.connect_activate(build_ui);
    app.run()
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("henny")
        .default_width(700)
        .default_height(500)
        .build();

    let root = GBox::new(Orientation::Vertical, 6);
    root.set_margin_top(10);
    root.set_margin_bottom(10);
    root.set_margin_start(10);
    root.set_margin_end(10);

    let search_row = GBox::new(Orientation::Horizontal, 6);
    let entry = Entry::builder()
        .placeholder_text("search documents...")
        .hexpand(true)
        .build();
    let btn = Button::with_label("Search");
    search_row.append(&entry);
    search_row.append(&btn);
    root.append(&search_row);

    let status = Label::new(None);
    status.set_halign(gtk4::Align::Start);
    root.append(&status);

    let list = ListBox::new();
    list.set_selection_mode(SelectionMode::None);

    let scroll = ScrolledWindow::builder()
        .child(&list)
        .vexpand(true)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .build();
    root.append(&scroll);

    window.set_child(Some(&root));

    let status_click = status.clone();
    list.connect_row_activated(move |_, row| {
        let path = row.widget_name().to_string();
        if path.is_empty() { return; }
        let url = format!("{}/file?path={}", BACKEND, url_encode(&path));
        if let Err(e) = std::process::Command::new("xdg-open").arg(&url).spawn() {
            status_click.set_text(&format!("error: {e}"));
        }
    });

    let do_search = {
        let entry = entry.clone();
        let list = list.clone();
        let status = status.clone();

        move || {
            let query = entry.text().trim().to_string();
            if query.is_empty() { return; }

            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            status.set_text("searching...");

            let url = format!("{}/query?query={}&n_result=255", BACKEND, url_encode(&query));
            let list = list.clone();
            let status = status.clone();

            glib::spawn_future_local(async move {
                let t0 = std::time::Instant::now();

                let parsed: Result<QueryResponse, _> = ureq::get(&url)
                    .call()
                    .and_then(|r| Ok(r.into_body().read_json()?));

                let elapsed = t0.elapsed().as_millis();

                match parsed {
                    Err(e) => {
                        status.set_text(&format!("error: {e}"));
                        add_row(&list, &format!("request failed: {e}"), true, None);
                    }
                    Ok(data) => {
                        if let Some(err) = data.error {
                            status.set_text("backend error");
                            add_row(&list, &err, true, None);
                        } else {
                            let results = data.results.unwrap_or_default();
                            let n = results.len();
                            status.set_text(&format!("{n} result(s) — {elapsed}ms"));
                            if n == 0 {
                                add_row(&list, "no results", false, None);
                            } else {
                                for (i, path) in results.iter().enumerate() {
                                    add_row(&list, path, false, Some(i + 1));
                                }
                            }
                        }
                    }
                }
            });
        }
    };

    let do_search2 = do_search.clone();
    btn.connect_clicked(move |_| do_search());
    entry.connect_activate(move |_| do_search2());

    window.present();
}

fn add_row(list: &ListBox, text: &str, is_err: bool, rank: Option<usize>) {
    let row = ListBoxRow::new();
    let label_text = match rank {
        Some(n) => format!("{n:>3}.  {text}"),
        None => text.to_string(),
    };
    let label = Label::new(Some(&label_text));
    if is_err { label.set_css_classes(&["err"]); }
    label.set_halign(gtk4::Align::Start);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    row.set_child(Some(&label));
    if rank.is_some() {
        row.set_widget_name(text); 
        row.set_activatable(true);
    }
    list.append(&row);
}

fn url_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char)
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}
