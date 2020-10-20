use anyhow::{bail, Context, Result};
use clap::{crate_authors, crate_description, crate_version, AppSettings, Clap};
use indicatif::ProgressStyle;
use std::iter::IntoIterator;
use std::sync::mpsc;
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;

use crate::cmd::{Clean, List, Push, Verify};

#[derive(Clap, Debug)]
#[clap(
    version=crate_version!(),
    author=crate_authors!(),
    about=crate_description!(),
    global_setting=AppSettings::ColoredHelp,
    global_setting=AppSettings::InferSubcommands)]
pub struct Opts {
    /// Path to configuration folder. Alternatively, ASFA_CONFIG can be set.
    /// [default: ~/.config/asfa]
    #[clap(short = 'c', long = "config")]
    pub config: Option<String>,

    /// Make output more verbose.
    /// Equivalent to loglevels 'debug' and 'trace' if (specified multiple times).
    /// Should not be specified with `--loglevel`.
    #[clap(short, long, parse(from_occurrences))]
    pub verbose: i32,

    /// Make output more quiet.
    /// Equivalent to loglevels 'warn' and 'error' if (specified multiple times).
    /// Should not be specified with `--loglevel`.
    #[clap(short, long, parse(from_occurrences))]
    pub quiet: i32,

    /// Set loglevel. Defaults to 'info' if unset. Should not be specified with `--verbose`.
    #[clap(
        short,
        long,
        possible_values = &["trace", "debug", "info", "warn", "error"],
    )]
    pub loglevel: Option<String>,

    /// Name of remote site to push to. Only relevant if several remote sites are configured.
    /// The default host can be set in config via `default_host`-option.
    #[clap(short = 'H', long)]
    pub host: Option<String>,

    #[clap(subcommand)]
    pub cmd: UserCommand,
}

impl Opts {
    pub fn verify(&self) -> Result<()> {
        match (self.verbose, self.quiet, self.loglevel.as_deref()) {
            (_, 0, None) => Ok(()),
            (0, _, None) => Ok(()),
            (v, q, _) if v + q > 0 => {
                bail!("Cannot specify --verbose and --quiet.");
            }
            (v, _, Some(_)) if v > 0 => {
                bail!("Cannot specify --verbose and --loglevel");
            }
            (_, q, Some(_)) if q > 0 => {
                bail!("Cannot specify --quiet and --loglevel");
            }
            _ => Ok(()),
        }
    }
}

#[derive(Clap, Debug)]
pub enum UserCommand {
    #[clap(name = "clean")]
    Clean(Clean),

    #[clap(name = "list")]
    List(List),

    #[clap(name = "push")]
    Push(Push),

    #[clap(name = "verify")]
    Verify(Verify),
}

/// Progress bar style for file transfers
pub fn style_progress_bar_transfer() -> indicatif::ProgressStyle {
    ProgressStyle::default_bar()
        .template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes} / {total_bytes} \
                   @ {bytes_per_sec} ({eta})",
        )
        .progress_chars("#>-")
}

/// Progress bar style for counts
pub fn style_progress_bar_count() -> indicatif::ProgressStyle {
    ProgressStyle::default_bar()
        .template(
            "{spinner:.green} [{elapsed_precise}] {msg}[{bar:40.cyan/blue}] {pos} / {len} \
                   @ {per_sec} ({eta})",
        )
        .progress_chars("#>-")
}

pub fn spinner() -> indicatif::ProgressBar {
    let bar = indicatif::ProgressBar::new(!0);
    bar.set_style(style_spinner());
    bar
}

pub fn style_spinner() -> indicatif::ProgressStyle {
    ProgressStyle::default_spinner().template("{spinner:.green} {msg}")
}

pub fn draw_boxed<'a, H: AsRef<str>, I: IntoIterator<Item = &'a str>>(
    header: H,
    content: I,
    color_box: &console::Style,
) -> Result<()> {
    let corner_top_left = color_box.apply_to("┌");
    let corner_top_right = color_box.apply_to("┐");
    let corner_bottom_left = color_box.apply_to("└");
    let corner_bottom_right = color_box.apply_to("┘");
    let header_left = color_box.apply_to("┤");
    let header_right = color_box.apply_to("├");

    let content: Vec<&str> = content.into_iter().collect();

    let header_len = console::strip_ansi_codes(header.as_ref()).chars().count();

    let line_len = {
        let content_max = content
            .iter()
            .map(|l| console::strip_ansi_codes(l).chars().count())
            .max()
            .with_context(|| "No lines supplied.")?;

        [60, content_max, header_len + 2]
            .iter()
            .max()
            .unwrap()
            .clone()
    };

    let line_horizontal = |len: usize| color_box.apply_to("─".repeat(len));
    let line_vertical = color_box.apply_to("│");

    let header_raw = format!(
        "{cl}{hl}{hdr}{hr}{fl}{cr}",
        cl = corner_top_left,
        cr = corner_top_right,
        hl = header_left,
        hr = header_right,
        hdr = header.as_ref(),
        fl = line_horizontal(line_len - 2 /* header left/right */ - header_len)
    );
    println!("{}", join_frames(&content[0], &header_raw, '┬'));
    for line in content.iter() {
        let pad_width = line_len - console::strip_ansi_codes(line).chars().count();
        println!(
            "{border}{line}{pad}{border}",
            line = line,
            border = line_vertical,
            pad = " ".repeat(pad_width)
        );
    }
    let last_line_raw = format!(
        "{cl}{l}{cr}",
        cl = corner_bottom_left,
        cr = corner_bottom_right,
        l = line_horizontal(line_len)
    );
    let last_line = join_frames(&content[content.len() - 1], &last_line_raw, '┴');

    println!("{}", last_line);

    Ok(())
}

