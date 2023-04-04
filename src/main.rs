use std::ffi::OsStr;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;
use clap::Parser;
use markdown::Options;
use markdown::CompileOptions;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
	/// Path to prologue file.
	#[arg(short, long)]
	prologue: Option<PathBuf>,
	
	/// Path to epilogue file.
	#[arg(short, long)]
	epilogue: Option<PathBuf>,
	
	/// Path to output file. If not provided, an html file is produced adjacent to the input.
	#[arg(short, long)]
	output: Option<PathBuf>,

	/// Path to index file. Creates a page listing all articles in the current directory.
	#[arg(short, long)]
	index: Option<PathBuf>,

	/// Path to tag file. Describes how to generate the index.
	#[arg(short, long)]
	tags: Option<PathBuf>,

	/// Path to markdown document, or directory containing markdown documents.
	input: PathBuf,
}

fn optional_file_concat(out: &mut String, path: Option<impl AsRef<Path>>) -> Result<(), Box<dyn Error>> {
	if let Some(path) = path {
		*out += &fs::read_to_string(path)?;
	}
	Ok(())
}

fn convert_document(
	cli: &Cli,
	infile: &Path,
	outfile: &Path,
) {
	// Oh how I long for let/else in stable Rust.
	let document = match fs::read_to_string(&infile) {
		// Like seriously how'd it take this long. This is so silly...
		Ok(document) => document,
		Err(err) => {
			eprintln!("Failed to read {}: {}", infile.display(), err);
			exit(1)
		}
	};

	let mut html = String::new();
	optional_file_concat(&mut html, cli.prologue.as_deref()).unwrap();
	html += &markdown::to_html_with_options(
		&document,
		&Options {
			compile: CompileOptions {
			  allow_dangerous_html: true,
			  ..CompileOptions::default()
			},
			..Options::gfm()
		}
	).unwrap();
	optional_file_concat(&mut html, cli.epilogue.as_deref()).unwrap();

	if let Err(err) = fs::write(&outfile, html) {
		eprintln!("Failed to write to {}: {}", outfile.display(), err);
		exit(1)
	}
}

fn main() {
	let cli = Cli::parse();

	if fs::metadata(&cli.input).unwrap().is_dir() {
		for entry in fs::read_dir(&cli.input).unwrap() {
			if let Err(_) = entry { continue; }
			let entry = entry.unwrap();
			if entry.path().extension() != Some(&OsStr::new("md")) { continue; }
			let input = entry.path();
			let mut output = entry.path().to_path_buf();
			output.set_extension("html");
			convert_document(&cli, &input, &output);
		}
	} else {
		let output = if let Some(ref output) = cli.output {
			output.to_path_buf()
		} else {
			let mut output = cli.input.to_path_buf();
			output.set_extension("html");
			output
		};

		convert_document(&cli, &cli.input, &output);
	}
}
