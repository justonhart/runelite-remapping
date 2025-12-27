use std::io::{self, Read, Write};
use std::process::Command;
use std::time::{Duration, Instant};

const RUNELITE_CLASS: &str = "net-runelite-client-RuneLite";
const FOCUS_CHECK_INTERVAL: Duration = Duration::from_millis(100);

// Linux input event structure
#[repr(C)]
#[derive(Debug, Clone, Copy)]
struct InputEvent {
    time: libc::timeval,
    type_: u16,
    code: u16,
    value: i32,
}

// Input event types and codes
const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;

const SYN_REPORT: u16 = 0;
const REL_X: u16 = 0x00;
const REL_Y: u16 = 0x01;
const REL_HWHEEL: u16 = 0x06;

const BTN_SIDE: u16 = 0x113;
const BTN_EXTRA: u16 = 0x114;
const BTN_10: u16 = 282;
const KEY_ESC: u16 = 1;
const KEY_BACKSPACE: u16 = 14;
const KEY_SPACE: u16 = 57;
const KEY_LEFTSHIFT: u16 = 42;
const KEY_1: u16 = 2;
const KEY_2: u16 = 3;
const KEY_F5: u16 = 63;

struct FocusChecker {
    last_check: Instant,
    cached_focus: bool,
    sway_socket: Option<String>,
}

impl FocusChecker {
    fn new() -> Self {
        FocusChecker {
            last_check: Instant::now() - FOCUS_CHECK_INTERVAL,
            cached_focus: false,
            sway_socket: Self::find_sway_socket(),
        }
    }

    fn find_sway_socket() -> Option<String> {
        let uid = unsafe { libc::getuid() };
        let pattern = format!("/run/user/{}/sway-ipc.*.sock", uid);
        
        glob::glob(&pattern)
            .ok()?
            .filter_map(Result::ok)
            .next()
            .and_then(|path| path.to_str().map(String::from))
    }

    fn is_runelite_focused(&mut self) -> bool {
        let now = Instant::now();
        
        // Use cached value if checked recently
        if now.duration_since(self.last_check) < FOCUS_CHECK_INTERVAL {
            return self.cached_focus;
        }
        
        self.last_check = now;
        
        let socket = match &self.sway_socket {
            Some(s) => s,
            None => {
                self.cached_focus = false;
                return false;
            }
        };

        let output = Command::new("sh")
            .arg("-c")
            .arg(format!(
                "SWAYSOCK={} swaymsg -t get_tree 2>/dev/null | jq -r '.. | select(.focused? == true) | .app_id // .window_properties.class' 2>/dev/null",
                socket
            ))
            .output();

        match output {
            Ok(output) => {
                let result = String::from_utf8_lossy(&output.stdout);
                self.cached_focus = result.contains(RUNELITE_CLASS);
                self.cached_focus
            }
            Err(_) => {
                self.cached_focus = false;
                false
            }
        }
    }
}

struct EventEmitter {
    stdout: io::Stdout,
}

impl EventEmitter {
    fn new() -> Self {
        EventEmitter {
            stdout: io::stdout(),
        }
    }

    fn emit(&mut self, type_: u16, code: u16, value: i32) -> io::Result<()> {
        let event = InputEvent {
            time: unsafe { std::mem::zeroed() },
            type_,
            code,
            value,
        };
        
        let bytes = unsafe {
            std::slice::from_raw_parts(
                &event as *const _ as *const u8,
                std::mem::size_of::<InputEvent>(),
            )
        };
        
        self.stdout.write_all(bytes)?;
        self.stdout.flush()
    }

    fn emit_event(&mut self, event: &InputEvent) -> io::Result<()> {
        let bytes = unsafe {
            std::slice::from_raw_parts(
                event as *const _ as *const u8,
                std::mem::size_of::<InputEvent>(),
            )
        };
        
        self.stdout.write_all(bytes)?;
        self.stdout.flush()
    }

    fn emit_key_press(&mut self, key: u16) -> io::Result<()> {
        self.emit(EV_KEY, key, 1)?;
        self.emit(EV_SYN, SYN_REPORT, 0)?;
        self.emit(EV_KEY, key, 0)?;
        self.emit(EV_SYN, SYN_REPORT, 0)
    }
}

fn main() -> io::Result<()> {
    let mut focus_checker = FocusChecker::new();
    let mut emitter = EventEmitter::new();
    let mut stdin = io::stdin();
    let mut event_buffer = vec![0u8; std::mem::size_of::<InputEvent>()];

    loop {
        // Read event from stdin
        match stdin.read_exact(&mut event_buffer) {
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                break;
            }
            Err(e) => {
                return Err(e);
            }
        }

        let mut event: InputEvent = unsafe { std::ptr::read(event_buffer.as_ptr() as *const _) };

        // Skip mouse movement events
        if event.type_ == EV_REL && (event.code == REL_X || event.code == REL_Y) {
            emitter.emit_event(&event)?;
            continue;
        }

        // Apply remappings if RuneLite is focused
        if focus_checker.is_runelite_focused() {
            if event.type_ == EV_KEY {
                match event.code {
                    BTN_SIDE => {
                        event.code = KEY_ESC;
                    }
                    BTN_EXTRA => {
                        event.code = KEY_SPACE;
                    }
                    BTN_10 => {
                        event.code = KEY_LEFTSHIFT;
                    }
                    _ => {}
                }
            } else if event.type_ == EV_REL && event.code == REL_HWHEEL {
                if event.value < 0 {
                    emitter.emit_key_press(KEY_BACKSPACE)?;
                } else {
                    emitter.emit_key_press(KEY_F5)?;
                }
                continue;
            }
        }

        emitter.emit_event(&event)?;
    }

    Ok(())
}