/// Replace portions in raw that are a horizontal line ('─') with `joiner` where content contains a
/// vertical line ('│').
/// This makes it possible to join frames.
fn join_frames(content: &str, raw: &str, joiner: char) -> String {
    // Make sure any frames drawn in last line are joined
    let mut replacer = text::ColoredTextReplacer::new(raw.to_string());
    let nocolor: Vec<_> = console::strip_ansi_codes(raw).chars().collect();
    for idx_separator in console::strip_ansi_codes(content)
        .chars()
        .enumerate()
        .filter_map(|(idx, c)| if c == '│' { Some(idx) } else { None })
    {
        // replacer.replace_if(idx_separator, '─', joiner);
        let idx = idx_separator + /* frame */ 1;
        if nocolor[idx] == '─' {
            replacer.replace(idx_separator + /* frame */ 1, joiner);
        }
    }
    replacer.get()
}

/// Spinner that spins until finish() is called.
pub struct WaitingSpinner {
    handle: thread::JoinHandle<()>,
    stop_token: Arc<Mutex<bool>>,
    tx: mpsc::Sender<SpinnerSetting>,
}

enum SpinnerSetting {
    Println(String),
    Message(String),
}

impl WaitingSpinner {
    pub fn new(message: String) -> Self {
        let (tx, rx) = channel();
        let stop_token = Arc::new(Mutex::new(false));
        let stop_token_pbar = Arc::clone(&stop_token);
        let handle = thread::spawn(move || {
            let spinner = crate::cli::spinner();
            spinner.set_message(&message);

            let handle_messages = || {
                while let Ok(msg) = rx.try_recv() {
                    match msg {
                        SpinnerSetting::Message(msg) => spinner.set_message(&msg),
                        SpinnerSetting::Println(msg) => spinner.println(&msg),
                    }
                }
            };

            while !*stop_token_pbar.lock().unwrap() {
                handle_messages();
                spinner.inc(1);
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            handle_messages();
            spinner.inc(1);
            spinner.finish_and_clear();
        });
        Self {
            handle,
            stop_token,
            tx,
        }
    }

    pub fn set_message(&self, message: String) -> Result<()> {
        self.tx.send(SpinnerSetting::Message(message))?;
        Ok(())
    }

    pub fn println(&self, msg: String) -> Result<()> {
        self.tx.send(SpinnerSetting::Println(msg))?;
        Ok(())
    }

    pub fn finish(self) {
        *self.stop_token.lock().unwrap() = true;
        self.handle.join().unwrap();
    }
}

#[allow(non_upper_case_globals)]
pub mod color {

    use console::Style;

    lazy_static::lazy_static! {
        pub static ref dot : Style = Style::new().cyan();
        pub static ref entry : Style = Style::new();
        pub static ref failure : Style = Style::new().red().bright();
        pub static ref filename : Style = Style::new().blue().bright();
        pub static ref frame : Style = Style::new().blue();
        pub static ref success : Style = Style::new().green().bright();
    }
}

#[allow(non_upper_case_globals)]
pub mod text {
    use std::collections::HashMap;

    pub fn separator() -> String {
        format!("{}", super::color::frame.apply_to("│"))
    }

    /// Helper to replace text in colored text
    pub struct ColoredTextReplacer {
        original: String,
        nocolor: String,
        replacements: HashMap<usize, char>,
    }

    impl ColoredTextReplacer {
        pub fn new(original: String) -> Self {
            let nocolor = console::strip_ansi_codes(&original).to_string();
            let replacements = HashMap::new();

            Self {
                original,
                nocolor,
                replacements,
            }
        }

        /// Replace the char at position `idx` with replacement. The index counts chars in the
        /// colorless variant.
        pub fn replace(&mut self, idx: usize, replacement: char) -> &mut Self {
            self.replacements.insert(idx, replacement);
            self
        }

        /// Get the replaced string
        pub fn get(&self) -> String {
            let mut iter_nocolor = self.nocolor.chars().enumerate().peekable();

            self.original
                .chars()
                .map(|char_orig| {
                    let current_nocolor = iter_nocolor.peek().cloned();
                    match current_nocolor {
                        Some((idx_strip, char_nocolor)) if char_nocolor == char_orig => {
                            iter_nocolor.next();
                            self.replacements
                                .get(&idx_strip)
                                .unwrap_or(&char_orig)
                                .clone()
                        }
                        _ => char_orig,
                    }
                })
                .collect()
        }
    }
}
