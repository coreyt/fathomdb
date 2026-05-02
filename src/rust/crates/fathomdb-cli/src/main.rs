use std::env;

fn main() {
    let command = env::args().nth(1);

    match command.as_deref() {
        None | Some("help") | Some("--help") => print_help(),
        Some("doctor") => {
            println!("doctor scaffold: add inspection verbs during rewrite implementation")
        }
        Some("recover") => {
            println!("recover scaffold: add lossy recovery flow during rewrite implementation")
        }
        Some(other) => {
            eprintln!("unknown command: {other}");
            print_help();
            std::process::exit(2);
        }
    }
}

fn print_help() {
    println!("fathomdb-cli 0.6.0 rewrite scaffold");
    println!("available commands:");
    println!("  doctor   rewrite-era operator diagnostics scaffold");
    println!("  recover  rewrite-era lossy recovery scaffold");
}
