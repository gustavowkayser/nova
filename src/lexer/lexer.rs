use std::fs::File;
use std::io::{self, BufRead, BufReader};
use regex;

use crate::lexer::token::Token;
use crate::lexer::token_types::TokenType;

pub struct Lexer;

impl Lexer {
    pub fn new() -> Self {
        Lexer
    }

    fn read_lines(file_path: &str) -> io::Result<std::io::Lines<BufReader<File>>> {
        let file = File::open(file_path);
        let reader = BufReader::new(file?);

        Ok(reader.lines())
    }

    fn clear(file_path: &str) -> io::Result<Vec<String>> {
        let lines = Self::read_lines(file_path)?;
        let mut new_lines = Vec::new();

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

            // Write the line
            new_lines.push(line.to_string());
        }

        Ok(new_lines)
    }

    pub fn tokenize(&self, file_path: &str) -> io::Result<Vec<Token>> {
        let new_lines = Self::clear(file_path)?;

        let mut tokens = Vec::new();

        // Base URL regex
        let base_url_regex = regex::Regex::new(r"https?://[^\s]+").unwrap();

        for line in new_lines {
            // Check for base URL
            if base_url_regex.is_match(&line) {
                let token = Token::new(TokenType::BaseUrl, Some(line.trim().to_string()));
                tokens.push(token);
                continue;
            }

            let words = line.split_whitespace();
            for word in words {
                match word {
                    "{" => {
                        let token = Token::new(TokenType::LBracket, None);
                        tokens.push(token);
                    }
                    "}" => {
                        let token = Token::new(TokenType::RBracket, None);
                        tokens.push(token);
                    }
                    "GET" | "POST" | "PUT" | "DELETE" | "PATCH" => {
                        let token = Token::new(TokenType::HttpMethod, Some(word.to_string()));
                        tokens.push(token);
                    }
                    _ => {
                        // Handle other cases if needed
                    }
                }
            }
        }

        Ok(tokens)
    }
}