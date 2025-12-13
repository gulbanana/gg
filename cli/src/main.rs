#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use clap::Parser;

fn main() -> anyhow::Result<()> {
    let args = gg::Args::parse();
    gg::run(args)
}
