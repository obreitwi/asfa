use clap::{crate_authors, crate_description, crate_version, AppSettings, Clap};

use anyhow::{Context, Result};

use indicatif::ProgressStyle;

use crate::cmd::{Clean, List, Push};

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

    /// Set loglevel.
    #[clap(
        short,
        long,
        default_value = "info",
        possible_values = &["trace", "debug", "info", "warn", "error"],
    )]
    pub loglevel: String,

    /// Name of remote site to push to. Only relevant if several remote sites are configured.
    /// The default host can be set in config via `default_host`-option.
    #[clap(short = 'H', long)]
    pub host: Option<String>,

    #[clap(subcommand)]
    pub cmd: UserCommand,
}

#[derive(Clap, Debug)]
pub enum UserCommand {
    #[clap(name = "clean")]
    Clean(Clean),

    #[clap(name = "list")]
    List(List),

    #[clap(name = "push")]
    Push(Push),
}

pub fn style_progress_bar() -> indicatif::ProgressStyle {
    ProgressStyle::default_bar()
        .template(
            "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes} / {total_bytes} \
                   @ {bytes_per_sec} ({eta})",
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

pub fn draw_boxed(
    text: &str,
    color_line: &console::Style,
    color_box: &console::Style,
) -> Result<()> {
    let corner_top_left = color_box.apply_to("┌");
    let corner_top_right = color_box.apply_to("┐");
    let corner_bottom_left = color_box.apply_to("└");
    let corner_bottom_right = color_box.apply_to("┘");

    let line_len_max = text
        .lines()
        .map(|l| l.len())
        .max()
        .with_context(|| "No lines supplied.")?;
    let length_horizontal = line_len_max + 2;

    let line_horizontal = color_box.apply_to("─".repeat(length_horizontal));
    let line_vertical = color_box.apply_to("│");

    println!("{}{}{}", corner_top_left, line_horizontal, corner_top_right);
    for line in text.lines() {
        println!("{box} {line:<width$} {box}", line=color_line.apply_to(line), box=line_vertical, width=line_len_max);
    }
    println!(
        "{}{}{}",
        corner_bottom_left, line_horizontal, corner_bottom_right
    );

    Ok(())
}

#[allow(non_upper_case_globals)]
pub mod color {

    use console::Style;

    lazy_static::lazy_static! {
        // static ref heading : Style = console::Style::new().cyan().bright().bold();
        // static ref frame : Style = console::Style::new().magenta();
        pub static ref heading : Style = console::Style::new();
        pub static ref frame : Style = console::Style::new().blue();
        pub static ref entry : Style = console::Style::new().red().bright();
        pub static ref dot : Style = console::Style::new().cyan();
    }
}
