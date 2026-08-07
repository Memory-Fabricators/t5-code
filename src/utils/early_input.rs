//! Port of `src/utils/earlyInput.ts`.
//!
//! Captures terminal input typed before the REPL is ready.

use std::io::{self, IsTerminal, Read};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};

static EARLY_INPUT_BUFFER: OnceLock<Mutex<String>> = OnceLock::new();
static IS_CAPTURING: AtomicBool = AtomicBool::new(false);
static CAPTURE_THREAD: OnceLock<Mutex<Option<JoinHandle<()>>>> = OnceLock::new();

fn buffer() -> &'static Mutex<String> {
    EARLY_INPUT_BUFFER.get_or_init(|| Mutex::new(String::new()))
}

fn capture_thread_slot() -> &'static Mutex<Option<JoinHandle<()>>> {
    CAPTURE_THREAD.get_or_init(|| Mutex::new(None))
}

/// Start capturing stdin data early, before the REPL is initialized.
pub fn start_capturing_early_input() {
    let args: Vec<String> = std::env::args().collect();
    if !io::stdin().is_terminal()
        || IS_CAPTURING.load(Ordering::SeqCst)
        || args.iter().any(|a| a == "-p" || a == "--print")
    {
        return;
    }

    IS_CAPTURING.store(true, Ordering::SeqCst);
    if let Ok(mut b) = buffer().lock() {
        b.clear();
    }

    // Best-effort byte capture without raw mode (raw mode arrives with ratatui/REPL).
    // Spawns a reader thread that drains stdin while capturing is active.
    let handle = thread::spawn(|| {
        let mut stdin = io::stdin();
        let mut buf = [0u8; 256];
        while IS_CAPTURING.load(Ordering::SeqCst) {
            // Non-blocking-ish: read may block until first bytes; that's OK for early capture.
            match stdin.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    let chunk = String::from_utf8_lossy(&buf[..n]);
                    process_chunk(&chunk);
                }
                Err(_) => break,
            }
        }
    });

    if let Ok(mut slot) = capture_thread_slot().lock() {
        *slot = Some(handle);
    }
}

fn process_chunk(str: &str) {
    let mut i = 0;
    let chars: Vec<char> = str.chars().collect();
    while i < chars.len() {
        let ch = chars[i];
        let code = ch as u32;

        // Ctrl+C
        if code == 3 {
            stop_capturing_early_input();
            std::process::exit(130);
        }

        // Ctrl+D
        if code == 4 {
            stop_capturing_early_input();
            return;
        }

        // Backspace
        if code == 127 || code == 8 {
            if let Ok(mut b) = buffer().lock() {
                b.pop();
            }
            i += 1;
            continue;
        }

        // Escape sequences
        if code == 27 {
            i += 1;
            while i < chars.len() {
                let c = chars[i] as u32;
                if (64..=126).contains(&c) {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }

        // Skip other control characters (except tab and newline)
        if code < 32 && code != 9 && code != 10 && code != 13 {
            i += 1;
            continue;
        }

        // CR → LF
        if code == 13 {
            if let Ok(mut b) = buffer().lock() {
                b.push('\n');
            }
            i += 1;
            continue;
        }

        if let Ok(mut b) = buffer().lock() {
            b.push(ch);
        }
        i += 1;
    }
}

/// Stop capturing early input.
pub fn stop_capturing_early_input() {
    if !IS_CAPTURING.swap(false, Ordering::SeqCst) {
        return;
    }
    // Leave the reader thread to exit on next loop check; do not join (may block on read).
}

/// Consume any early input that was captured.
pub fn consume_early_input() -> String {
    stop_capturing_early_input();
    let mut b = buffer().lock().unwrap_or_else(|e| e.into_inner());
    let input = b.trim().to_string();
    b.clear();
    input
}

/// Check if there is any early input available without consuming it.
pub fn has_early_input() -> bool {
    buffer()
        .lock()
        .map(|b| !b.trim().is_empty())
        .unwrap_or(false)
}

/// Seed the early input buffer with text that will appear pre-filled.
pub fn seed_early_input(text: &str) {
    if let Ok(mut b) = buffer().lock() {
        *b = text.to_string();
    }
}

/// Check if early input capture is currently active.
pub fn is_capturing_early_input() -> bool {
    IS_CAPTURING.load(Ordering::SeqCst)
}
