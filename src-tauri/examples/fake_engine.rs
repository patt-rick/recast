//! Test double for engine processes. `cargo test` builds it into target/<profile>/examples/.
//! Usage: fake_engine <mode> [args…]
use std::io::Write;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

fn hang() -> ! {
    loop {
        sleep(Duration::from_secs(1));
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let out = std::io::stdout();
    match args.get(1).map(String::as_str).unwrap_or("ok") {
        // ffmpeg-style progress, then write "converted" to args[2]
        "ok" => {
            for i in 0..5 {
                let mut o = out.lock();
                writeln!(o, "out_time_us={}", i * 200_000).unwrap();
                writeln!(o, "progress=continue").unwrap();
                o.flush().unwrap();
                sleep(Duration::from_millis(40));
            }
            println!("progress=end");
            if let Some(p) = args.get(2) {
                std::fs::write(p, b"converted").unwrap();
            }
        }
        "fail" => {
            eprintln!("Invalid data found when processing input");
            std::process::exit(3);
        }
        "hang_after_progress" => {
            println!("out_time_us=100000");
            out.lock().flush().unwrap();
            hang();
        }
        "hang" => hang(),
        // starts a grandchild that hangs, reports its pid, then hangs itself
        "spawn_child" => {
            let child = Command::new(std::env::current_exe().unwrap()).arg("hang").spawn().unwrap();
            println!("child={}", child.id());
            out.lock().flush().unwrap();
            hang();
        }
        // 20k stderr lines: a runner that doesn't drain stderr deadlocks here
        "chatty" => {
            for i in 0..20_000 {
                eprintln!("stderr line {i} ................................................................");
            }
            println!("progress=end");
        }
        // write N bytes to args[2]
        "write_out" => std::fs::write(&args[2], vec![0u8; args[3].parse().unwrap()]).unwrap(),
        // write two pages into the folder args[2]
        "write_dir" => {
            std::fs::write(std::path::Path::new(&args[2]).join("page-1.txt"), b"1").unwrap();
            std::fs::write(std::path::Path::new(&args[2]).join("page-2.txt"), b"2").unwrap();
        }
        other => {
            eprintln!("unknown mode {other}");
            std::process::exit(64);
        }
    }
}
