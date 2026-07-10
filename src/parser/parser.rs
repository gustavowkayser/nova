use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use regex;

use crate::parser::token::Token;

pub struct Parser;

impl Parser {
    pub fn new() -> Self {
        Parser
    }

    fn read_lines(file_path: &str) -> io::Result<std::io::Lines<BufReader<File>>> {
        let file = File::open(file_path);
        let reader = BufReader::new(file?);

        Ok(reader.lines())
    }

    fn clear(file_path: &str) -> io::Result<String> {
        let lines = Self::read_lines(file_path)?;
        let output_path = file_path.replace(".nova", ".novac");
        let output_file = File::create(&output_path)?;
        let mut writer = BufWriter::new(output_file);

        for line in lines {
            let line = line?;

            // Remove blank lines
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            // Remove comments
            if line.starts_with("#") {
                continue;
            }

            if line.contains("#") {
                let parts: Vec<&str> = line.splitn(2, '#').collect();
                let line = parts[0].trim();
                if line.is_empty() {
                    continue;
                }
                writeln!(writer, "{}", line)?;
                continue;
            }

            // Write the line to the output file
            writeln!(writer, "{}", line)?;
        }

        Ok(output_path)
    }

    pub fn parse(&self, file_path: &str) -> io::Result<Vec<Token>> {
        let output_path = Self::clear(file_path)?;

        let lines = Self::read_lines(&output_path)?;
        let mut ast = Vec::new();

        // Base URL regex
        let base_url_regex = regex::Regex::new(r"https?://[^\s]+").unwrap();

        for line in lines {
            let line = line?;

            // Check for base URL
            if base_url_regex.is_match(&line) {
                let token = Token::new("BASE_URL", line.trim());
                ast.push(token);
            }
        }

        Ok(ast)
    }
}