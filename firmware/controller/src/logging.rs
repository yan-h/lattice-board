use crate::usb;
use log::{LevelFilter, Metadata, Record};

// A dummy struct to help us write to the pipe using the 'write!' macro
struct LogPipeWriter;
impl core::fmt::Write for LogPipeWriter {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        let _ = usb::LOG_PIPE.try_write(s.as_bytes());
        Ok(())
    }
}

// Logger implementation
struct Logger;
static LOGGER: Logger = Logger;

impl log::Log for Logger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            use core::fmt::Write;
            let _ = write!(LogPipeWriter, "{}: {}\r\n", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}

pub fn init() {
    unsafe {
        log::set_logger_racy(&LOGGER).unwrap();
        log::set_max_level_racy(LevelFilter::Info);
    }
}
