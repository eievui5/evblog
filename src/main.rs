use std::cmp::Ordering;
use std::ffi::OsStr;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::exit;
use clap::Parser;
use markdown::Options;
use markdown::CompileOptions;
use toml::value::Date;

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

fn date_to_english(date: &Date) -> String {
	format!(
		"{} {}{}, {}",
		match date.month {
			1 => "January",
			2 => "February",
			3 => "March",
			4 => "April",
			5 => "May",
			6 => "June",
			7 => "July",
			8 => "August",
			9 => "September",
			10 => "October",
			11 => "November",
			12 => "December",
			_ => "",
		},
		date.day,
		match date.day % 10 {
			1 => "st",
			2 => "nd",
			3 => "rd",
			_ => "th",
		},
		date.year,
	)
}

#[derive(Debug)]
struct Metadata {
	title: Option<String>,
	tags: Vec<String>,
	publish_date: Option<Date>,
	file_name: PathBuf,
}

impl Metadata {
	fn new() -> Self {
		Self {
			title: None,
			tags: Vec::new(),
			publish_date: None,
			file_name: PathBuf::new(),
		}
	}

	fn from_toml(toml: String) -> Self {
		if toml.len() == 0 { return Metadata::new(); }

		let mut metadata = Metadata::new();

		let toml = match toml.parse::<toml::Table>() {
			Ok(toml) => toml,
			Err(err) => {
				eprintln!("Failed to read TOML: {err}");
				return metadata;
			}
		};

		if let Some(toml::Value::String(title)) = &toml.get("title") {
			metadata.title = Some(title.clone());
		}

		if let Some(toml::Value::Datetime(datetime)) = &toml.get("published") {
			metadata.publish_date = datetime.date;
		}

		if let Some(toml::Value::Array(tags)) = &toml.get("tags") {
			for tag in tags {
				if let Some(tag) = tag.as_str() {
					metadata.tags.push(tag.to_string());
				}
			}
		}

		metadata
	}
}

#[derive(Debug)]
struct Tag {
	name: String,
	description: String,
}

#[derive(Debug)]
struct IndexConfig {
	title: String,
	tags: Vec<Tag>,
}

impl IndexConfig {
	fn new() -> Self {
		Self {
			title: String::new(),
			tags: Vec::new(),
		}
	}

	fn open(path: &Path) -> Self {
		let mut config = IndexConfig::new();

		match fs::read_to_string(path) {
			Ok(toml) => {
				match toml.parse::<toml::Table>() {
					Ok(table) => {
						if let Some(title) = table
							.get("title")
							.map_or(None, |s| s.as_str())
						{
							config.title = title.to_string();
						}
						if let Some(tags) = table
							.get("tag")
							.map_or(None, |s| s.as_array())
						{
							for tag in tags {
								if let Some(tag_table) = tag.as_table() {
									config.tags.push(Tag {
										name: tag_table
											.get("name")
											.map_or(None, |s| s.as_str())
											.unwrap_or("")
											.to_string(),
										description: tag_table
											.get("description")
											.map_or(None, |s| s.as_str())
											.unwrap_or("")
											.to_string(),
									});
								}
							}
						}
					}
					Err(err) => eprintln!("Failed to parse {}: {}", path.display(), err),
				}
			}
			Err(err) => eprintln!("Failed to open {}: {}", path.display(), err),
		}

		config
	}
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
) -> Metadata {
	// Oh how I long for let/else in stable Rust.
	let document = match fs::read_to_string(&infile) {
		// Like seriously how'd it take this long. This is so silly...
		Ok(document) => document,
		Err(err) => {
			eprintln!("Failed to read {}: {}", infile.display(), err);
			exit(1)
		}
	};

	let mut metadata = String::new();

	if document.starts_with("<!-- metadata") {
		let mut line_iter = document.split("\n");
		line_iter.next();
		for line in line_iter {
			if line == "-->" { break; }
			metadata += line;
			metadata += "\n";
		}
	}

	let mut metadata = Metadata::from_toml(metadata);
	metadata.file_name = outfile.file_name().unwrap().into();

	let mut html = String::new();

	// Prologue
	optional_file_concat(&mut html, cli.prologue.as_deref()).unwrap();

	// Title
	if let Some(title) = &metadata.title {
		html += &format!("<h1><center> {title} </center></h1>\n");
	}

	// Body
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

	// Epilogue
	optional_file_concat(&mut html, cli.epilogue.as_deref()).unwrap();

	if let Err(err) = fs::write(&outfile, html) {
		eprintln!("Failed to write to {}: {}", outfile.display(), err);
		exit(1)
	}

	metadata
}

fn main() {
	let cli = Cli::parse();

	if fs::metadata(&cli.input).unwrap().is_dir() {
		let mut article_data = Vec::<Metadata>::new();

		for entry in fs::read_dir(&cli.input).unwrap() {
			if let Err(_) = entry { continue; }
			let entry = entry.unwrap();
			if entry.path().extension() != Some(&OsStr::new("md")) { continue; }

			let input = entry.path();
			let mut output = entry.path().to_path_buf();
			output.set_extension("html");
			
			let metadata = convert_document(&cli, &input, &output);
			article_data.push(metadata);
		}

		article_data.sort_by(|b, a| {
			let (a_date, b_date) = match (a.publish_date, b.publish_date) {
				(Some(_), None) => return Ordering::Greater,
				(None, Some(_)) => return Ordering::Less,
				(None, None) => return Ordering::Equal,
				(Some(a_date), Some(b_date)) => (a_date, b_date),
			};

			let year_cmp = a_date.year.cmp(&b_date.year);
			if year_cmp != Ordering::Equal { return year_cmp; }
			let month_cmp = a_date.month.cmp(&b_date.month);
			if month_cmp != Ordering::Equal { return month_cmp; }
			a_date.day.cmp(&b_date.day)
		});

		if let Some(index_config_path) = &cli.index {
			let index_config = IndexConfig::open(index_config_path);
			let mut index_md = String::new();

			index_md += &format!("# <center> {} </center>\n", index_config.title);

			for tag in index_config.tags {
				index_md += &format!("## {}\n{}\n", tag.name, tag.description);

				for article in article_data.iter().filter(|a| a.tags.contains(&tag.name)) {
					let title = if let Some(title) = &article.title {
						title
					} else {
						continue;
					};
					index_md += &format!("- [{title}]({})", article.file_name.display());
					if let Some(date) = article.publish_date {
						index_md += &format!("<br>{}", date_to_english(&date));
					}
					index_md += "\n";
				};
			}

			let mut index_md_path = cli.input.to_path_buf();
			index_md_path.push("index.md");
			let mut index_html_path = cli.input.to_path_buf();
			index_html_path.push("index.html");

			if let Err(err) = fs::write(&index_md_path, index_md) {
				eprintln!("Failed to write to {}: {}", index_md_path.display(), err);
				exit(1)
			}

			convert_document(&cli, &index_md_path, &index_html_path);
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
