//! Word frequency counter pipeline
//!
//! Reads lines from stdin, splits into words, counts frequencies, and prints top-N words.
//!
//! Usage: cargo run --example word_count --release
//!        (Then type lines of text and press Ctrl-D to finish)

use std::collections::HashMap;
use std::io::{self, BufRead};
use std::time::Duration;
use stream_pipeline::{OverflowPolicy, PipelineBuilder, Stage, Result as PipelineResult};

/// Stage that reads lines and produces words
struct LineReaderStage;

impl LineReaderStage {
    fn new() -> Self {
        Self
    }
}

impl Stage for LineReaderStage {
    fn process(&mut self, input: Vec<u8>) -> PipelineResult<Vec<Vec<u8>>> {
        let text = String::from_utf8_lossy(&input);
        let words: Vec<Vec<u8>> = text
            .split_whitespace()
            .map(|w| w.to_lowercase().into_bytes())
            .collect();
        Ok(words)
    }

    fn name(&self) -> &str {
        "line_reader"
    }
}

/// Stage that cleans and normalizes words
struct WordCleanerStage;

impl Stage for WordCleanerStage {
    fn process(&mut self, input: Vec<u8>) -> PipelineResult<Vec<Vec<u8>>> {
        let word = String::from_utf8_lossy(&input);
        let cleaned: String = word
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect();

        if cleaned.len() > 2 {
            Ok(vec![cleaned.into_bytes()])
        } else {
            Ok(vec![])
        }
    }

    fn name(&self) -> &str {
        "word_cleaner"
    }
}

/// Stage that counts words and periodically outputs top words
struct WordCounterStage {
    counts: HashMap<String, usize>,
    batch_count: usize,
    batch_size: usize,
}

impl WordCounterStage {
    fn new() -> Self {
        Self {
            counts: HashMap::new(),
            batch_count: 0,
            batch_size: 100,
        }
    }

    fn get_top_n(&self, n: usize) -> Vec<(String, usize)> {
        let mut items: Vec<_> = self.counts.iter().map(|(k, v)| (k.clone(), *v)).collect();
        items.sort_by(|a, b| b.1.cmp(&a.1));
        items.into_iter().take(n).collect()
    }
}

impl Stage for WordCounterStage {
    fn process(&mut self, input: Vec<u8>) -> PipelineResult<Vec<Vec<u8>>> {
        let word = String::from_utf8_lossy(&input);
        *self.counts.entry(word.to_string()).or_insert(0) += 1;
        self.batch_count += 1;

        if self.batch_count % self.batch_size == 0 {
            let top_10 = self.get_top_n(10);
            println!("\n=== Top 10 Words (after {} words) ===", self.batch_count);
            for (i, (word, count)) in top_10.iter().enumerate() {
                println!("{:2}. {} ({})", i + 1, word, count);
            }
        }

        Ok(vec![])
    }

    fn name(&self) -> &str {
        "word_counter"
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Word Frequency Counter Pipeline");
    println!("================================");
    println!("Enter lines of text (Ctrl-D to finish):");
    println!();

    let pipeline = PipelineBuilder::new()
        .add_stage("reader", 100, OverflowPolicy::Block)
        .add_stage("cleaner", 200, OverflowPolicy::Block)
        .add_stage("counter", 50, OverflowPolicy::Block)
        .with_backpressure(true)
        .build()?;

    let input = pipeline.input();

    // Spawn a thread to read stdin
    let input_clone = input.clone();
    let reader_thread = std::thread::spawn(move || {
        let stdin = io::stdin();
        let reader = stdin.lock();
        for line in reader.lines() {
            if let Ok(line) = line {
                let _ = input_clone.push(line.into_bytes());
            } else {
                break;
            }
        }
    });

    let running = pipeline.start(|stage_idx| match stage_idx {
        0 => Box::new(LineReaderStage::new()),
        1 => Box::new(WordCleanerStage),
        2 => Box::new(WordCounterStage::new()),
        _ => Box::new(WordCleanerStage),
    })?;

    // Wait for input reading to finish
    reader_thread.join().expect("Reader thread panicked");

    // Give pipeline time to process remaining items
    std::thread::sleep(Duration::from_millis(500));

    running.wait()?;

    println!("\n\nProcessing complete!");

    Ok(())
}
