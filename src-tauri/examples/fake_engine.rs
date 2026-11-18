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
        "disk_full" => {
            eprintln!("av_interleaved_write_frame(): No space left on device");
            std::process::exit(1);
        }
        // print the value of environment variable args[2]
        "print_env" => println!("{}", std::env::var(&args[2]).unwrap_or_default()),
        "hang_after_progress" => {
            println!("out_time_us=100000");
            out.lock().flush().unwrap();
            hang();
        }
        "hang" => hang(),
        // first instruction: drop a marker file at args[2] (proves the process has run)
        "marker" => {
            std::fs::write(&args[2], b"ran").unwrap();
        }
        // starts a grandchild that hangs, reports its pid (stdout, and the file args[2] if
        // given: written whole via a rename so a reader never sees half a number), then
        // hangs itself
        "spawn_child" => {
            #[allow(clippy::zombie_processes)] // deliberate: the test needs an orphan grandchild
            let child = Command::new(std::env::current_exe().unwrap())
                .arg("hang")
                .spawn()
                .unwrap();
            println!("child={}", child.id());
            if let Some(path) = args.get(2) {
                let tmp = format!("{path}.tmp");
                std::fs::write(&tmp, child.id().to_string()).unwrap();
                std::fs::rename(&tmp, path).unwrap();
            }
            out.lock().flush().unwrap();
            hang();
        }
        // 20k stderr lines: a runner that doesn't drain stderr deadlocks here
        "chatty" => {
            for i in 0..20_000 {
                eprintln!(
                    "stderr line {i} ................................................................"
                );
            }
            println!("progress=end");
        }
        // write args[2] and a folder args[3] holding one image (pandoc --extract-media)
        "write_media" => {
            std::fs::write(&args[2], b"![](media/x.png)").unwrap();
            std::fs::create_dir_all(std::path::Path::new(&args[3]).join("media")).unwrap();
            std::fs::write(
                std::path::Path::new(&args[3]).join("media").join("x.png"),
                b"png",
            )
            .unwrap();
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
