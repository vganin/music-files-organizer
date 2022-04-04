use console::Term;
use indicatif::{ProgressBar, ProgressStyle};
use itertools::Itertools;

#[macro_export]
macro_rules! console_print {
    ($console:expr, $($arg:tt)*) => ({
        $console.println(&format!($($arg)*));
    })
}

pub struct Console {
    pbs: Vec<ProgressBar>,
}

impl Console {
    pub fn new() -> Self {
        Self {
            pbs: Vec::new(),
        }
    }

    pub fn println(&self, str: &str) {
        match self.pbs.iter().find_or_first(|pb| !pb.is_hidden() && !pb.is_finished()) {
            Some(pb) => pb.println(str),
            None => Term::stdout().write_line(str).unwrap()
        }
    }

    pub fn new_default_progress_bar(&mut self, len: u64) -> ProgressBar {
        let pb = ProgressBar::new(len);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.red/yellow} [{elapsed_precise}] [{bar:50.red/yellow}] {bytes}/{total_bytes} {wide_msg:.bold.dim}")
                .progress_chars(":: ")
                .tick_strings(TICK_STRINGS)
        );
        self.configure_progress_bar(pb)
    }

    pub fn new_default_spinner(&mut self) -> ProgressBar {
        let pb = ProgressBar::new(!0);
        pb.set_style(
            ProgressStyle::default_bar()
                .tick_strings(TICK_STRINGS)
                .template("{spinner:.red/yellow} [{elapsed_precise}] {wide_msg:.bold.dim}")
        );
        self.configure_progress_bar(pb)
    }

    fn configure_progress_bar(&mut self, pb: ProgressBar) -> ProgressBar {
        pb.enable_steady_tick(PROGRESS_TICK_MS);
        self.pbs.push(pb.clone());
        pb
    }
}

const PROGRESS_TICK_MS: u64 = 80u64;
const TICK_STRINGS: &'static [&str] = &[
    "⠋",
    "⠙",
    "⠹",
    "⠸",
    "⠼",
    "⠴",
    "⠦",
    "⠧",
    "⠇",
    "⠏"
];
