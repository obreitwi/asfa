use anyhow::{Context, Result};
use clap::{crate_authors, crate_description, crate_version, AppSettings, Clap};
use indicatif::ProgressStyle;
use std::iter::IntoIterator;

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

pub fn draw_boxed<'a, I: IntoIterator<Item = &'a str>>(
    header: &'a str,
    content: I,
    color_line: &console::Style,
    color_box: &console::Style,
) -> Result<()> {
    let corner_top_left = color_box.apply_to("┌");
    let corner_top_right = color_box.apply_to("┐");
    let corner_bottom_left = color_box.apply_to("└");
    let corner_bottom_right = color_box.apply_to("┘");
    let header_left = color_box.apply_to("┤");
    let header_right = color_box.apply_to("├");

    let content: Vec<&str> = content.into_iter().collect();

    let line_len = {
        let content_max = content
            .iter()
            .map(|l| console::strip_ansi_codes(l).len())
            .max()
            .with_context(|| "No lines supplied.")?;

        [60, content_max + 2, header.len() + 2]
            .iter()
            .max()
            .unwrap()
            .clone()
    };

    let line_horizontal = |len: usize| color_box.apply_to("─".repeat(len));
    let line_vertical = color_box.apply_to("│");

    println!(
        "{cl}{hl}{hdr}{hr}{fl}{cr}",
        cl = corner_top_left,
        cr = corner_top_right,
        hl = header_left,
        hr = header_right,
        hdr = header,
        fl = line_horizontal(line_len - 2 /* header left/right */ - header.len())
    );
    for line in content.iter() {
        let pad_width = line_len-console::strip_ansi_codes(line).len() - 2 /* padding */;
        println!(
            "{border} {line}{pad} {border}",
            line = line,
            border = line_vertical,
            pad = " ".repeat(pad_width)
        );
    }
    println!(
        "{cl}{l}{cr}",
        cl = corner_bottom_left,
        cr = corner_bottom_right,
        l = line_horizontal(line_len)
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
