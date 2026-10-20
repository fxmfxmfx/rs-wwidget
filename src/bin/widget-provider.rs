#[path = "../widget_plugins/mod.rs"]
mod widgets;

fn main() {
    if let Err(error) = run() {
        eprintln!("widget-provider: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let name = args
        .first()
        .ok_or("usage: widget-provider NAME [--package DIR]")?
        .clone();
    args.remove(0);
    let package = if let Some(index) = args.iter().position(|s| s == "--package") {
        if index + 1 >= args.len() {
            return Err("--package requires a directory".into());
        }
        let path = std::path::PathBuf::from(args.remove(index + 1));
        args.remove(index);
        path
    } else {
        std::env::current_exe()?
            .parent()
            .ok_or("missing executable directory")?
            .join(&name)
    };
    if !args.is_empty() {
        return Err("unexpected widget arguments".into());
    }
    match name.as_str() {
        "network" => widgets::network::run(&package),
        "temperatures" => widgets::temperatures::run(&package),
        "weather" => widgets::weather::run(&package),
        _ => Err(format!("unknown provider: {name}").into()),
    }
}
