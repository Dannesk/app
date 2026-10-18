

#[cfg(not(feature = "startup-trace"))]
mod imp {
    #[inline(always)]
    pub fn stamp(_stage: &'static str) {}

    #[inline(always)]
    pub fn watch_libraries() {}

    #[inline(always)]
    pub fn watch_data() {}

    #[inline(always)]
    pub fn first_frame() {}

    #[inline(always)]
    pub fn first_frame_pending() -> bool {
        false
    }
}

#[cfg(feature = "startup-trace")]
mod imp {
    use std::fmt::Write as _;
    use std::io::Write as _;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Mutex, OnceLock};
    use std::time::{Duration, Instant};

    struct Trace {
        started: Instant,
        seen: Mutex<Vec<&'static str>>,
        file: PathBuf,
    }

    static TRACE: OnceLock<Trace> = OnceLock::new();
    static FIRST_FRAME: AtomicBool = AtomicBool::new(false);

    fn trace() -> &'static Trace {
        TRACE.get_or_init(|| {
            let started = Instant::now();
            let file = dirs::runtime_dir()
                .unwrap_or_else(std::env::temp_dir)
                .join(format!("dannesk-startup-trace-{}.txt", std::process::id()));

            let mut head = format!("Dannesk {} start-up trace, pid {}\n", crate::VERSION, std::process::id());
            match since_process_start() {
                Some(ms) => {
                    let _ = writeln!(head, "process start to main: {ms:.0} ms (10 ms steps)");
                }
                None => head.push_str("process start to main: unknown\n"),
            }
            for var in ["WGPU_BACKEND", "VK_DRIVER_FILES", "VK_ICD_FILENAMES", "WAYLAND_DISPLAY", "DISPLAY"] {
                if let Some(value) = std::env::var_os(var) {
                    let _ = writeln!(head, "{var}={}", value.to_string_lossy());
                }
            }
            head.push_str(
                "\n  wall ms  thread  its cpu ms  its disk faults  process disk faults  process MB read  stage\n",
            );
            let _ = std::fs::write(&file, head);
            let _ = dannesk_core::ws::TRACE.set(stamp);

            Trace { started, seen: Mutex::new(Vec::new()), file }
        })
    }

    pub fn stamp(stage: &'static str) {
        let trace = trace();
        let wall = trace.started.elapsed().as_secs_f64() * 1e3;
        {
            let mut seen = trace.seen.lock().expect("startup trace");
            if seen.contains(&stage) {
                return;
            }
            seen.push(stage);
        }
        let thread = usage(libc::RUSAGE_THREAD);
        let process = usage(libc::RUSAGE_SELF);
        let who = if std::thread::current().name() == Some("main") { "main" } else { "side" };
        append(
            &trace.file,
            &format!(
                "{wall:>9.1}  {who:<6}  {:>10.1}  {:>15}  {:>19}  {:>15.1}  {stage}\n",
                thread.cpu_ms, thread.disk_faults, process.disk_faults, process.read_mb,
            ),
        );
    }

   
    pub fn watch_data() {
        use crate::channel::CHANNEL;
        use crate::utils::price;
        let _ = std::thread::Builder::new().name("startup-trace-data".into()).spawn(|| {
            let give_up = Instant::now() + Duration::from_secs(30);
            let mut left: Vec<(&'static str, fn() -> bool)> = vec![
                ("proxy reports rates up", || *CHANNEL.rates_ws_status_rx.borrow()),
                ("first price in the app", || !CHANNEL.rates_rx.borrow().is_empty()),
                ("XRP and BTC priced", || price::usd("XRP") > 0.0 && price::usd("BTC") > 0.0),
                ("XRP wallet known", || CHANNEL.wallet_balance_rx.borrow().1.is_some()),
                ("BTC wallet known", || CHANNEL.bitcoin_wallet_rx.borrow().1.is_some()),
                ("wallet files read", || CHANNEL.loaded_rx.borrow().wallets),
                ("XRP balance in", || CHANNEL.loaded_rx.borrow().xrp),
                ("BTC balance in", || CHANNEL.loaded_rx.borrow().btc),
            ];
            while !left.is_empty() && Instant::now() < give_up {
                left.retain(|&(stage, reached)| {
                    if reached() {
                        stamp(stage);
                        false
                    } else {
                        true
                    }
                });
                std::thread::sleep(Duration::from_millis(5));
            }
        });
    }

    pub fn first_frame() {
        stamp("first frame on screen");
        FIRST_FRAME.store(true, Ordering::Release);
    }

    pub fn first_frame_pending() -> bool {
        !FIRST_FRAME.load(Ordering::Acquire)
    }

  
    pub fn watch_libraries() {
        let _ = std::thread::Builder::new().name("startup-trace".into()).spawn(|| {
            let trace = trace();
            let mut seen: Vec<(f64, String)> = Vec::new();
            let give_up = Instant::now() + Duration::from_secs(30);
            loop {
                let last = FIRST_FRAME.load(Ordering::Acquire) || Instant::now() > give_up;
                let at = trace.started.elapsed().as_secs_f64() * 1e3;
                if let Ok(maps) = std::fs::read_to_string("/proc/self/maps") {
                    for line in maps.lines() {
                        let Some(path) = line.find('/').map(|i| &line[i..]) else { continue };
                        if !(path.contains(".so") || path.contains("shader_cache")) {
                            continue;
                        }
                        if !seen.iter().any(|(_, known)| known == path) {
                            seen.push((at, path.to_owned()));
                        }
                    }
                }
                if last {
                    break;
                }
                std::thread::sleep(Duration::from_millis(4));
            }

            let mut out = String::from("\nshared libraries, by when each was first seen mapped (4 ms steps):\n");
            for (at, path) in &seen {
                let mb = std::fs::metadata(path).map(|m| m.len() as f64 / 1e6).unwrap_or(0.0);
                let _ = writeln!(out, "{at:>9.1}  {mb:>7.1} MB  {path}");
            }
            append(&trace.file, &out);
        });
    }

    struct Usage {
        cpu_ms: f64,
        disk_faults: i64,
        read_mb: f64,
    }

    fn usage(who: libc::c_int) -> Usage {
        // SAFETY: an all-zero rusage is a valid value, and getrusage only writes it.
        let mut ru: libc::rusage = unsafe { std::mem::zeroed() };
        unsafe { libc::getrusage(who, &mut ru) };
        let ms = |tv: libc::timeval| tv.tv_sec as f64 * 1e3 + tv.tv_usec as f64 / 1e3;
        Usage {
            cpu_ms: ms(ru.ru_utime) + ms(ru.ru_stime),
            disk_faults: ru.ru_majflt,
            // In 512-byte blocks, whatever the device's block size.
            read_mb: ru.ru_inblock as f64 * 512.0 / 1e6,
        }
    }

    /// Milliseconds from the process's creation to now.
    fn since_process_start() -> Option<f64> {
        let stat = std::fs::read_to_string("/proc/self/stat").ok()?;
        // Field 22 (starttime, in clock ticks since boot). Counted after the
        // command name, which sits in parentheses and may hold spaces.
        let start_ticks: f64 = stat.rsplit_once(')')?.1.split_whitespace().nth(19)?.parse().ok()?;
        // SAFETY: sysconf takes no pointers; clock_gettime writes a valid timespec.
        let hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
        let mut now: libc::timespec = unsafe { std::mem::zeroed() };
        unsafe { libc::clock_gettime(libc::CLOCK_BOOTTIME, &mut now) };
        Some((now.tv_sec as f64 + now.tv_nsec as f64 / 1e9 - start_ticks / hz) * 1e3)
    }

    fn append(file: &Path, text: &str) {
        if let Ok(mut f) = std::fs::OpenOptions::new().append(true).create(true).open(file) {
            let _ = f.write_all(text.as_bytes());
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// One run: each stage once, the start-to-main figure read, and the
        /// library list written when the first frame ends the watch.
        #[test]
        fn a_run_writes_its_stages_and_its_libraries() {
            stamp("main");
            stamp("main");
            watch_libraries();
            first_frame();
            std::thread::sleep(Duration::from_millis(60));

            let file = &trace().file;
            let text = std::fs::read_to_string(file).expect("the trace file");
            println!("{text}");
            assert_eq!(text.matches("  main\n").count(), 1, "a stage is recorded once");
            assert!(text.contains("  first frame on screen\n"));
            assert!(text.contains("process start to main: ") && !text.contains("unknown"));
            assert!(text.contains("libc.so"), "the library list");
            assert!(!first_frame_pending());
            let _ = std::fs::remove_file(file);
        }
    }
}

pub use imp::*;
