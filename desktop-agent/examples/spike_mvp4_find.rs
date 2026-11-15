//! MVP 4 spike, read-only: lists the elements of Apple Music's main window whose name, id or class
//! contains any given text (case-insensitive), with type, patterns, toggle state and position.
//!   cargo run --example spike_mvp4_find -- shuffle repeat queue
//!   cargo run --example spike_mvp4_find -- --all          (every element; long)

#[path = "spike_common/mod.rs"]
mod common;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_lowercase()).collect();
    let ui = common::start();
    let started = std::time::Instant::now();
    let all = common::array_items(&ui.all());
    println!(
        "{} elements in the main window (read in {} ms)",
        all.len(),
        started.elapsed().as_millis()
    );
    let everything = args.iter().any(|a| a == "--all");
    let needles: Vec<&str> = args.iter().map(String::as_str).collect();
    for el in all {
        if everything || common::matches_any(&el, &needles) {
            println!("{}", common::describe(&el));
        }
    }
}
